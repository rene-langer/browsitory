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
