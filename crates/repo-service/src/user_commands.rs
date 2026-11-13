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
    let values = handle.user_command_values(context)?;
    let expanded = template.expand(&values).map_err(ErrorDto::from_error)?;
    Ok(Prepared {
        expanded,
        root: std::path::PathBuf::from(values.repo),
    })
}

/// Runs a prepared command as run `run_id` (which `cancel` stops), on the caller's thread: call
/// it off the repository's worker and off any async runtime. A failing program is an answer
/// (`success` false), not an error; a command that can't start is.
pub fn execute(prepared: &Prepared, run_id: &str) -> Result<UserCommandRunDto, ErrorDto> {
    runs().insert(run_id.to_string(), false);
    let _registered = Registered(run_id.to_string());
    let cancelled = || runs().get(run_id).copied().unwrap_or(false);
    match user_command::run(
        &prepared.expanded,
        &prepared.root,
        git_core::process::find_program,
        &cancelled,
        RUN_TIMEOUT,
        OUTPUT_LIMIT,
    ) {
        Ok(outcome) => Ok(outcome.into()),
        Err(RunError::Cancelled) => Ok(UserCommandRunDto::cancelled()),
        Err(error) => Err(ErrorDto::from_error(error)),
    }
}
