//! Running user-defined commands (#894). A run is looked up by id among the commands stored in
//! the app's config (the repository's own first, then the app-wide ones), so a caller can only
//! run what the user defined, never a command line it sends. The variables are filled on the
//! repository's worker (a quick read); the program then runs on the caller's thread, off the
//! worker, so a long command never holds up the repository's queue.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use git_core::process::CommandStyle;
use git_core::user_command::{self, Context, RunError, OUTPUT_LIMIT, RUN_TIMEOUT};

use git_core::process::{record_runs, RunLog};

use crate::oplog;
use crate::wire::{ErrorDto, UserCommandRunDto};
use crate::worker::ReadHandle;

/// The runs going on, by the caller's run id, and whether a cancel was asked for.
fn runs() -> MutexGuard<'static, HashMap<String, bool>> {
    static RUNS: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    RUNS.get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// Forgets a run id when its run ends, on every path.
struct Registered(String);

impl Drop for Registered {
    fn drop(&mut self) {
        runs().remove(&self.0);
    }
}

/// Asks the run `run_id` to stop. Ignored when no such run is going on (it ended already).
pub fn cancel(run_id: &str) {
    if let Some(cancelled) = runs().get_mut(run_id) {
        *cancelled = true;
    }
}

const NOT_DEFINED: &str = "This command doesn't exist any more.";

/// The stored command `command_id` for `repo_path`: its own commands first, then the app-wide.
fn find(repo_path: &Path, command_id: &str) -> Result<config::UserCommand, ErrorDto> {
    let commands = config::list_user_commands(Some(repo_path)).map_err(ErrorDto::from_error)?;
    find_in(commands, command_id)
}

/// `command_id` among `commands`, the repository's own first.
fn find_in(
    commands: config::UserCommands,
    command_id: &str,
) -> Result<config::UserCommand, ErrorDto> {
    commands
        .repository
        .into_iter()
        .chain(commands.global)
        .find(|command| command.id == command_id)
        .ok_or_else(|| NOT_DEFINED.into())
}

/// A user command ready to run: its arguments and working directory filled in.
#[derive(Debug, Clone)]
pub struct Prepared {
    expanded: user_command::Expanded,
    root: std::path::PathBuf,
    /// The command's name, the target of its operation log entry.
    name: String,
    /// The repository's operation log (`worker::repo_key`); `None` when the path can't be read.
    journal_key: Option<String>,
    /// The command's id and the selection it runs on, for Run again.
    command_id: String,
    context: Context,
}

/// Looks up the stored command `command_id` for `repo_path` and fills its variables from
/// `context` on the repository's worker (`handle`, a quick read). Errors when the command is gone,
/// doesn't parse, or needs a value `context` doesn't give.
pub fn prepare(
    handle: &ReadHandle,
    repo_path: &Path,
    command_id: &str,
    context: Context,
) -> Result<Prepared, ErrorDto> {
    let command = find(repo_path, command_id)?;
    let template = user_command::parse(
        &command.command_line,
        command.working_directory.as_deref(),
        CommandStyle::native(),
    )
    .map_err(ErrorDto::from_error)?;
    let values = handle.user_command_values(context.clone())?;
    let expanded = template.expand(&values).map_err(ErrorDto::from_error)?;
    Ok(Prepared {
        expanded,
        root: std::path::PathBuf::from(values.repo),
        name: command.name,
        journal_key: crate::worker::repo_key(repo_path).ok(),
        command_id: command.id,
        context,
    })
}

/// Runs a prepared command as run `run_id` (which `cancel` stops), on the caller's thread: call
/// it off the repository's worker and off any async runtime. A failing program is an answer
/// (`success` false), not an error; a command that can't start is.
///
/// Each run is an operation log entry ("Run command", the command's name as its target) with
/// the program it ran (#894).
pub fn execute(prepared: &Prepared, run_id: &str) -> Result<UserCommandRunDto, ErrorDto> {
    runs().insert(run_id.to_string(), false);
    let _registered = Registered(run_id.to_string());
    let cancelled = || runs().get(run_id).copied().unwrap_or(false);
    let entry = prepared.journal_key.as_deref().map(|key| {
        let journal = oplog::journal(key);
        let id = journal.begin(oplog::Start {
            name: "Run command".to_string(),
            target: Some(prepared.name.clone()),
            params: Vec::new(),
            rerun: Some(oplog::Rerun::UserCommand {
                command_id: prepared.command_id.clone(),
                context: prepared.context.clone(),
            }),
        });
        (journal, id)
    });
    let programs = RunLog::new();
    let result = record_runs(&programs, || {
        user_command::run(
            &prepared.expanded,
            &prepared.root,
            git_core::process::find_program,
            &cancelled,
            RUN_TIMEOUT,
            OUTPUT_LIMIT,
        )
    });
    if let Some((journal, id)) = entry {
        let mut finish = log_finish(&result);
        finish.runs = programs.take();
        journal.finish(id, finish);
    }
    match result {
        Ok(outcome) => Ok(outcome.into()),
        Err(RunError::Cancelled) => Ok(UserCommandRunDto::cancelled()),
        Err(error) => Err(ErrorDto::from_error(error)),
    }
}

/// How a run ends in the operation log.
fn log_finish(result: &Result<user_command::RunOutcome, RunError>) -> oplog::Finish {
    let failed = |message: String| oplog::Finish {
        outcome: oplog::OperationOutcome::Failed,
        message: Some(message),
        ..oplog::Finish::succeeded()
    };
    match result {
        Ok(outcome) if outcome.success => oplog::Finish::succeeded(),
        Ok(outcome) if outcome.timed_out => failed("Took too long and was stopped.".to_string()),
        Ok(outcome) => failed(match outcome.exit_code {
            Some(code) => format!("Exited with code {code}."),
            None => "Failed.".to_string(),
        }),
        Err(RunError::Cancelled) => oplog::Finish {
            outcome: oplog::OperationOutcome::Cancelled,
            ..oplog::Finish::succeeded()
        },
        Err(error) => failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::time::Duration;

    fn command(id: &str, name: &str) -> config::UserCommand {
        config::UserCommand {
            id: id.to_string(),
            name: name.to_string(),
            command_line: "true".to_string(),
            working_directory: None,
            targets: Vec::new(),
            confirm: false,
            show_output: true,
            refresh: true,
        }
    }

    #[test]
    fn a_command_is_found_by_id_the_repositorys_own_first() {
        let commands = config::UserCommands {
            global: vec![command("a", "App-wide A"), command("b", "App-wide B")],
            repository: vec![command("a", "Repository A")],
        };
        assert_eq!(find_in(commands.clone(), "a").unwrap().name, "Repository A");
        assert_eq!(find_in(commands.clone(), "b").unwrap().name, "App-wide B");
        assert_eq!(find_in(commands, "c").unwrap_err().message, NOT_DEFINED);
    }

    fn outcome(success: bool, timed_out: bool, exit_code: Option<i32>) -> user_command::RunOutcome {
        user_command::RunOutcome {
            exit_code,
            success,
            timed_out,
            output: String::new(),
        }
    }

    #[test]
    fn a_run_ends_in_the_log_as_it_ended() {
        let finish = |result| {
            let finish = log_finish(&result);
            (finish.outcome, finish.message)
        };
        use oplog::OperationOutcome::*;
        assert_eq!(finish(Ok(outcome(true, false, Some(0)))), (Succeeded, None));
        assert_eq!(
            finish(Ok(outcome(false, false, Some(3)))),
            (Failed, Some("Exited with code 3.".to_string()))
        );
        assert_eq!(
            finish(Ok(outcome(false, false, None))),
            (Failed, Some("Failed.".to_string()))
        );
        assert_eq!(
            finish(Ok(outcome(false, true, None))),
            (Failed, Some("Took too long and was stopped.".to_string()))
        );
        assert_eq!(finish(Err(RunError::Cancelled)), (Cancelled, None));
        assert_eq!(
            finish(Err(RunError::NotFound("nope".to_string()))),
            (
                Failed,
                Some(RunError::NotFound("nope".to_string()).to_string())
            )
        );
    }

    #[cfg(unix)]
    fn prepared(script: &str, key: &str, root: &Path) -> Prepared {
        Prepared {
            expanded: user_command::Expanded {
                args: vec!["sh".to_string(), "-c".to_string(), script.to_string()],
                directory: None,
            },
            root: root.to_path_buf(),
            name: "Unit".to_string(),
            journal_key: Some(key.to_string()),
            command_id: "unit-command".to_string(),
            context: Context {
                commit: Some("abc".to_string()),
                paths: vec!["a.txt".to_string()],
                ..Context::default()
            },
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_run_is_logged_with_its_program_and_forgotten_when_it_ends() {
        let dir = tempfile::TempDir::new().unwrap();
        let key = "user-commands-unit-logged";
        let run = execute(&prepared("echo hi", key, dir.path()), "unit-run-1").unwrap();
        assert!(run.success);
        assert_eq!(run.output, "hi\n");
        assert!(!runs().contains_key("unit-run-1"));
        let page = oplog::journal(key).page(None, 10);
        let entry = &page.entries[0];
        assert_eq!(entry.name, "Run command");
        assert_eq!(entry.target.as_deref(), Some("Unit"));
        assert_eq!(entry.outcome, oplog::OperationOutcome::Succeeded);
        assert_eq!(entry.runs.len(), 1);
        assert_eq!(entry.runs[0].output, "hi\n");
        // Run again (#890) repeats the command by id, on the same selection.
        assert_eq!(
            entry.rerun,
            Some(oplog::Rerun::UserCommand {
                command_id: "unit-command".to_string(),
                context: Context {
                    commit: Some("abc".to_string()),
                    paths: vec!["a.txt".to_string()],
                    ..Context::default()
                },
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn cancel_stops_a_run_and_only_that_one() {
        let dir = tempfile::TempDir::new().unwrap();
        let key = "user-commands-unit-cancel";
        // A cancel for a run that isn't going on is dropped, not kept for a later one.
        cancel("unit-run-2");
        assert!(!runs().contains_key("unit-run-2"));
        let stopper = std::thread::spawn(|| {
            for _ in 0..100 {
                std::thread::sleep(Duration::from_millis(50));
                if runs().contains_key("unit-run-2") {
                    cancel("unit-run-2");
                    return;
                }
            }
        });
        let run = execute(&prepared("sleep 30", key, dir.path()), "unit-run-2").unwrap();
        stopper.join().unwrap();
        assert!(run.cancelled);
        assert_eq!(
            oplog::journal(key).page(None, 10).entries[0].outcome,
            oplog::OperationOutcome::Cancelled
        );
    }
}
