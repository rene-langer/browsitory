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
//! - **Residual race:** a process the hook starts between `CreateProcess` and
//!   `AssignProcessToJobObject` escapes the job. `sh.exe` (the MSYS2 runtime) takes far longer
//!   than that window to initialise before it can start anything. Closing the window would need
//!   `CREATE_SUSPENDED` plus resuming the primary thread, whose handle std doesn't expose.
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
#[cfg_attr(not(test), allow(dead_code))] // Used by hooks once #180 is fixed.
pub(super) const CREATE_SUSPENDED: u32 = 0x0000_0004;

/// Resumes every thread of a process started with [`CREATE_SUSPENDED`].
pub(super) fn resume(_child: &Child) -> std::io::Result<()> {
    Ok(()) // Not yet implemented (#180): this commit only adds the tests.
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
    fn a_suspended_program_runs_only_after_resume() {
        let mut child = Command::new("cmd")
            .args(["/C", "exit", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED)
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            child.try_wait().unwrap().is_none(),
            "a suspended program ran"
        );
        resume(&child).unwrap();
        assert!(
            exited_within(&mut child, Duration::from_secs(20)),
            "the program never ran after resume"
        );
    }
}
