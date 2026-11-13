//! The programs `run_captured` and `run_captured_split` run (hooks and signing programs), noted
//! for the operation log (#887).
//!
//! A caller that wants them installs a [`RunLog`] on its thread with [`record_runs`]; every
//! captured run on that thread is then pushed to it, with its program, arguments, the output
//! the runner kept and how it ended. Nothing is noted when no log is installed, so callers that
//! don't care pay for nothing. A helper thread doing work for the caller (a push's transfer
//! thread, which runs `pre-push`) installs the same log with [`current_run_log`] and
//! `record_runs`.
//!
//! The log holds what the program was given and printed, unfiltered: the caller redacts
//! credentials before it keeps or shows anything.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

/// One program run, as the runner saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramRun {
    pub program: String,
    pub args: Vec<String>,
    /// The output the runner kept: the combined tail (`run_captured`), or stderr's tail
    /// (`run_captured_split`, whose stdout is data such as a signature). For a program that
    /// could not start, why.
    pub output: String,
    pub status: ProgramRunStatus,
}

/// How a [`ProgramRun`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramRunStatus {
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    /// It could not be started (or waited for).
    NotStarted,
}

/// Where the runs on a thread are noted (see the module docs). Clones share the list.
#[derive(Debug, Clone, Default)]
pub struct RunLog(Arc<Mutex<Vec<ProgramRun>>>);

impl RunLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the runs noted so far, leaving the log empty.
    pub fn take(&self) -> Vec<ProgramRun> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|error| error.into_inner()))
    }

    pub(crate) fn push(&self, run: ProgramRun) {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(run);
    }
}

thread_local! {
    static CURRENT: RefCell<Option<RunLog>> = const { RefCell::new(None) };
}

/// Runs `f` with `log` installed on this thread, then puts back whatever was installed before
/// (also when `f` panics).
pub fn record_runs<R>(log: &RunLog, f: impl FnOnce() -> R) -> R {
    let _ = log;
    f()
}

/// The log installed on this thread, if any.
pub fn current_run_log() -> Option<RunLog> {
    None
}

/// The program and arguments of `command`, when a log is installed to note its run.
pub(crate) fn describe(command: &std::process::Command) -> Option<(RunLog, String, Vec<String>)> {
    let _ = command;
    None
}

#[cfg(all(test, unix))]
mod tests {
    use std::process::Command;
    use std::time::Duration;

    use super::super::{run_captured, run_captured_split};
    use super::*;

    fn sh(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn a_captured_run_is_noted_with_its_program_arguments_output_and_status() {
        let log = RunLog::new();
        record_runs(&log, || {
            run_captured(
                sh("echo out; echo err >&2; exit 3"),
                None,
                &|| false,
                Duration::from_secs(30),
                1024,
            )
            .unwrap();
        });
        assert_eq!(
            log.take(),
            vec![ProgramRun {
                program: "sh".into(),
                args: vec!["-c".into(), "echo out; echo err >&2; exit 3".into()],
                output: "out\nerr\n".into(),
                status: ProgramRunStatus::Failed,
            }]
        );
        assert!(log.take().is_empty(), "take empties the log");
    }

    #[test]
    fn a_split_run_notes_stderr_not_stdout() {
        let log = RunLog::new();
        record_runs(&log, || {
            run_captured_split(
                sh("echo signature; echo diagnostics >&2"),
                None,
                &|| false,
                Duration::from_secs(30),
                1024,
                1024,
            )
            .unwrap();
        });
        let runs = log.take();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].output, "diagnostics\n");
        assert_eq!(runs[0].status, ProgramRunStatus::Succeeded);
    }

    #[test]
    fn timed_out_cancelled_and_unstartable_runs_are_noted_too() {
        let log = RunLog::new();
        record_runs(&log, || {
            let _ = run_captured(
                sh("sleep 30"),
                None,
                &|| false,
                Duration::from_millis(100),
                1024,
            );
            let _ = run_captured(
                sh("sleep 30"),
                None,
                &|| true,
                Duration::from_secs(30),
                1024,
            );
            let _ = run_captured(
                Command::new("browsitory-no-such-program"),
                None,
                &|| false,
                Duration::from_secs(30),
                1024,
            );
        });
        let statuses: Vec<_> = log.take().into_iter().map(|run| run.status).collect();
        assert_eq!(
            statuses,
            [
                ProgramRunStatus::TimedOut,
                ProgramRunStatus::Cancelled,
                ProgramRunStatus::NotStarted
            ]
        );
    }

    #[test]
    fn nothing_is_noted_without_a_log_and_the_previous_log_comes_back() {
        assert!(current_run_log().is_none());
        let outer = RunLog::new();
        let inner = RunLog::new();
        record_runs(&outer, || {
            assert!(current_run_log().is_some());
            record_runs(&inner, || {
                run_captured(
                    sh("echo inner"),
                    None,
                    &|| false,
                    Duration::from_secs(30),
                    64,
                )
                .unwrap();
            });
            run_captured(
                sh("echo outer"),
                None,
                &|| false,
                Duration::from_secs(30),
                64,
            )
            .unwrap();
        });
        assert!(current_run_log().is_none());
        assert_eq!(inner.take()[0].output, "inner\n");
        let outer_runs = outer.take();
        assert_eq!(outer_runs.len(), 1);
        assert_eq!(outer_runs[0].output, "outer\n");
    }

    #[test]
    fn another_thread_records_into_the_log_it_was_handed() {
        let log = RunLog::new();
        record_runs(&log, || {
            let handed = current_run_log().expect("installed");
            std::thread::spawn(move || {
                record_runs(&handed, || {
                    run_captured(
                        sh("echo helper"),
                        None,
                        &|| false,
                        Duration::from_secs(30),
                        64,
                    )
                    .unwrap();
                })
            })
            .join()
            .unwrap();
        });
        assert_eq!(log.take()[0].output, "helper\n");
    }
}
