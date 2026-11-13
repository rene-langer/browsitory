//! User-defined commands (#893): a command line with `${…}` variables, split into argv like the
//! external tools' commands (`process::split_command_line`, never a shell), then expanded word by
//! word. A value never splits into more words and is never read as quoting, so a branch or file
//! name with spaces, quotes or shell syntax stays exactly one argument; only a `${paths}` word of
//! its own becomes one argument per path.
//!
//! `$${` stands for a literal `${`; any other `$` is an ordinary character.

use serde::Serialize;
use thiserror::Error;

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::process::{
    run_captured, split_command_line, CommandStyle, RunError as ProcessRunError, SplitError,
    REPO_PINNING_ENV,
};

/// A variable a user command can use, written `${name}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "UserCommandVariable"))]
#[serde(rename_all = "camelCase")]
pub enum Variable {
    /// The repository's work tree root.
    Repo,
    /// A commit's full id.
    Commit,
    /// A commit's abbreviated id.
    ShortCommit,
    /// A branch's short name (`main`, `origin/main`).
    Branch,
    /// A branch's or tag's full ref name (`refs/heads/main`, `refs/tags/v1`).
    Ref,
    /// One file's absolute path.
    Path,
    /// The selected files' absolute paths, one argument each.
    Paths,
}

impl Variable {
    pub const ALL: [Variable; 7] = [
        Variable::Repo,
        Variable::Commit,
        Variable::ShortCommit,
        Variable::Branch,
        Variable::Ref,
        Variable::Path,
        Variable::Paths,
    ];

    /// The name written between `${` and `}`.
    pub fn name(self) -> &'static str {
        match self {
            Variable::Repo => "repo",
            Variable::Commit => "commit",
            Variable::ShortCommit => "shortCommit",
            Variable::Branch => "branch",
            Variable::Ref => "ref",
            Variable::Path => "path",
            Variable::Paths => "paths",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|variable| variable.name() == name)
    }
}

impl std::fmt::Display for Variable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${{{}}}", self.name())
    }
}

fn known_variables() -> String {
    let names: Vec<String> = Variable::ALL.iter().map(Variable::to_string).collect();
    let (last, rest) = names.split_last().expect("variables");
    format!("{} or {last}", rest.join(", "))
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TemplateError {
    #[error(transparent)]
    Split(#[from] SplitError),
    #[error("Unknown variable ${{{0}}}: use {known}.", known = known_variables())]
    UnknownVariable(String),
    #[error("A “${{” has no closing “}}”.")]
    UnclosedVariable,
    #[error("${{paths}} must be an argument of its own.")]
    PathsNotAlone,
    #[error("The working directory can't use ${{paths}}.")]
    PathsInDirectory,
}

/// A variable the command uses has no value where it was run (no commit selected, say).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("This command needs {0}, which isn't available here.")]
pub struct MissingValue(pub Variable);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Text(String),
    Var(Variable),
}

/// One argument before expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Word(Vec<Piece>);

impl Word {
    fn parse(text: &str) -> Result<Self, TemplateError> {
        let mut pieces = Vec::new();
        let mut literal = String::new();
        let mut rest = text;
        while let Some(at) = rest.find('$') {
            literal.push_str(&rest[..at]);
            let after = &rest[at + 1..];
            if let Some(escaped) = after.strip_prefix("${") {
                literal.push_str("${");
                rest = escaped;
            } else if let Some(open) = after.strip_prefix('{') {
                let close = open.find('}').ok_or(TemplateError::UnclosedVariable)?;
                let name = &open[..close];
                let variable = Variable::from_name(name)
                    .ok_or_else(|| TemplateError::UnknownVariable(name.to_string()))?;
                if !literal.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut literal)));
                }
                pieces.push(Piece::Var(variable));
                rest = &open[close + 1..];
            } else {
                literal.push('$');
                rest = after;
            }
        }
        literal.push_str(rest);
        if !literal.is_empty() || pieces.is_empty() {
            pieces.push(Piece::Text(literal));
        }
        Ok(Word(pieces))
    }

    fn uses(&self, variable: Variable) -> bool {
        self.0.contains(&Piece::Var(variable))
    }

    fn is_only(&self, variable: Variable) -> bool {
        self.0 == [Piece::Var(variable)]
    }

    fn display(&self) -> String {
        self.0
            .iter()
            .map(|piece| match piece {
                Piece::Text(text) => text.replace("${", "$${"),
                Piece::Var(variable) => variable.to_string(),
            })
            .collect()
    }

    fn expand(&self, values: &Values) -> Result<String, MissingValue> {
        let mut out = String::new();
        for piece in &self.0 {
            match piece {
                Piece::Text(text) => out.push_str(text),
                Piece::Var(variable) => out.push_str(values.single(*variable)?),
            }
        }
        Ok(out)
    }
}

/// What the variables stand for where a command runs; `None` where there is nothing to fill in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Values {
    pub repo: String,
    pub commit: Option<String>,
    pub short_commit: Option<String>,
    pub branch: Option<String>,
    pub git_ref: Option<String>,
    pub path: Option<String>,
    pub paths: Option<Vec<String>>,
}

impl Values {
    fn single(&self, variable: Variable) -> Result<&str, MissingValue> {
        let value = match variable {
            Variable::Repo => Some(&self.repo),
            Variable::Commit => self.commit.as_ref(),
            Variable::ShortCommit => self.short_commit.as_ref(),
            Variable::Branch => self.branch.as_ref(),
            Variable::Ref => self.git_ref.as_ref(),
            Variable::Path => self.path.as_ref(),
            // Only ever a word of its own (`PathsNotAlone`), expanded by `CommandTemplate`.
            Variable::Paths => None,
        };
        value.map(String::as_str).ok_or(MissingValue(variable))
    }
}

/// A parsed command line and working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandTemplate {
    words: Vec<Word>,
    directory: Option<Word>,
}

/// The program's arguments and working directory after expansion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    /// The program (first) and its arguments.
    pub args: Vec<String>,
    /// `None`: the repository's root.
    pub directory: Option<String>,
}

/// Splits `command_line` (with `style`'s quoting rules) and checks every `${…}`. A blank
/// `working_directory` is the repository's root; any other is one word, never split.
pub fn parse(
    command_line: &str,
    working_directory: Option<&str>,
    style: CommandStyle,
) -> Result<CommandTemplate, TemplateError> {
    let words = split_command_line(command_line, style)?
        .iter()
        .map(|word| Word::parse(word))
        .collect::<Result<Vec<_>, _>>()?;
    if words
        .iter()
        .any(|word| word.uses(Variable::Paths) && !word.is_only(Variable::Paths))
    {
        return Err(TemplateError::PathsNotAlone);
    }
    let directory = match working_directory
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
    {
        Some(dir) => {
            let word = Word::parse(dir)?;
            if word.uses(Variable::Paths) {
                return Err(TemplateError::PathsInDirectory);
            }
            Some(word)
        }
        None => None,
    };
    Ok(CommandTemplate { words, directory })
}

impl CommandTemplate {
    /// Every variable the command line and working directory use, in `Variable::ALL` order.
    pub fn variables(&self) -> Vec<Variable> {
        Variable::ALL
            .into_iter()
            .filter(|variable| {
                self.words
                    .iter()
                    .chain(self.directory.iter())
                    .any(|word| word.uses(*variable))
            })
            .collect()
    }

    /// The arguments as split, variables still written `${…}`: what "Test" shows.
    pub fn words(&self) -> Vec<String> {
        self.words.iter().map(Word::display).collect()
    }

    /// The program word, if it names the program without a variable (it can be looked up now).
    pub fn literal_program(&self) -> Option<&str> {
        match self.words.first().map(|word| word.0.as_slice()) {
            Some([Piece::Text(text)]) => Some(text),
            _ => None,
        }
    }

    pub fn expand(&self, values: &Values) -> Result<Expanded, MissingValue> {
        let mut args = Vec::new();
        for word in &self.words {
            if word.is_only(Variable::Paths) {
                let paths = values.paths.as_ref().ok_or(MissingValue(Variable::Paths))?;
                args.extend(paths.iter().cloned());
            } else {
                args.push(word.expand(values)?);
            }
        }
        let directory = self
            .directory
            .as_ref()
            .map(|word| word.expand(values))
            .transpose()?;
        Ok(Expanded { args, directory })
    }
}

// ---------------------------------------------------------------------------------------------
// Running a user command (#894).
// ---------------------------------------------------------------------------------------------

/// How long a user command may run before it is stopped.
pub const RUN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// How much of a user command's output (stdout and stderr together, the end) is kept.
pub const OUTPUT_LIMIT: usize = 256 * 1024;

/// Where a command runs from: what the user selected. Empty fields have nothing to fill in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    /// A commit id (full or abbreviated).
    pub commit: Option<String>,
    /// A local (`main`) or remote-tracking (`origin/main`) branch's short name.
    pub branch: Option<String>,
    /// A full ref name (`refs/tags/v1`).
    pub git_ref: Option<String>,
    /// Work tree paths, relative to the root.
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ContextError {
    #[error("The commit {0} isn't in this repository.")]
    UnknownCommit(String),
    #[error("The branch {0} isn't in this repository.")]
    UnknownBranch(String),
    #[error("The ref {0} isn't in this repository.")]
    UnknownRef(String),
    #[error("The path {0} isn't a path in the work tree.")]
    InvalidPath(String),
}

/// The variables' values for `context` in `repo`, whose work tree root (or git folder, when bare)
/// is `root`. A branch or ref also fills in `${commit}` (its tip) when no commit is given, and a
/// branch its `${ref}`; a local branch ref its `${branch}`. `${path}` is filled for exactly one
/// path, `${paths}` for one or more. Every name is checked against the repository, and every path
/// must stay inside the work tree (`launch::resolve_in_workdir`).
pub fn values(
    repo: &git2::Repository,
    root: &Path,
    context: &Context,
) -> Result<Values, ContextError> {
    let mut values = Values {
        repo: root.to_string_lossy().into_owned(),
        ..Values::default()
    };
    let mut tip = None;
    if let Some(branch) = &context.branch {
        let reference = ["refs/heads/", "refs/remotes/"]
            .iter()
            .filter_map(|prefix| repo.find_reference(&format!("{prefix}{branch}")).ok())
            .next()
            .ok_or_else(|| ContextError::UnknownBranch(branch.clone()))?;
        values.branch = Some(branch.clone());
        values.git_ref = reference.name().ok().map(str::to_string);
        tip = reference.peel_to_commit().ok();
    }
    if let Some(name) = &context.git_ref {
        let reference = repo
            .find_reference(name)
            .map_err(|_| ContextError::UnknownRef(name.clone()))?;
        if values.branch.is_none() {
            values.branch = name
                .strip_prefix("refs/heads/")
                .or_else(|| name.strip_prefix("refs/remotes/"))
                .map(str::to_string);
        }
        values.git_ref = Some(name.clone());
        tip = tip.or_else(|| reference.peel_to_commit().ok());
    }
    let commit = match &context.commit {
        Some(id) => Some(
            repo.revparse_single(id)
                .ok()
                .filter(|_| !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit()))
                .and_then(|object| object.peel_to_commit().ok())
                .ok_or_else(|| ContextError::UnknownCommit(id.clone()))?,
        ),
        None => tip,
    };
    if let Some(commit) = commit {
        values.commit = Some(commit.id().to_string());
        values.short_commit = commit
            .as_object()
            .short_id()
            .ok()
            .and_then(|buf| buf.as_str().ok().map(str::to_string));
    }
    if !context.paths.is_empty() {
        let paths = context
            .paths
            .iter()
            .map(|rel| {
                crate::launch::resolve_in_workdir(root, rel)
                    .map(|path| path.to_string_lossy().into_owned())
                    .map_err(|_| ContextError::InvalidPath(rel.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let [only] = paths.as_slice() {
            values.path = Some(only.clone());
        }
        values.paths = Some(paths);
    }
    Ok(values)
}

/// How a user command ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutcome {
    /// `None` when it was killed (time limit, or a signal).
    pub exit_code: Option<i32>,
    pub success: bool,
    pub timed_out: bool,
    /// The end of stdout and stderr together, at most `limit` bytes, escape sequences removed.
    pub output: String,
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error("The command has no program.")]
    NoProgram,
    #[error("Couldn't find the program '{0}'. Use a program on PATH or its absolute path.")]
    NotFound(String),
    #[error("The working directory {0} doesn't exist.")]
    NoDirectory(String),
    #[error("Couldn't start '{program}': {message}")]
    Start { program: String, message: String },
    #[error("Couldn't wait for the command: {0}")]
    Wait(String),
    #[error("The command was stopped.")]
    Cancelled,
}

/// Runs an expanded command: its program resolved with `find` (absolute `PATH` entries only, see
/// `process::find_program`; a relative path such as `./x` is never run), in its working directory
/// (relative to `root`; `root` itself when none), stdin closed, stdout and stderr captured
/// together, no shell. Killed with everything it started after `timeout` or when `cancel` says
/// so. On Windows it gets no console window and runs in a job object, like a hook.
pub fn run(
    expanded: &Expanded,
    root: &Path,
    find: impl Fn(&str) -> Option<PathBuf>,
    cancel: &dyn Fn() -> bool,
    timeout: Duration,
    limit: usize,
) -> Result<RunOutcome, RunError> {
    let (name, args) = expanded.args.split_first().ok_or(RunError::NoProgram)?;
    if name.is_empty() {
        return Err(RunError::NoProgram);
    }
    let program = find(name).ok_or_else(|| RunError::NotFound(name.clone()))?;
    let directory = match &expanded.directory {
        Some(dir) => root.join(dir),
        None => root.to_path_buf(),
    };
    if !directory.is_dir() {
        return Err(RunError::NoDirectory(directory.display().to_string()));
    }
    let mut command = std::process::Command::new(&program);
    command.args(args).current_dir(&directory);
    for key in REPO_PINNING_ENV {
        command.env_remove(key);
    }
    match run_captured(command, None, cancel, timeout, limit) {
        Ok(captured) => Ok(RunOutcome {
            exit_code: captured.code,
            success: captured.success,
            timed_out: captured.timed_out,
            output: captured.output,
        }),
        Err(ProcessRunError::Cancelled) => Err(RunError::Cancelled),
        Err(ProcessRunError::Spawn(error)) => Err(RunError::Start {
            program: program.display().to_string(),
            message: error.to_string(),
        }),
        Err(ProcessRunError::Wait(error)) => Err(RunError::Wait(error.to_string())),
    }
}
