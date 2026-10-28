//! git's `diff.tool` / `merge.tool`: resolved like `git difftool` / `git mergetool` do, with a
//! short list of built-in presets, and launched by Browsitory itself.
//!
//! Security model:
//! - **Only the user's own git config is trusted.** Tool settings (`diff.tool`, `merge.tool`,
//!   `difftool.<name>.*`, `mergetool.<name>.*`) are read from the system, XDG and global levels
//!   only. A repository's own `.git/config` (and anything it includes) is ignored, because
//!   whoever can write to a repository (a shared drive, a downloaded archive) could otherwise
//!   make Browsitory run any command just by the user clicking "Open in diff tool". When only
//!   the repository sets a tool, the error says so.
//! - A `cmd` runs as `sh -c <cmd>` exactly as git runs it, with `LOCAL`, `REMOTE`, `MERGED` and
//!   `BASE` as environment variables. Browsitory never substitutes paths into the command text,
//!   so a file name can never inject shell syntax. Presets are argv arrays run without a shell.
//! - Side files go into a fresh private (0700 on Unix) temp directory per launch, created with
//!   `O_EXCL` (never through an existing file or symlink) under a sanitized name derived only from
//!   the file name, so a repository path can never choose where they are written.
//! - All paths handed to a tool are absolute, so none can be read as an option.
//!
//! See `docs/superpowers/specs/2026-10-27-parity-batch4f-design.md`.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use git2::{Config, ConfigLevel, ErrorCode, ObjectType, Oid, Repository, Tree};
use thiserror::Error;

use crate::launch::is_safe_relative;
use crate::merge::{find_conflict, MergeError};
use crate::process::{
    self, drain_tail, hides_console, spawn, tail_text, CommandStyle, LaunchError, SpawnSpec,
    SplitError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Diff,
    Merge,
}

/// The git config level a tool setting came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    System,
    Xdg,
    Global,
    Local,
    Worktree,
    Other,
}

impl From<ConfigLevel> for ConfigSource {
    fn from(level: ConfigLevel) -> Self {
        match level {
            ConfigLevel::ProgramData | ConfigLevel::System => ConfigSource::System,
            ConfigLevel::XDG => ConfigSource::Xdg,
            ConfigLevel::Global => ConfigSource::Global,
            ConfigLevel::Local => ConfigSource::Local,
            ConfigLevel::Worktree => ConfigSource::Worktree,
            _ => ConfigSource::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCommand {
    /// `<kind>tool.<name>.cmd`, run by `sh -c`.
    Shell(String),
    /// A built-in argv template, with `<kind>tool.<name>.path` applied to the program.
    Preset { program: String, args: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTool {
    pub name: String,
    /// Level of the `diff.tool` / `merge.tool` entry used.
    pub source: ConfigSource,
    /// The diff tool fell back to `merge.tool`.
    pub via_merge_tool: bool,
    pub command: ToolCommand,
    /// Merge only; always false for diff.
    pub trust_exit_code: bool,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("No diff tool is configured. Set diff.tool in your git config, for example: git config --global diff.tool meld")]
    NoDiffTool,
    #[error("No merge tool is configured. Set merge.tool in your git config, for example: git config --global merge.tool meld")]
    NoMergeTool,
    #[error("{setting} is set to \"{name}\" only in this repository's git config. Browsitory ignores tool settings from a repository's own config, so a repository can't make it run commands. Set it in your global git config instead, for example: git config --global {setting} {name}")]
    RepositoryConfigIgnored { setting: String, name: String },
    #[error("{name} runs in a terminal, which Browsitory can't host. Choose a graphical tool, or set {key} to run it in a terminal window.")]
    TerminalTool { name: String, key: String },
    #[error("{setting} is set to \"{name}\", but {key} is not set and Browsitory has no built-in command for it.")]
    UnknownTool {
        setting: String,
        name: String,
        key: String,
    },
    #[error("Invalid path.")]
    InvalidPath,
    #[error("repository has no working directory (bare repository)")]
    NoWorkdir,
    #[error("no conflict found for path '{0}'")]
    NoConflict(String),
    #[error("'{0}' is an add/delete conflict, not a text conflict")]
    NotATextConflict(String),
    #[error("{0} is a submodule, not a file.")]
    NotAFile(String),
    #[error("{0} is a symbolic link, which a merge tool can't merge.")]
    Symlink(String),
    #[error("{0} is too large to open (over 20 MB).")]
    TooLarge(String),
    #[error("The {tool} failed (exit {code}): {output}")]
    ToolFailed {
        tool: &'static str,
        code: i32,
        output: String,
    },
    #[error(transparent)]
    Launch(#[from] LaunchError),
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<SplitError> for ToolError {
    fn from(error: SplitError) -> Self {
        ToolError::Launch(LaunchError::Split(error))
    }
}

// ---------------------------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------------------------

/// (name, program, diff args, merge args)
type Preset = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

const BC_DIFF: &[&str] = &["$LOCAL", "$REMOTE"];
const BC_MERGE: &[&str] = &["$LOCAL", "$REMOTE", "$BASE", "-mergeoutput=$MERGED"];

const PRESETS: &[Preset] = &[
    (
        "meld",
        "meld",
        &["$LOCAL", "$REMOTE"],
        &["--output=$MERGED", "$LOCAL", "$BASE", "$REMOTE"],
    ),
    (
        "kdiff3",
        "kdiff3",
        &["$LOCAL", "$REMOTE"],
        &["--auto", "-o", "$MERGED", "$BASE", "$LOCAL", "$REMOTE"],
    ),
    ("bc", "bcompare", BC_DIFF, BC_MERGE),
    ("bc3", "bcompare", BC_DIFF, BC_MERGE),
    ("bc4", "bcompare", BC_DIFF, BC_MERGE),
    (
        "p4merge",
        "p4merge",
        &["$LOCAL", "$REMOTE"],
        // git's order: base, theirs, ours, result.
        &["$BASE", "$REMOTE", "$LOCAL", "$MERGED"],
    ),
    (
        "vscode",
        "code",
        &["--wait", "--diff", "$LOCAL", "$REMOTE"],
        &["--wait", "--merge", "$REMOTE", "$LOCAL", "$BASE", "$MERGED"],
    ),
    (
        "winmerge",
        "WinMergeU",
        &["-u", "-e", "$LOCAL", "$REMOTE"],
        &[
            "-u", "-e", "-wl", "-wr", "-fm", "$LOCAL", "$BASE", "$REMOTE", "-o", "$MERGED",
        ],
    ),
];

const TERMINAL_TOOLS: &[&str] = &[
    "vimdiff",
    "vimdiff1",
    "vimdiff2",
    "vimdiff3",
    "nvimdiff",
    "nvimdiff1",
    "nvimdiff2",
    "nvimdiff3",
    "emerge",
];

pub fn preset(kind: ToolKind, name: &str) -> Option<(&'static str, &'static [&'static str])> {
    PRESETS.iter().find(|p| p.0 == name).map(|p| {
        // Like git: on Windows `bcomp` (it waits); `BCompare.exe` returns at once.
        let program = if p.1 == "bcompare" && cfg!(windows) {
            "bcomp"
        } else {
            p.1
        };
        (program, if kind == ToolKind::Diff { p.2 } else { p.3 })
    })
}

/// Presets that, like git, leave out the base when a conflict has none (add/add).
const DROP_MISSING_BASE: &[&str] = &["kdiff3", "bc", "bc3", "bc4"];

pub fn is_terminal_tool(name: &str) -> bool {
    TERMINAL_TOOLS.contains(&name)
}

/// The levels whose files the user wrote: never a repository's own config.
fn trust_rank(level: ConfigLevel) -> Option<u8> {
    match level {
        ConfigLevel::ProgramData => Some(1),
        ConfigLevel::System => Some(2),
        ConfigLevel::XDG => Some(3),
        ConfigLevel::Global => Some(4),
        _ => None,
    }
}

/// Matches a config entry name against `section[.subsection].variable`: section and variable
/// case-insensitively, the subsection exactly (git's rules).
fn key_matches(entry_name: &str, key: &str) -> bool {
    fn parts(name: &str) -> Option<(&str, &str, &str)> {
        let first = name.find('.')?;
        let last = name.rfind('.')?;
        let subsection = if first == last {
            ""
        } else {
            &name[first + 1..last]
        };
        Some((&name[..first], subsection, &name[last + 1..]))
    }
    match (parts(entry_name), parts(key)) {
        (Some((s1, sub1, v1)), Some((s2, sub2, v2))) => {
            s1.eq_ignore_ascii_case(s2) && sub1 == sub2 && v1.eq_ignore_ascii_case(v2)
        }
        _ => false,
    }
}

/// One config value with the level it came from. `None` value = a bare `key` line (boolean
/// true in git).
#[derive(Debug, Clone)]
struct Entry {
    value: Option<String>,
    level: ConfigLevel,
}

/// Every value of `key` across all levels, lowest priority first.
fn entries_for(config: &Config, key: &str) -> Result<Vec<Entry>, git2::Error> {
    let mut found = Vec::new();
    let mut entries = config.entries(None)?;
    while let Some(entry) = entries.next() {
        let entry = entry?;
        if entry.name().is_ok_and(|name| key_matches(name, key)) {
            found.push(Entry {
                value: entry
                    .has_value()
                    .then(|| String::from_utf8_lossy(entry.value_bytes()).into_owned()),
                level: entry.level(),
            });
        }
    }
    Ok(found)
}

/// The effective value of `key` from trusted levels only: highest-priority level wins, and
/// within a level the last value.
fn trusted_entry(config: &Config, key: &str) -> Result<Option<Entry>, git2::Error> {
    Ok(entries_for(config, key)?
        .into_iter()
        .filter_map(|entry| trust_rank(entry.level).map(|rank| (rank, entry)))
        .enumerate()
        .max_by_key(|(order, (rank, _))| (*rank, *order))
        .map(|(_, (_, entry))| entry))
}

fn trusted_string(
    config: &Config,
    key: &str,
) -> Result<Option<(String, ConfigSource)>, git2::Error> {
    Ok(trusted_entry(config, key)?.and_then(|entry| {
        let value = entry.value?.trim().to_string();
        (!value.is_empty()).then(|| (value, entry.level.into()))
    }))
}

/// The value of `key` set at any untrusted level (a repository's own config).
fn untrusted_string(
    config: &Config,
    key: &str,
) -> Result<Option<(String, ConfigSource)>, git2::Error> {
    Ok(entries_for(config, key)?
        .into_iter()
        .rev()
        .filter(|entry| trust_rank(entry.level).is_none())
        .find_map(|entry| {
            let value = entry.value?.trim().to_string();
            (!value.is_empty()).then(|| (value, entry.level.into()))
        }))
}

/// git's boolean parsing: a bare key, `true`/`yes`/`on` or a non-zero integer is true.
fn parse_bool(entry: &Entry) -> bool {
    match entry.value.as_deref().map(str::trim) {
        None => true,
        Some(value) => {
            let value = value.to_ascii_lowercase();
            matches!(value.as_str(), "true" | "yes" | "on")
                || value.parse::<i64>().is_ok_and(|n| n != 0)
        }
    }
}

/// Resolves the diff or merge tool from **trusted** git config levels (see the module docs).
pub fn resolve_tool(config: &Config, kind: ToolKind) -> Result<ResolvedTool, ToolError> {
    let (setting, section) = match kind {
        ToolKind::Diff => ("diff.tool", "difftool"),
        ToolKind::Merge => ("merge.tool", "mergetool"),
    };
    let (name, source, via_merge_tool) = match trusted_string(config, setting)? {
        Some((name, source)) => (name, source, false),
        None => match (kind, trusted_string(config, "merge.tool")?) {
            (ToolKind::Diff, Some((name, source))) => (name, source, true),
            _ => {
                let ignored = match untrusted_string(config, setting)? {
                    Some((name, _)) => Some((setting, name)),
                    None if kind == ToolKind::Diff => untrusted_string(config, "merge.tool")?
                        .map(|(name, _)| ("merge.tool", name)),
                    None => None,
                };
                if let Some((setting, name)) = ignored {
                    return Err(ToolError::RepositoryConfigIgnored {
                        setting: setting.to_string(),
                        name,
                    });
                }
                return Err(match kind {
                    ToolKind::Diff => ToolError::NoDiffTool,
                    ToolKind::Merge => ToolError::NoMergeTool,
                });
            }
        },
    };
    let key = format!("{section}.{name}.cmd");
    let trust_exit_code = kind == ToolKind::Merge
        && trusted_entry(config, &format!("{section}.{name}.trustExitCode"))?
            .is_some_and(|entry| parse_bool(&entry));
    let command = if let Some((cmd, _)) = trusted_string(config, &key)? {
        ToolCommand::Shell(cmd)
    } else if is_terminal_tool(&name) {
        return Err(ToolError::TerminalTool { name, key });
    } else if let Some((program, args)) = preset(kind, &name) {
        ToolCommand::Preset {
            program: trusted_string(config, &format!("{section}.{name}.path"))?
                .map(|(path, _)| path)
                .unwrap_or_else(|| program.to_string()),
            args: args.iter().map(|a| a.to_string()).collect(),
        }
    } else {
        let setting = if via_merge_tool {
            "merge.tool"
        } else {
            setting
        };
        return Err(ToolError::UnknownTool {
            setting: setting.to_string(),
            name,
            key,
        });
    };
    Ok(ResolvedTool {
        name,
        source,
        via_merge_tool,
        command,
        trust_exit_code,
    })
}

/// The four paths a tool receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolVars {
    pub local: PathBuf,
    pub remote: PathBuf,
    pub merged: PathBuf,
    pub base: PathBuf,
}

impl ToolVars {
    fn get(&self, name: &str) -> Option<&Path> {
        match name {
            "LOCAL" => Some(&self.local),
            "REMOTE" => Some(&self.remote),
            "MERGED" => Some(&self.merged),
            "BASE" => Some(&self.base),
            _ => None,
        }
    }

    fn env(&self) -> Vec<(String, OsString)> {
        [
            ("LOCAL", &self.local),
            ("REMOTE", &self.remote),
            ("MERGED", &self.merged),
            ("BASE", &self.base),
        ]
        .into_iter()
        .map(|(name, path)| (name.to_string(), path.as_os_str().to_owned()))
        .collect()
    }
}

/// Expands `$LOCAL`/`${LOCAL}` (and REMOTE, MERGED, BASE) inside one argv word. Any other `$…`
/// stays literal: there is no shell here, and the expanded path stays inside its one word.
pub fn expand_tool_vars(word: &str, vars: &ToolVars) -> String {
    let mut out = String::new();
    let mut rest = word;
    while let Some(pos) = rest.find('$') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        let (name, consumed) = if let Some(inner) = after.strip_prefix('{') {
            match inner.find('}') {
                Some(end) => (&inner[..end], end + 2),
                None => ("", 0),
            }
        } else {
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(after.len());
            (&after[..end], end)
        };
        match vars.get(name) {
            Some(path) if consumed > 0 => {
                out.push_str(&path.display().to_string());
                rest = &after[consumed..];
            }
            _ => {
                out.push('$');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A resolved tool launch: program (absolute), argv, env and working directory.
#[derive(Debug, Clone)]
pub struct ToolInvocation {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, OsString)>,
    pub cwd: PathBuf,
    pub no_window: bool,
}

fn program_path(name: &str) -> Result<PathBuf, ToolError> {
    process::find_program(name)
        .ok_or_else(|| ToolError::Launch(LaunchError::NotFound(name.to_string())))
}

#[cfg(windows)]
fn shell() -> Option<PathBuf> {
    process::find_sh()
}

#[cfg(not(windows))]
fn shell() -> Option<PathBuf> {
    Some(PathBuf::from("/bin/sh"))
}

pub(crate) fn build_invocation(
    command: &ToolCommand,
    vars: &ToolVars,
    workdir: &Path,
) -> Result<ToolInvocation, ToolError> {
    let cwd = workdir.to_path_buf();
    match command {
        ToolCommand::Preset { program, args } => {
            let program = program_path(program)?;
            Ok(ToolInvocation {
                no_window: cfg!(windows) && hides_console(&program),
                program,
                args: args.iter().map(|a| expand_tool_vars(a, vars)).collect(),
                env: vars.env(),
                cwd,
            })
        }
        ToolCommand::Shell(cmd) => match shell() {
            // The paths travel only as environment variables; the cmd text is the user's own.
            Some(sh) => Ok(ToolInvocation {
                program: sh,
                args: vec!["-c".into(), cmd.clone()],
                env: vars.env(),
                cwd,
                no_window: cfg!(windows),
            }),
            None => {
                let words: Vec<String> = process::split_command_line(cmd, CommandStyle::Posix)?
                    .iter()
                    .map(|w| expand_tool_vars(w, vars))
                    .collect();
                let (program, args) = words
                    .split_first()
                    .ok_or(ToolError::from(SplitError::Empty))?;
                let program = program_path(program)?;
                Ok(ToolInvocation {
                    no_window: cfg!(windows) && hides_console(&program),
                    program,
                    args: args.to_vec(),
                    env: vars.env(),
                    cwd,
                })
            }
        },
    }
}

/// What the Preferences dialog shows for one tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitToolInfo {
    pub name: String,
    /// The cmd, or the preset joined with spaces.
    pub command: Option<String>,
    pub source: ConfigSource,
    pub via_merge_tool: bool,
    /// Why it can't be used.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTools {
    pub diff_tool: Option<GitToolInfo>,
    pub merge_tool: Option<GitToolInfo>,
}

fn info(config: &Config, kind: ToolKind) -> Result<Option<GitToolInfo>, ToolError> {
    let (setting, fallback) = match kind {
        ToolKind::Diff => ("diff.tool", Some("merge.tool")),
        ToolKind::Merge => ("merge.tool", None),
    };
    match resolve_tool(config, kind) {
        Ok(tool) => Ok(Some(GitToolInfo {
            command: Some(match &tool.command {
                ToolCommand::Shell(cmd) => cmd.clone(),
                ToolCommand::Preset { program, args } => std::iter::once(program.clone())
                    .chain(args.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(" "),
            }),
            name: tool.name,
            source: tool.source,
            via_merge_tool: tool.via_merge_tool,
            problem: None,
        })),
        Err(ToolError::NoDiffTool | ToolError::NoMergeTool) => Ok(None),
        Err(ToolError::RepositoryConfigIgnored { setting: key, name }) => {
            let source = untrusted_string(config, &key)?
                .map(|(_, source)| source)
                .unwrap_or(ConfigSource::Local);
            let error = ToolError::RepositoryConfigIgnored {
                setting: key.clone(),
                name: name.clone(),
            };
            Ok(Some(GitToolInfo {
                name,
                command: None,
                source,
                via_merge_tool: key != setting,
                problem: Some(error.to_string()),
            }))
        }
        Err(error @ (ToolError::TerminalTool { .. } | ToolError::UnknownTool { .. })) => {
            let entry = match trusted_string(config, setting)? {
                Some((name, source)) => Some((name, source, false)),
                None => match fallback {
                    Some(fallback) => {
                        trusted_string(config, fallback)?.map(|(name, source)| (name, source, true))
                    }
                    None => None,
                },
            };
            Ok(entry.map(|(name, source, via_merge_tool)| GitToolInfo {
                name,
                command: None,
                source,
                via_merge_tool,
                problem: Some(error.to_string()),
            }))
        }
        Err(error) => Err(error),
    }
}

/// The diff and merge tools as resolved from `config`, with problems as text.
pub fn git_tools_from(config: &Config) -> Result<GitTools, ToolError> {
    Ok(GitTools {
        diff_tool: info(config, ToolKind::Diff)?,
        merge_tool: info(config, ToolKind::Merge)?,
    })
}

pub fn git_tools(repo: &Repository) -> Result<GitTools, ToolError> {
    git_tools_from(&repo.config()?)
}

// ---------------------------------------------------------------------------------------------
// File versions
// ---------------------------------------------------------------------------------------------

/// Which two sides "Open in diff tool" compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSource {
    /// Index ↔ work tree file.
    Unstaged,
    /// HEAD ↔ index.
    Staged,
    /// First parent ↔ commit.
    Commit {
        commit_id: String,
        old_path: Option<String>,
    },
    /// `from` ↔ `to`; `to_id: None` is the work tree file (4d's compare with the working tree).
    Range {
        from_id: String,
        to_id: Option<String>,
        old_path: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileVersion {
    Head,
    Index,
    /// A conflict stage: 1 base, 2 ours, 3 theirs.
    Stage(u8),
    Commit(String),
    FirstParent(String),
}

/// Upper bound for `file_version_contents` when the bytes are sent over IPC (VSCode).
pub const MAX_FILE_VERSION_BYTES: usize = 20 * 1024 * 1024;
const GITLINK: u32 = 0o160000;
const LINK: u32 = 0o120000;
const STDERR_TAIL_BYTES: usize = 4096;
const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);
/// How long `launch_diff_tool` watches for an immediate failure.
pub const DIFF_TOOL_GRACE: Duration = Duration::from_secs(1);

fn checked(path: &str) -> Result<&Path, ToolError> {
    if is_safe_relative(path) {
        Ok(Path::new(path))
    } else {
        Err(ToolError::InvalidPath)
    }
}

fn commit_by_id<'r>(repo: &'r Repository, id: &str) -> Result<git2::Commit<'r>, ToolError> {
    Ok(repo.find_commit(Oid::from_str(id)?)?)
}

fn tree_blob(tree: &Tree, rel: &Path, path: &str) -> Result<Option<Oid>, ToolError> {
    match tree.get_path(rel) {
        Ok(entry) if entry.filemode() as u32 == GITLINK => {
            Err(ToolError::NotAFile(path.to_string()))
        }
        Ok(entry) if entry.kind() == Some(ObjectType::Blob) => Ok(Some(entry.id())),
        Ok(_) => Ok(None),
        Err(e) if e.code() == ErrorCode::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn index_blob(
    repo: &Repository,
    rel: &Path,
    stage: i32,
    path: &str,
) -> Result<Option<Oid>, ToolError> {
    match repo.index()?.get_path(rel, stage) {
        Some(entry) if entry.mode == GITLINK => Err(ToolError::NotAFile(path.to_string())),
        Some(entry) => Ok(Some(entry.id)),
        None => Ok(None),
    }
}

/// The contents of `path` in `version`, read from git objects (never through a shell or the
/// work tree). `Ok(None)` when the path doesn't exist there; `TooLarge` above `limit`, checked
/// from the object header before the blob is loaded.
pub fn file_version_contents(
    repo: &Repository,
    path: &str,
    version: &FileVersion,
    limit: usize,
) -> Result<Option<Vec<u8>>, ToolError> {
    let rel = checked(path)?;
    let blob = match version {
        FileVersion::Head => match repo.head().ok().and_then(|h| h.peel_to_tree().ok()) {
            Some(tree) => tree_blob(&tree, rel, path)?,
            None => None,
        },
        FileVersion::Index => index_blob(repo, rel, 0, path)?,
        FileVersion::Stage(stage) => index_blob(repo, rel, i32::from(*stage), path)?,
        FileVersion::Commit(id) => tree_blob(&commit_by_id(repo, id)?.tree()?, rel, path)?,
        FileVersion::FirstParent(id) => match commit_by_id(repo, id)?.parents().next() {
            Some(parent) => tree_blob(&parent.tree()?, rel, path)?,
            None => None,
        },
    };
    let Some(id) = blob else { return Ok(None) };
    let (size, _) = repo.odb()?.read_header(id)?;
    if size > limit {
        return Err(ToolError::TooLarge(path.to_string()));
    }
    Ok(Some(repo.find_blob(id)?.content().to_vec()))
}

// ---------------------------------------------------------------------------------------------
// Temp side files
// ---------------------------------------------------------------------------------------------

const TOOL_DIR_PREFIX: &str = "browsitory-tool-";

/// Where per-launch tool directories are created: the user's temp dir. Each launch gets its own
/// randomly named, private directory directly in it (no shared parent another user could
/// pre-create).
pub fn tool_temp_root() -> PathBuf {
    std::env::temp_dir()
}

/// Removes `browsitory-tool-*` directories in `root` older than `max_age`: what a crash or a
/// handed-off launcher left behind. Symlinks are never followed.
pub fn sweep_stale_tool_dirs(root: &Path, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(TOOL_DIR_PREFIX)
        {
            continue;
        }
        // `DirEntry::file_type`/`metadata` don't follow symlinks.
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.file_type().is_dir() {
            continue;
        }
        let old = meta
            .modified()
            .map(|modified| now.duration_since(modified).unwrap_or_default() >= max_age)
            .unwrap_or(false);
        if old {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn new_tool_dir(root: &Path) -> Result<PathBuf, ToolError> {
    sweep_stale_tool_dirs(root, STALE_AFTER);
    // A random name, created exclusively, owner-only from the start on Unix (no window where
    // the umask's group/other bits apply).
    let mut builder = tempfile::Builder::new();
    builder.prefix(TOOL_DIR_PREFIX);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    Ok(builder.tempdir_in(root)?.keep())
}

/// `<stem>_<SIDE>.<ext>` from the file name only, with anything but ASCII letters, digits,
/// `.`, `-`, `_` and space replaced and the length capped, so a repository path can't steer
/// the name (no separators, no `..`, no device names, no Windows streams).
fn side_name(path: &str, side: &str) -> String {
    fn clean(text: &str, max: usize) -> String {
        text.chars()
            .map(|c| {
                // Letters and digits of any script are safe in a file name; separators, `:`,
                // wildcards and control characters are not.
                if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                    c
                } else {
                    '_'
                }
            })
            .take(max)
            .collect()
    }
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => (&name[..dot], Some(&name[dot + 1..])),
        _ => (name.as_str(), None),
    };
    let stem = clean(stem.trim_start_matches('.'), 64);
    let stem = if stem.is_empty() {
        "file".to_string()
    } else {
        stem
    };
    match ext.map(|e| clean(e, 16)) {
        Some(ext) if !ext.is_empty() => format!("{stem}_{side}.{ext}"),
        _ => format!("{stem}_{side}"),
    }
}

/// Creates a new file (never an existing file or symlink: `O_EXCL`), owner-only on Unix.
fn write_side(dir: &Path, path: &str, side: &str, contents: &[u8]) -> Result<PathBuf, ToolError> {
    let file = dir.join(side_name(path, side));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(&file)?.write_all(contents)?;
    Ok(file)
}

/// The work tree file for `rel`, refused when a directory on the way is a symlink out of the
/// work tree (a tool must never read or write outside it through `REMOTE`/`MERGED`).
fn work_tree_file(workdir: &Path, rel: &Path) -> Result<PathBuf, ToolError> {
    let file = workdir.join(rel);
    if crate::launch::parent_stays_inside(workdir, &file) {
        Ok(file)
    } else {
        Err(ToolError::InvalidPath)
    }
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

/// A prepared `git difftool`-style launch; `temp_dir` holds the side files.
#[derive(Debug)]
pub struct PreparedDiffTool {
    pub invocation: ToolInvocation,
    pub temp_dir: PathBuf,
}

/// A prepared merge tool launch, plus what `run_merge_tool` needs to judge the outcome.
#[derive(Debug)]
pub struct PreparedMergeTool {
    pub invocation: ToolInvocation,
    pub temp_dir: PathBuf,
    pub merged: PathBuf,
    pub merged_before: Vec<u8>,
    pub trust_exit_code: bool,
}

pub fn prepare_diff_tool(
    repo: &Repository,
    path: &str,
    source: &DiffSource,
) -> Result<PreparedDiffTool, ToolError> {
    prepare_diff_tool_with(repo, &repo.config()?, path, source, &tool_temp_root())
}

/// `prepare_diff_tool` with the git config and temp root given (tests).
pub fn prepare_diff_tool_with(
    repo: &Repository,
    config: &Config,
    path: &str,
    source: &DiffSource,
    root: &Path,
) -> Result<PreparedDiffTool, ToolError> {
    let rel = checked(path)?;
    let workdir = repo.workdir().ok_or(ToolError::NoWorkdir)?.to_path_buf();
    let tool = resolve_tool(config, ToolKind::Diff)?;
    let (local_version, remote_version, local_path) = match source {
        DiffSource::Unstaged => (FileVersion::Index, None, path),
        DiffSource::Staged => (FileVersion::Head, Some(FileVersion::Index), path),
        DiffSource::Commit {
            commit_id,
            old_path,
        } => (
            FileVersion::FirstParent(commit_id.clone()),
            Some(FileVersion::Commit(commit_id.clone())),
            old_path.as_deref().unwrap_or(path),
        ),
        DiffSource::Range {
            from_id,
            to_id,
            old_path,
        } => (
            FileVersion::Commit(from_id.clone()),
            to_id.clone().map(FileVersion::Commit),
            old_path.as_deref().unwrap_or(path),
        ),
    };
    let local_contents = file_version_contents(repo, local_path, &local_version, usize::MAX)?;
    let remote_contents = match &remote_version {
        Some(version) => Some(file_version_contents(repo, path, version, usize::MAX)?),
        None => None,
    };
    let work_file = work_tree_file(&workdir, rel)?;
    let temp_dir = new_tool_dir(root)?;
    let result = (|| {
        let local = write_side(
            &temp_dir,
            path,
            "LOCAL",
            local_contents.as_deref().unwrap_or_default(),
        )?;
        let remote = match remote_contents {
            Some(contents) => write_side(
                &temp_dir,
                path,
                "REMOTE",
                contents.as_deref().unwrap_or_default(),
            )?,
            // A symlink is compared as its target text, like git does, never followed.
            None if is_symlink(&work_file) => {
                let target = std::fs::read_link(&work_file)?;
                write_side(
                    &temp_dir,
                    path,
                    "REMOTE",
                    target.to_string_lossy().as_bytes(),
                )?
            }
            None if work_file.is_file() => work_file.clone(),
            None => write_side(&temp_dir, path, "REMOTE", b"")?,
        };
        // As in git difftool: MERGED is the path being compared and BASE equals MERGED.
        let vars = ToolVars {
            local,
            remote,
            merged: work_file.clone(),
            base: work_file.clone(),
        };
        build_invocation(&tool.command, &vars, &workdir)
    })();
    match result {
        Ok(invocation) => Ok(PreparedDiffTool {
            invocation,
            temp_dir,
        }),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&temp_dir);
            Err(error)
        }
    }
}

/// A conflicted path a merge tool (or VSCode's merge editor) may open.
pub struct MergeTarget {
    pub conflict: git2::IndexConflict,
    /// The work tree file the result is written to.
    pub merged: PathBuf,
    pub workdir: PathBuf,
}

/// The checks before any merge tool or merge editor opens `path`: a safe relative path, a text
/// conflict with both sides, no symlink or submodule entry, and a work tree file that is not a
/// symlink and is not reached through a directory symlink out of the work tree.
pub fn check_merge_target(repo: &Repository, path: &str) -> Result<MergeTarget, ToolError> {
    let rel = checked(path)?;
    let workdir = repo.workdir().ok_or(ToolError::NoWorkdir)?.to_path_buf();
    let conflict = find_conflict(repo, path).map_err(|error| match error {
        MergeError::NoConflict(path) => ToolError::NoConflict(path),
        MergeError::Git(error) => ToolError::Git(error),
        other => ToolError::Io(std::io::Error::other(other.to_string())),
    })?;
    let (Some(ours), Some(theirs)) = (&conflict.our, &conflict.their) else {
        return Err(ToolError::NotATextConflict(path.to_string()));
    };
    let entries = [Some(ours), Some(theirs), conflict.ancestor.as_ref()];
    if entries.iter().flatten().any(|entry| entry.mode == GITLINK) {
        return Err(ToolError::NotAFile(path.to_string()));
    }
    let merged = work_tree_file(&workdir, rel)?;
    if entries.iter().flatten().any(|entry| entry.mode == LINK) || is_symlink(&merged) {
        return Err(ToolError::Symlink(path.to_string()));
    }
    Ok(MergeTarget {
        conflict,
        merged,
        workdir,
    })
}

pub fn prepare_merge_tool(repo: &Repository, path: &str) -> Result<PreparedMergeTool, ToolError> {
    prepare_merge_tool_with(repo, &repo.config()?, path, &tool_temp_root())
}

/// `prepare_merge_tool` with the git config and temp root given (tests).
pub fn prepare_merge_tool_with(
    repo: &Repository,
    config: &Config,
    path: &str,
    root: &Path,
) -> Result<PreparedMergeTool, ToolError> {
    let target = check_merge_target(repo, path)?;
    let (conflict, merged, workdir) = (target.conflict, target.merged, target.workdir);
    let (Some(ours), Some(theirs)) = (&conflict.our, &conflict.their) else {
        return Err(ToolError::NotATextConflict(path.to_string()));
    };
    let mut tool = resolve_tool(config, ToolKind::Merge)?;
    if conflict.ancestor.is_none() && DROP_MISSING_BASE.contains(&tool.name.as_str()) {
        if let ToolCommand::Preset { args, .. } = &mut tool.command {
            args.retain(|arg| arg != "$BASE");
        }
    }
    let blob =
        |id: Oid| -> Result<Vec<u8>, ToolError> { Ok(repo.find_blob(id)?.content().to_vec()) };
    let ours = blob(ours.id)?;
    let theirs = blob(theirs.id)?;
    let base = conflict
        .ancestor
        .as_ref()
        .map(|a| blob(a.id))
        .transpose()?
        .unwrap_or_default();
    let merged_before = std::fs::read(&merged).unwrap_or_default();
    let temp_dir = new_tool_dir(root)?;
    let result = (|| {
        let vars = ToolVars {
            local: write_side(&temp_dir, path, "LOCAL", &ours)?,
            remote: write_side(&temp_dir, path, "REMOTE", &theirs)?,
            base: write_side(&temp_dir, path, "BASE", &base)?,
            merged: merged.clone(),
        };
        build_invocation(&tool.command, &vars, &workdir)
    })();
    match result {
        Ok(invocation) => Ok(PreparedMergeTool {
            invocation,
            temp_dir,
            merged,
            merged_before,
            trust_exit_code: tool.trust_exit_code,
        }),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&temp_dir);
            Err(error)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Launch and wait
// ---------------------------------------------------------------------------------------------

fn spawn_tool(invocation: &ToolInvocation) -> Result<std::process::Child, ToolError> {
    Ok(spawn(&SpawnSpec {
        program: invocation.program.clone(),
        args: invocation.args.clone(),
        cwd: invocation.cwd.clone(),
        env: invocation.env.clone(),
        new_console: false,
        no_window: invocation.no_window,
        capture_stderr: true,
    })?)
}

/// Starts the diff tool. Fails when it exits non-zero within `grace` (typically "command not
/// found"). A quick exit 0 leaves the temp dir for the 24 h sweep, because a launcher that
/// hands off to a running instance still needs the files. Otherwise a background thread waits
/// for the tool, keeps draining its stderr, and removes the temp dir when it exits.
pub fn launch_diff_tool(prepared: PreparedDiffTool, grace: Duration) -> Result<(), ToolError> {
    let mut child = match spawn_tool(&prepared.invocation) {
        Ok(child) => child,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&prepared.temp_dir);
            return Err(error);
        }
    };
    let stderr = child.stderr.take().expect("stderr is piped");
    let (tail, reader) = drain_tail(stderr, STDERR_TAIL_BYTES);
    let deadline = Instant::now() + grace;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = reader.join();
                if status.success() {
                    return Ok(());
                }
                let _ = std::fs::remove_dir_all(&prepared.temp_dir);
                let output = tail_text(tail.lock().unwrap_or_else(|e| e.into_inner()).tail_bytes());
                return Err(ToolError::ToolFailed {
                    tool: "diff tool",
                    code: status.code().unwrap_or(-1),
                    output,
                });
            }
            Ok(None) => {}
            Err(error) => {
                // Leave the child to its background reaper below; the error is ours.
                let temp_dir = prepared.temp_dir;
                std::thread::spawn(move || {
                    let _ = child.wait();
                    let _ = reader.join();
                    let _ = std::fs::remove_dir_all(temp_dir);
                });
                return Err(error.into());
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let temp_dir = prepared.temp_dir;
    std::thread::spawn(move || {
        let _ = child.wait();
        let _ = reader.join();
        let _ = std::fs::remove_dir_all(temp_dir);
    });
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeToolOutcome {
    Resolved,
    Unresolved,
}

/// Runs the merge tool and blocks until it exits: never call it on the repo worker thread.
/// With `trust_exit_code` the exit code decides; otherwise any change to `MERGED` counts as
/// resolved. Exit 126/127 (the shell couldn't run the command) is an error, not an outcome.
pub fn run_merge_tool(prepared: PreparedMergeTool) -> Result<MergeToolOutcome, ToolError> {
    let result = (|| {
        let mut child = spawn_tool(&prepared.invocation)?;
        let stderr = child.stderr.take().expect("stderr is piped");
        let (tail, reader) = drain_tail(stderr, STDERR_TAIL_BYTES);
        let status = child.wait()?;
        let _ = reader.join();
        let code = status.code().unwrap_or(-1);
        if code == 126 || code == 127 {
            let output = tail_text(tail.lock().unwrap_or_else(|e| e.into_inner()).tail_bytes());
            return Err(ToolError::ToolFailed {
                tool: "merge tool",
                code,
                output,
            });
        }
        let resolved = if prepared.trust_exit_code {
            status.success()
        } else {
            std::fs::read(&prepared.merged).unwrap_or_default() != prepared.merged_before
        };
        Ok(if resolved {
            MergeToolOutcome::Resolved
        } else {
            MergeToolOutcome::Unresolved
        })
    })();
    let _ = std::fs::remove_dir_all(&prepared.temp_dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> ToolVars {
        ToolVars {
            local: "/t/a_LOCAL.rs".into(),
            remote: "/t/a_REMOTE.rs".into(),
            merged: "/r/a.rs".into(),
            base: "/t/a_BASE.rs".into(),
        }
    }

    #[test]
    fn expand_tool_vars_replaces_only_the_four_names() {
        let v = vars();
        assert_eq!(expand_tool_vars("$LOCAL", &v), "/t/a_LOCAL.rs");
        assert_eq!(expand_tool_vars("--output=$MERGED", &v), "--output=/r/a.rs");
        assert_eq!(expand_tool_vars("${REMOTE}x", &v), "/t/a_REMOTE.rsx");
        assert_eq!(
            expand_tool_vars("$BASEX $HOME $ ${", &v),
            "$BASEX $HOME $ ${"
        );
        assert_eq!(
            expand_tool_vars("-mergeoutput=$MERGED", &v),
            "-mergeoutput=/r/a.rs"
        );
    }

    #[test]
    fn presets_and_terminal_tools() {
        assert_eq!(
            preset(ToolKind::Diff, "meld"),
            Some(("meld", &["$LOCAL", "$REMOTE"][..]))
        );
        // Like git: Beyond Compare's console launcher on Windows (`BCompare.exe` returns at
        // once, which would end the merge before you start).
        let bc = if cfg!(windows) { "bcomp" } else { "bcompare" };
        assert_eq!(preset(ToolKind::Merge, "bc4").unwrap().0, bc);
        assert_eq!(
            preset(ToolKind::Merge, "p4merge").unwrap().1,
            &["$BASE", "$REMOTE", "$LOCAL", "$MERGED"][..]
        );
        assert_eq!(
            preset(ToolKind::Merge, "vscode").unwrap().1,
            &["--wait", "--merge", "$REMOTE", "$LOCAL", "$BASE", "$MERGED"][..]
        );
        assert_eq!(preset(ToolKind::Diff, "vimdiff"), None);
        assert!(is_terminal_tool("nvimdiff2"));
        assert!(!is_terminal_tool("gvimdiff"));
    }

    #[test]
    fn batch_files_are_started_without_a_console_window() {
        assert!(hides_console(Path::new(r"C:\Users\me\bin\code.cmd")));
        assert!(hides_console(Path::new("x.BAT")));
        assert!(!hides_console(Path::new("meld.exe")));
        assert!(!hides_console(Path::new("/usr/bin/meld")));
    }

    #[test]
    fn key_matching_follows_git_case_rules() {
        assert!(key_matches("difftool.Meld.cmd", "DiffTool.Meld.CMD"));
        assert!(!key_matches("difftool.meld.cmd", "difftool.Meld.cmd"));
        assert!(key_matches("diff.tool", "Diff.Tool"));
        assert!(key_matches("difftool.a.b.cmd", "difftool.a.b.cmd"));
        assert!(!key_matches("diff.tool", "merge.tool"));
    }

    #[test]
    fn side_names_keep_the_extension_and_drop_anything_unsafe() {
        assert_eq!(side_name("src/main.rs", "LOCAL"), "main_LOCAL.rs");
        assert_eq!(side_name("Makefile", "BASE"), "Makefile_BASE");
        assert_eq!(side_name(".gitignore", "REMOTE"), "gitignore_REMOTE");
        assert_eq!(
            side_name("a/we:ird*na$me?.t|xt", "LOCAL"),
            "we_ird_na_me__LOCAL.t_xt"
        );
        assert_eq!(side_name("CON", "LOCAL"), "CON_LOCAL");
        assert_eq!(side_name("dir/Übersicht.md", "LOCAL"), "Übersicht_LOCAL.md");
        assert_eq!(side_name("文档.txt", "BASE"), "文档_BASE.txt");
        let long = format!("{}.rs", "x".repeat(300));
        assert_eq!(side_name(&long, "LOCAL").len(), 64 + "_LOCAL.rs".len());
    }
}
