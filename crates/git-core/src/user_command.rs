//! User-defined commands (#893): a command line with `${…}` variables, split into argv like the
//! external tools' commands (`process::split_command_line`, never a shell), then expanded word by
//! word. A value never splits into more words and is never read as quoting, so a branch or file
//! name with spaces, quotes or shell syntax stays exactly one argument; only a `${paths}` word of
//! its own becomes one argument per path.
//!
//! `$${` stands for a literal `${`; any other `$` is an ordinary character.

use serde::Serialize;
use thiserror::Error;

use crate::process::{split_command_line, CommandStyle, SplitError};

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
