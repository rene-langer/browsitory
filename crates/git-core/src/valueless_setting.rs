//! Settings written without a value (`flag` alone in a config file: boolean true to git). libgit2
//! crashes the process on one, a null dereference rather than an error (#1180), when it matches a
//! value pattern against it (removing one of several values, adding a value next to it:
//! `git_remote_create`'s `fetch`) or compares its value (`branch.*.remote`, which git requires to
//! name a remote). The checks here run before such calls. Branch and remote sections are removed
//! and moved by `config_section`, which never needs a pattern for a single value.

use git2::Repository;

/// A setting written without a value that a command can't work around; nothing changed.
#[derive(Debug, thiserror::Error)]
pub enum ValuelessSetting {
    /// One of the setting's values has none: libgit2 can only rewrite such a setting with a value
    /// pattern, which crashes on it.
    #[error(
        "The setting '{0}' has several values, one of them written without a value. Give that one \
         a value or remove it (git config --edit), then try again."
    )]
    MultiValued(String),
    /// The setting must name something (a remote, a refspec), as git requires too.
    #[error(
        "The setting '{0}' is written without a value; git rejects it as a bad config variable. \
         Give it a value or remove it (git config --edit), then try again."
    )]
    Missing(String),
}

/// The pattern libgit2's remote rename and delete match `branch.<name>.remote` with.
const BRANCH_REMOTES: &str = r"branch\..+\.remote";

/// Before a remote is renamed or removed: refuses when a `branch.<name>.remote` setting, in any
/// config file, has no value. git refuses such a setting too.
pub(crate) fn refuse_valueless_branch_remotes<E>(repo: &Repository) -> Result<(), E>
where
    E: From<git2::Error> + From<ValuelessSetting>,
{
    let config = repo.config()?;
    let mut entries = config.entries(Some(BRANCH_REMOTES))?;
    refuse_missing(&mut entries)
}

/// Refuses when setting `name` has a value written without one, in any config file.
pub(crate) fn refuse_valueless<E>(repo: &Repository, name: &str) -> Result<(), E>
where
    E: From<git2::Error> + From<ValuelessSetting>,
{
    let config = repo.config()?;
    let mut entries = config.multivar(name, None)?;
    refuse_missing(&mut entries)
}

fn refuse_missing<E>(entries: &mut git2::ConfigEntries<'_>) -> Result<(), E>
where
    E: From<git2::Error> + From<ValuelessSetting>,
{
    while let Some(entry) = entries.next() {
        let entry = entry?;
        if !entry.has_value() {
            let name = String::from_utf8_lossy(entry.name_bytes()).into_owned();
            return Err(ValuelessSetting::Missing(name).into());
        }
    }
    Ok(())
}
