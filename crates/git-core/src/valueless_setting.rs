//! Settings written without a value (`flag` alone in a config file: boolean true to git). libgit2
//! crashes the process on one, a null dereference rather than an error (#1180), when it matches a
//! value pattern against it (removing one of several values, adding a value next to it:
//! `git_remote_create`'s `fetch`) or compares its value (`branch.*.remote`, which git requires to
//! name a remote), and when it reads the merge driver or the conflict style (#1182). The checks
//! here run before such calls. Branch and remote sections are removed and moved by
//! `config_section`, which never needs a pattern for a single value.

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
    /// [`Self::Missing`], in the user's own config (`~/.gitconfig` or the XDG one, #1189).
    #[error(
        "The setting '{0}' is written without a value in your global Git config; git rejects it \
         as a bad config variable. Give it a value or remove it (git config --global --edit), \
         then try again."
    )]
    MissingInUserConfig(String),
    /// [`Self::Missing`], in the system's config.
    #[error(
        "The setting '{0}' is written without a value in the system's Git config; git rejects it \
         as a bad config variable. Give it a value or remove it (git config --system --edit), \
         then try again."
    )]
    MissingInSystemConfig(String),
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

/// The merge driver, which libgit2 reads (and copies) at the start of every merge, in memory or
/// not, unless the caller names one; git2 can't.
const MERGE_DRIVER: &str = "merge.default";
/// The conflict style, which libgit2 reads (and compares) at the start of every checkout whose
/// options name none: its own (stash save, applying a patch to the work tree, adding a worktree)
/// and Browsitory's.
const CONFLICT_STYLE: &str = "merge.conflictstyle";

/// Before a command that merges and checks out the result (merge, cherry-pick, revert, rebase,
/// stash apply): refuses a merge driver or a conflict style written without a value, which git
/// refuses too ("missing value").
pub(crate) fn refuse_for_merge<E>(repo: &Repository) -> Result<(), E>
where
    E: From<git2::Error> + From<ValuelessSetting>,
{
    refuse_valueless::<E>(repo, MERGE_DRIVER)?;
    refuse_for_checkout(repo)
}

/// Before a command that checks out: refuses a conflict style written without a value.
pub(crate) fn refuse_for_checkout<E>(repo: &Repository) -> Result<(), E>
where
    E: From<git2::Error> + From<ValuelessSetting>,
{
    refuse_valueless(repo, CONFLICT_STYLE)
}

/// Before a command that checks out in a repository it is about to create (a clone, a
/// submodule's first clone, #1189): refuses a conflict style written without a value in the
/// user's or the system's config, the only config such a repository has.
pub(crate) fn refuse_for_new_checkout<E>() -> Result<(), E>
where
    E: From<git2::Error> + From<ValuelessSetting>,
{
    let config = git2::Config::open_default()?;
    let mut entries = config.multivar(CONFLICT_STYLE, None)?;
    refuse_missing(&mut entries)
}

/// Whether a merge in memory can run: false when the merge driver is written without a value
/// (or the config can't be read).
pub(crate) fn can_merge_in_memory(repo: &Repository) -> bool {
    refuse_valueless::<Unreadable>(repo, MERGE_DRIVER).is_ok()
}

/// Any reason [`can_merge_in_memory`] says no.
struct Unreadable;

impl From<git2::Error> for Unreadable {
    fn from(_: git2::Error) -> Self {
        Unreadable
    }
}

impl From<ValuelessSetting> for Unreadable {
    fn from(_: ValuelessSetting) -> Self {
        Unreadable
    }
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
            // Named where `git config` edits it.
            let refusal = match entry.level() {
                git2::ConfigLevel::Global | git2::ConfigLevel::XDG => {
                    ValuelessSetting::MissingInUserConfig(name)
                }
                git2::ConfigLevel::System | git2::ConfigLevel::ProgramData => {
                    ValuelessSetting::MissingInSystemConfig(name)
                }
                _ => ValuelessSetting::Missing(name),
            };
            return Err(refusal.into());
        }
    }
    Ok(())
}
