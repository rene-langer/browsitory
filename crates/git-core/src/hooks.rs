//! Client-side git hooks: discovery (`core.hooksPath`), running them the way git does, and the
//! reporter/options the commit and push paths use. See
//! `docs/superpowers/specs/2026-10-27-parity-batch4e-design.md`.
//!
//! `git-core` never prints: problems the UI does not show (a failed `post-*` hook, a hook
//! without an execute bit) are handed to the caller's `HookReporter` to log.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use git2::Repository;
use thiserror::Error;

use crate::process::{run_captured, Captured, RunError};

/// A hook still running after this long is killed (with its process group on Unix).
pub const HOOK_TIMEOUT: Duration = Duration::from_secs(600);
/// Only the last 64 KiB of a hook's combined output is kept.
pub const OUTPUT_LIMIT: usize = 64 * 1024;
/// The hooks `git commit` runs before writing the commit; what the "Skip hooks" UI is about.
pub const COMMIT_HOOKS: [&str; 3] = ["pre-commit", "prepare-commit-msg", "commit-msg"];
/// What `--no-verify` skips.
pub const SKIPPABLE_HOOKS: [&str; 3] = ["pre-commit", "commit-msg", "pre-push"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRun {
    pub success: bool,
    pub output: String,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    NotInstalled,
    /// Unix: the file exists without an execute bit. Not run, like git.
    NotExecutable,
    /// Windows: no `sh.exe` next to `git.exe` or on `PATH`.
    NoShell,
    Ran(HookRun),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HookError {
    #[error("Could not run the {hook} hook: {message}")]
    Spawn { hook: String, message: String },
    #[error("The {hook} hook was cancelled.")]
    Cancelled { hook: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookLookup {
    Missing,
    NotExecutable(PathBuf),
    Found(PathBuf),
}

/// Told about every hook that was found: `started` right before it is spawned, `finished` with
/// its result (also for `NotExecutable`/`NoShell`, which never start).
pub trait HookReporter {
    fn started(&mut self, hook: &str);
    fn finished(&mut self, hook: &str, result: &Result<HookOutcome, HookError>);
}

pub struct HookOptions<'a> {
    /// `--no-verify`: skip `SKIPPABLE_HOOKS`.
    pub no_verify: bool,
    pub cancel: Option<&'a dyn Fn() -> bool>,
    pub reporter: Option<&'a mut dyn HookReporter>,
    pub timeout: Duration,
}

impl Default for HookOptions<'_> {
    fn default() -> Self {
        Self {
            no_verify: false,
            cancel: None,
            reporter: None,
            timeout: HOOK_TIMEOUT,
        }
    }
}

pub struct HookInvocation<'a> {
    pub name: &'a str,
    pub args: &'a [&'a str],
    pub stdin: Option<&'a [u8]>,
    pub env: &'a [(&'a str, &'a OsStr)],
}

/// Where hooks run: the worktree root, or the git dir of a bare repository.
fn run_dir(repo: &Repository) -> &Path {
    repo.workdir().unwrap_or_else(|| repo.path())
}

/// `core.hooksPath` (`~/` expanded; relative to the worktree root) or `<common dir>/hooks`, so
/// linked worktrees share the main repository's hooks.
pub fn hooks_dir(repo: &Repository) -> Result<PathBuf, git2::Error> {
    match repo.config()?.get_path("core.hooksPath") {
        Ok(path) if path.is_absolute() => Ok(path),
        Ok(path) => Ok(run_dir(repo).join(path)),
        Err(error) if error.code() == git2::ErrorCode::NotFound => {
            Ok(repo.commondir().join("hooks"))
        }
        Err(error) => Err(error),
    }
}

pub fn find_hook(repo: &Repository, name: &str) -> Result<HookLookup, git2::Error> {
    let path = hooks_dir(repo)?.join(name);
    let Ok(metadata) = std::fs::metadata(&path) else {
        return Ok(HookLookup::Missing);
    };
    if !metadata.is_file() {
        return Ok(HookLookup::Missing);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Ok(HookLookup::NotExecutable(path));
        }
    }
    Ok(HookLookup::Found(path))
}

/// The subset of `names` that is installed and would run.
pub fn installed_hooks(repo: &Repository, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .filter(|name| matches!(find_hook(repo, name), Ok(HookLookup::Found(_))))
        .map(|name| name.to_string())
        .collect()
}

/// Where Git for Windows keeps `sh.exe` relative to a `git.exe` found on `PATH`
/// (`Git\cmd\git.exe`, `Git\bin\git.exe`, `Git\mingw64\bin\git.exe`), nearest first.
pub fn sh_candidates(git_exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut dir = git_exe.parent();
    for _ in 0..3 {
        let Some(current) = dir else { break };
        for relative in ["sh.exe", "bin/sh.exe", "usr/bin/sh.exe"] {
            let candidate = current.join(relative);
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
        dir = current.parent();
    }
    candidates
}

#[cfg(windows)]
fn find_on_path(file: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(file))
        .find(|path| path.is_file())
}

#[cfg(windows)]
fn find_sh() -> Option<PathBuf> {
    find_on_path("git.exe")
        .and_then(|git| sh_candidates(&git).into_iter().find(|path| path.is_file()))
        .or_else(|| find_on_path("sh.exe"))
}

#[cfg(unix)]
fn hook_command(path: &Path) -> Option<Command> {
    Some(Command::new(path))
}

#[cfg(windows)]
fn hook_command(path: &Path) -> Option<Command> {
    let mut command = Command::new(find_sh()?);
    command.arg(path.to_string_lossy().replace('\\', "/"));
    Some(command)
}

fn configure(command: &mut Command, repo: &Repository, invocation: &HookInvocation<'_>) {
    command
        .args(invocation.args)
        .current_dir(run_dir(repo))
        .env("GIT_DIR", repo.path());
    for (key, value) in invocation.env {
        command.env(key, value);
    }
}

fn captured_outcome(
    hook: &str,
    result: Result<Captured, RunError>,
) -> Result<HookOutcome, HookError> {
    match result {
        Ok(captured) => Ok(HookOutcome::Ran(HookRun {
            success: captured.success,
            output: captured.output,
            timed_out: captured.timed_out,
        })),
        Err(RunError::Cancelled) => Err(HookError::Cancelled {
            hook: hook.to_string(),
        }),
        Err(RunError::Spawn(error)) | Err(RunError::Wait(error)) => Err(HookError::Spawn {
            hook: hook.to_string(),
            message: error.to_string(),
        }),
    }
}

/// Runs hook `invocation.name` if it is installed: cwd = worktree root, `GIT_DIR` set, combined
/// output capped at `OUTPUT_LIMIT`, killed after `timeout` or when `cancel` returns true.
pub fn run_hook(
    repo: &Repository,
    invocation: &HookInvocation<'_>,
    cancel: &dyn Fn() -> bool,
    timeout: Duration,
) -> Result<HookOutcome, HookError> {
    let path = match find_hook(repo, invocation.name) {
        Ok(HookLookup::Found(path)) => path,
        Ok(HookLookup::Missing) => return Ok(HookOutcome::NotInstalled),
        Ok(HookLookup::NotExecutable(_)) => return Ok(HookOutcome::NotExecutable),
        Err(error) => {
            return Err(HookError::Spawn {
                hook: invocation.name.to_string(),
                message: error.message().to_string(),
            })
        }
    };
    let Some(mut command) = hook_command(&path) else {
        return Ok(HookOutcome::NoShell);
    };
    configure(&mut command, repo, invocation);
    let result = run_captured(command, invocation.stdin, cancel, timeout, OUTPUT_LIMIT);
    #[cfg(unix)]
    {
        // No shebang: exec fails with ENOEXEC and git runs the file with sh instead.
        const ENOEXEC: i32 = 8;
        if let Err(RunError::Spawn(error)) = &result {
            if error.raw_os_error() == Some(ENOEXEC) {
                let mut command = Command::new("sh");
                command.arg(&path);
                configure(&mut command, repo, invocation);
                return captured_outcome(
                    invocation.name,
                    run_captured(command, invocation.stdin, cancel, timeout, OUTPUT_LIMIT),
                );
            }
        }
    }
    captured_outcome(invocation.name, result)
}

/// `run_hook` plus reporter calls, using `options.cancel` and `options.timeout`.
pub fn run_reported(
    repo: &Repository,
    invocation: &HookInvocation<'_>,
    options: &mut HookOptions<'_>,
) -> Result<HookOutcome, HookError> {
    let never = || false;
    let cancel: &dyn Fn() -> bool = match options.cancel {
        Some(cancel) => cancel,
        None => &never,
    };
    if matches!(find_hook(repo, invocation.name), Ok(HookLookup::Found(_))) {
        if let Some(reporter) = options.reporter.as_deref_mut() {
            reporter.started(invocation.name);
        }
    }
    let result = run_hook(repo, invocation, cancel, options.timeout);
    if !matches!(result, Ok(HookOutcome::NotInstalled)) {
        if let Some(reporter) = options.reporter.as_deref_mut() {
            reporter.finished(invocation.name, &result);
        }
    }
    result
}

fn describe(timeout: Duration) -> String {
    let seconds = timeout.as_secs();
    if seconds >= 60 && seconds.is_multiple_of(60) {
        let minutes = seconds / 60;
        if minutes == 1 {
            "1 minute".to_string()
        } else {
            format!("{minutes} minutes")
        }
    } else if seconds >= 1 {
        format!("{seconds} seconds")
    } else {
        format!("{} ms", timeout.as_millis())
    }
}

/// What a failed gate hook shows the user; `None` when the outcome lets the operation proceed.
pub fn failure_output(outcome: &HookOutcome, timeout: Duration) -> Option<String> {
    match outcome {
        HookOutcome::Ran(run) if !run.success => {
            let mut text = run.output.clone();
            if run.timed_out {
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&format!(
                    "Browsitory stopped the hook after {}.",
                    describe(timeout)
                ));
            }
            Some(text)
        }
        HookOutcome::NoShell => Some(
            "Browsitory could not find sh.exe (install Git for Windows) to run this hook."
                .to_string(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_candidates_cover_git_for_windows_layouts() {
        let cmd = sh_candidates(Path::new("C:/Program Files/Git/cmd/git.exe"));
        assert!(cmd.contains(&Path::new("C:/Program Files/Git").join("bin/sh.exe")));
        let mingw = sh_candidates(Path::new("C:/Program Files/Git/mingw64/bin/git.exe"));
        assert!(mingw.contains(&Path::new("C:/Program Files/Git").join("usr/bin/sh.exe")));
        let bin = sh_candidates(Path::new("C:/Git/bin/git.exe"));
        assert_eq!(bin[0], Path::new("C:/Git/bin").join("sh.exe"));
    }

    #[test]
    fn failure_output_notes_a_timeout() {
        let outcome = HookOutcome::Ran(HookRun {
            success: false,
            output: "slow\n".into(),
            timed_out: true,
        });
        assert_eq!(
            failure_output(&outcome, Duration::from_secs(600)).unwrap(),
            "slow\nBrowsitory stopped the hook after 10 minutes."
        );
        let ok = HookOutcome::Ran(HookRun {
            success: true,
            output: String::new(),
            timed_out: false,
        });
        assert_eq!(failure_output(&ok, HOOK_TIMEOUT), None);
        assert_eq!(
            failure_output(&HookOutcome::NotInstalled, HOOK_TIMEOUT),
            None
        );
        assert_eq!(
            failure_output(&HookOutcome::NoShell, HOOK_TIMEOUT).unwrap(),
            "Browsitory could not find sh.exe (install Git for Windows) to run this hook."
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_failing_hook_runs_through_the_discovered_sh() {
        if find_sh().is_none() {
            eprintln!("no sh.exe found; skipping");
            return;
        }
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let hooks = dir.path().join("h");
        repo.config()
            .unwrap()
            .set_str("core.hooksPath", hooks.to_str().unwrap())
            .unwrap();
        std::fs::create_dir_all(&hooks).unwrap();
        std::fs::write(
            hooks.join("pre-commit"),
            "#!/bin/sh\necho nope >&2\nexit 1\n",
        )
        .unwrap();
        let outcome = run_hook(
            &repo,
            &HookInvocation {
                name: "pre-commit",
                args: &[],
                stdin: None,
                env: &[],
            },
            &|| false,
            Duration::from_secs(60),
        )
        .unwrap();
        assert!(
            matches!(outcome, HookOutcome::Ran(ref run) if !run.success && run.output.contains("nope")),
            "{outcome:?}"
        );
    }
}
