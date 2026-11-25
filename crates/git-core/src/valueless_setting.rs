//! Settings written without a value (`flag` alone in a config file: boolean true to git). libgit2
//! crashes the process on one, a null dereference rather than an error (#1180), when it rewrites
//! the setting with a value pattern: removing or renaming its section (`git_branch_delete`,
//! `git_branch_move`, `git_remote_rename`, `git_remote_delete`) or adding a value next to it
//! (`git_remote_create`'s `fetch`). `git_remote_rename` and `git_remote_delete` also compare every
//! `branch.*.remote` with the remote's name. The checks here run before those calls.

use git2::{ConfigLevel, ErrorCode, Repository};

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

/// Before libgit2 removes or renames the config sections `sections` (`branch.<name>`,
/// `remote.<name>`): writes each setting it would rewrite that has no value as `true`, what git
/// reads it as. libgit2 rewrites every setting whose name holds `<section>.`, so for branch `a`
/// branch `a.b`'s too. Only the repository's own files, the ones libgit2 writes, are changed; a
/// setting from an included or user-wide file is never in the text it rewrites. Refuses, writing
/// nothing, when such a setting also has other values.
pub(crate) fn spell_out_sections<E>(repo: &Repository, sections: &[String]) -> Result<(), E>
where
    E: From<git2::Error> + From<ValuelessSetting>,
{
    let prefixes: Vec<String> = sections
        .iter()
        .map(|section| format!("{section}."))
        .collect();
    let mut writes = Vec::new();
    for level in [ConfigLevel::Local, ConfigLevel::Worktree] {
        let file = match repo.config()?.open_level(level) {
            Ok(file) => file,
            Err(error) if error.code() == ErrorCode::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        // (name, written in this file without a value)
        let mut names = Vec::new();
        let mut entries = file.entries(None)?;
        while let Some(entry) = entries.next() {
            let entry = entry?;
            names.push((
                entry.name_bytes().to_vec(),
                !entry.has_value() && entry.include_depth() == 0,
            ));
        }
        drop(entries);
        let mut missing = Vec::new();
        for (name, valueless) in &names {
            let rewritten = prefixes
                .iter()
                .any(|prefix| contains(name, prefix.as_bytes()));
            if !valueless || !rewritten {
                continue;
            }
            let text = String::from_utf8_lossy(name).into_owned();
            if names.iter().filter(|(other, _)| other == name).count() > 1 {
                return Err(ValuelessSetting::MultiValued(text).into());
            }
            // A name that isn't UTF-8 can't be written back through git2.
            if std::str::from_utf8(name).is_err() {
                return Err(ValuelessSetting::Missing(text).into());
            }
            missing.push(text);
        }
        writes.push((file, missing));
    }
    for (mut file, missing) in writes {
        for name in missing {
            file.set_str(&name, "true")?;
        }
    }
    Ok(())
}

/// Before libgit2 renames or deletes a remote: refuses when a `branch.<name>.remote` setting, in
/// any config file, has no value. git refuses such a setting too.
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

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
