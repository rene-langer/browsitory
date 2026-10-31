//! A Windows job object per hook (#122), so a timeout or cancel kills the whole process tree a
//! hook started, not just its `sh.exe`.
//!
//! Processes started by a process in a job join the job, so the hook shell's children (a
//! husky hook's `node`, an MSYS2 background job) and their children are covered. Nested jobs
//! work since Windows 8, so this also works when Browsitory itself runs inside a job (VSCode,
//! Windows Terminal).
//!
//! - **No `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`** (D6-A): only a timeout or cancel terminates
//!   the job. A background process that a hook leaves behind after exiting normally keeps
//!   running, as on Unix, where a normal exit doesn't kill the process group. The handle is
//!   closed when the run ends, and the job object goes away with its last process.
//! - **No escape before the job (#180):** a hook starts with `CREATE_SUSPENDED`, is assigned to
//!   its job, and only then resumed ([`resume`], a Toolhelp32 walk over its threads, since std
//!   doesn't expose the primary thread's handle). So nothing it starts directly or through its
//!   descendants can run outside the job. Out of reach: processes started on its behalf by a
//!   broker (COM/DCOM activation, Task Scheduler, WMI, a service), which belong to the broker's
//!   tree, and a hook whose job can't be created or assigned (it is still resumed, see the last
//!   point). A hook that can't be resumed is killed at once rather than left suspended until
//!   the time limit.
//! - **Signing programs are not put in a job** (`run_captured_split`): gpg may auto-start
//!   `gpg-agent` during the call, and a job kill on a signing timeout would take the agent down.
//! - If creating or assigning the job fails, the run falls back to killing the direct child.

use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::process::Child;

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
    fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    fn TerminateJobObject(job: *mut c_void, exit_code: u32) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

/// An unnamed job object holding one hook's process tree. Closing it (on drop) does not end the
/// processes in it.
pub(super) struct Job(*mut c_void);

impl Job {
    /// Creates a job and assigns `child` to it. `None` when either step fails.
    pub(super) fn assign(child: &Child) -> Option<Job> {
        // SAFETY: null attributes and name are documented as valid (default security, unnamed).
        let handle = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
        if handle.is_null() {
            return None;
        }
        let job = Job(handle);
        // SAFETY: both handles are valid and open: `job` is owned here and `child` is not yet
        // waited on, so its process handle is still held by `Child`.
        let assigned = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) };
        if assigned == 0 {
            return None; // `job` drops here and closes the handle.
        }
        Some(job)
    }

    /// The raw job handle, for tests that ask Windows about the job.
    #[cfg(test)]
    pub(super) fn handle(&self) -> *mut c_void {
        self.0
    }

    /// Terminates every process in the job (the hook and everything it started).
    pub(super) fn terminate(&self) {
        // SAFETY: `self.0` is a valid job handle until drop. A failure is ignored: the caller
        // still kills the direct child afterwards.
        unsafe {
            TerminateJobObject(self.0, 1);
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by `CreateJobObjectW` and is closed exactly once.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// `CreateProcess` flag: the primary thread starts suspended.
pub(super) const CREATE_SUSPENDED: u32 = 0x0000_0004;

const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
const THREAD_SUSPEND_RESUME: u32 = 0x0002;

/// `THREADENTRY32`.
#[repr(C)]
#[derive(Default)]
struct ThreadEntry32 {
    size: u32,
    usage: u32,
    thread_id: u32,
    owner_process_id: u32,
    base_priority: i32,
    delta_priority: i32,
    flags: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> *mut c_void;
    fn Thread32First(snapshot: *mut c_void, entry: *mut ThreadEntry32) -> i32;
    fn Thread32Next(snapshot: *mut c_void, entry: *mut ThreadEntry32) -> i32;
    fn OpenThread(access: u32, inherit: i32, thread_id: u32) -> *mut c_void;
    fn ResumeThread(thread: *mut c_void) -> u32;
}

/// How often `resume` tries to take a thread snapshot (which can fail transiently, e.g. with
/// `ERROR_BAD_LENGTH` while threads come and go) before giving up.
const SNAPSHOT_ATTEMPTS: usize = 3;

/// A Toolhelp32 snapshot of every thread in the system, retried a few times.
fn thread_snapshot() -> std::io::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    let mut attempt = 1;
    loop {
        // SAFETY: plain call; the result is checked before use.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if !snapshot.is_null() && snapshot != (-1isize) as *mut c_void {
            // SAFETY: a new, valid handle nothing else owns; closed exactly once, on drop.
            return Ok(unsafe { OwnedHandle::from_raw_handle(snapshot) });
        }
        let error = std::io::Error::last_os_error();
        if attempt == SNAPSHOT_ATTEMPTS {
            return Err(error);
        }
        attempt += 1;
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Resumes the threads of a process started with [`CREATE_SUSPENDED`] (std closes the primary
/// thread's handle, so the threads are found with a Toolhelp32 snapshot). A thread that can't be
/// opened or resumed is skipped; an error only when no thread at all was resumed, so the caller
/// never leaves the process suspended.
pub(super) fn resume(child: &Child) -> std::io::Result<()> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    let snapshot = thread_snapshot()?;
    let pid = child.id();
    let mut resumed = 0;
    let mut last_error = None;
    let mut entry = ThreadEntry32 {
        size: std::mem::size_of::<ThreadEntry32>() as u32,
        ..ThreadEntry32::default()
    };
    // SAFETY: the snapshot handle is open; `entry` is a writable `THREADENTRY32` whose `size`
    // field is set, as both calls require.
    let mut more = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) } != 0;
    while more {
        if entry.owner_process_id == pid {
            // SAFETY: plain call; the result is checked before use.
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.thread_id) };
            if thread.is_null() {
                last_error = Some(std::io::Error::last_os_error());
            } else {
                // SAFETY: a new, valid handle nothing else owns; closed exactly once, on drop.
                let thread = unsafe { OwnedHandle::from_raw_handle(thread) };
                // SAFETY: the thread handle is open and has THREAD_SUSPEND_RESUME access.
                if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                    last_error = Some(std::io::Error::last_os_error());
                } else {
                    resumed += 1;
                }
            }
        }
        // SAFETY: as for `Thread32First`.
        more = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) } != 0;
    }
    if resumed == 0 {
        return Err(
            last_error.unwrap_or_else(|| std::io::Error::other("the hook's thread was not found"))
        );
    }
    Ok(())
}

/// Kills and reaps a test's child when dropped, so a failing assertion never leaves a suspended
/// process behind.
#[cfg(test)]
pub(super) struct KillOnDrop(pub(super) Child);

#[cfg(test)]
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Waits up to `limit` for `child` to exit; kills it (so the test never hangs) when it
    /// doesn't.
    fn exited_within(child: &mut Child, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
        false
    }

    #[test]
    fn thread_entry_has_the_documented_layout() {
        assert_eq!(std::mem::size_of::<ThreadEntry32>(), 28);
    }

    #[test]
    fn a_suspended_program_runs_only_after_resume() {
        let mut child = KillOnDrop(
            Command::new("cmd")
                .args(["/C", "exit", "0"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
                .spawn()
                .unwrap(),
        );
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "a suspended program ran"
        );
        resume(&child.0).unwrap();
        assert!(
            exited_within(&mut child.0, Duration::from_secs(20)),
            "the program never ran after resume"
        );
    }
}
