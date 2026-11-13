//! User-defined commands (#893): a command line with `${…}` variables, split into argv like the
//! external tools' commands (`process::split_command_line`, never a shell), then expanded word by
//! word. A value never splits into more words and is never read as quoting, so a branch or file
//! name with spaces, quotes or shell syntax stays exactly one argument; only a `${paths}` word of
//! its own becomes one argument per path.
//!
//! `$${` stands for a literal `${`; any other `$` is an ordinary character.

use serde::Serialize;
use thiserror::Error;

use crate::process::{CommandStyle, SplitError};

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

/// A parsed command line and working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandTemplate {}

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
    let _ = (command_line, working_directory, style);
    todo!()
}

impl CommandTemplate {
    pub fn variables(&self) -> Vec<Variable> {
        todo!()
    }

    pub fn words(&self) -> Vec<String> {
        todo!()
    }

    pub fn literal_program(&self) -> Option<&str> {
        todo!()
    }

    pub fn expand(&self, values: &Values) -> Result<Expanded, MissingValue> {
        let _ = values;
        todo!()
    }
}
