//! Settings of the repository's own config file (`.git/config`) removed or moved as git removes
//! and renames a branch's or remote's section (#1181): exactly `<section>.<variable>`, never a
//! setting of an included file or of the user's config. libgit2's `git_config_rename_section`
//! (behind its branch delete and move, remote rename and delete) picks every setting whose name
//! holds `<section>.` anywhere, in every config file: branch `a.b`'s settings with branch `a`'s,
//! Browsitory's own `browsitory.remote.<name>.*`, and a setting of the user's config, which it
//! then fails to delete from the repository's file.
//!
//! A setting written without a value (`flag` alone) is removed without a value pattern, which is
//! safe. One with several values is removed with a pattern, and libgit2 crashes matching a
//! pattern against a missing value (#1180), so such a setting is refused, nothing changed.

use std::collections::BTreeSet;

use git2::{Config, ConfigLevel, Repository};

use crate::valueless_setting::ValuelessSetting;

/// Some settings of the repository's own config file, read before anything changes.
pub(crate) struct Settings {
    /// The repository's own config file (libgit2 always has one, empty when the file is
    /// missing).
    file: Config,
    /// The chosen settings written in the file itself, in file order: name and value (`None`:
    /// written without one).
    own: Vec<(String, Option<Vec<u8>>)>,
    /// The chosen names with a value anywhere in the file or the files it includes.
    present: BTreeSet<String>,
    /// The chosen names with more than one value there.
    shared: BTreeSet<String>,
}

impl Settings {
    /// The settings of the repository's own file whose names `choose` picks. Refuses one with
    /// several values, one of them written without a value: it couldn't be removed safely.
    pub(crate) fn read<E>(repo: &Repository, choose: impl Fn(&str) -> bool) -> Result<Self, E>
    where
        E: From<git2::Error> + From<ValuelessSetting>,
    {
        let file = repo.config()?.open_level(ConfigLevel::Local)?;
        let mut own = Vec::new();
        let mut present = BTreeSet::new();
        let mut shared = BTreeSet::new();
        let mut entries = file.entries(None)?;
        while let Some(entry) = entries.next() {
            let entry = entry?;
            // A name that isn't UTF-8 is never one Browsitory chooses.
            let Some(name) = entry.name().ok().filter(|name| choose(name)) else {
                continue;
            };
            if !present.insert(name.to_string()) {
                shared.insert(name.to_string());
            }
            if entry.include_depth() == 0 {
                let value = entry.has_value().then(|| entry.value_bytes().to_vec());
                own.push((name.to_string(), value));
            }
        }
        drop(entries);
        if let Some((name, _)) = own
            .iter()
            .find(|(name, value)| value.is_none() && shared.contains(name))
        {
            return Err(ValuelessSetting::MultiValued(name.clone()).into());
        }
        Ok(Settings {
            file,
            own,
            present,
            shared,
        })
    }

    /// The settings `<section>.<variable>` of section `section` (`branch.<name>`,
    /// `remote.<name>`). A variable never holds a dot; a branch or remote name may.
    pub(crate) fn section<E>(repo: &Repository, section: &str) -> Result<Self, E>
    where
        E: From<git2::Error> + From<ValuelessSetting>,
    {
        Self::read(repo, |name| variable_of(name, section).is_some())
    }

    /// Removes the settings from the file, every value of each.
    pub(crate) fn remove<E>(self) -> Result<(), E>
    where
        E: From<git2::Error>,
    {
        let mut file = self.file;
        let names: BTreeSet<&String> = self.own.iter().map(|(name, _)| name).collect();
        for name in names {
            if self.shared.contains(name) {
                file.remove_multivar(name, ".*")?;
            } else {
                file.remove(name)?;
            }
        }
        Ok(())
    }
}

/// `name`'s variable when it is a setting of section `section`.
pub(crate) fn variable_of<'a>(name: &'a str, section: &str) -> Option<&'a str> {
    name.strip_prefix(section)?
        .strip_prefix('.')
        .filter(|variable| !variable.contains('.'))
}

/// A section's settings moved to another section, as `git branch -m` and `git remote rename`
/// move them: planned (read and checked) before the ref moves, written after.
pub(crate) struct SectionMove {
    from: Settings,
    to: Settings,
    to_section: String,
    /// The settings to write under `to_section`, in the old section's order: variable and value
    /// (`true` for one written without a value, which is what git reads it as).
    values: Vec<(String, String)>,
}

impl SectionMove {
    pub(crate) fn plan<E>(repo: &Repository, from: &str, to: &str) -> Result<Self, E>
    where
        E: From<git2::Error> + From<ValuelessSetting>,
    {
        let from_settings = Settings::section::<E>(repo, from)?;
        let to_settings = Settings::section::<E>(repo, to)?;
        let mut values = Vec::new();
        for (name, value) in &from_settings.own {
            let variable = variable_of(name, from).unwrap_or_default().to_string();
            let value = match value {
                None => "true".to_string(),
                Some(bytes) => String::from_utf8(bytes.clone()).map_err(|_| {
                    git2::Error::from_str(&format!(
                        "The setting '{name}' has a value that isn't UTF-8, which Browsitory \
                         can't move. Change it (git config --edit), then try again."
                    ))
                })?,
            };
            values.push((variable, value));
        }
        Ok(SectionMove {
            from: from_settings,
            to: to_settings,
            to_section: to.to_string(),
            values,
        })
    }

    /// The settings to write, for the caller to change a value before [`Self::apply`].
    pub(crate) fn values_mut(&mut self) -> &mut [(String, String)] {
        &mut self.values
    }

    /// Writes the settings under the new section, after any it already has (as git's renamed
    /// section joins a left-over one), then removes the old section's.
    pub(crate) fn apply<E>(self) -> Result<(), E>
    where
        E: From<git2::Error>,
    {
        let mut file = self.to.file;
        // A value is added next to an existing one with a value pattern, which crashes on a
        // setting written without a value: such a setting gets the value git reads it as.
        for (name, value) in &self.to.own {
            if value.is_none() {
                file.set_str(name, "true")?;
            }
        }
        let mut present = self.to.present.clone();
        for (variable, value) in &self.values {
            let name = format!("{}.{variable}", self.to_section);
            if present.contains(&name) {
                // A pattern no value matches adds the value.
                file.set_multivar(&name, "a^", value)?;
            } else {
                file.set_str(&name, value)?;
                present.insert(name);
            }
        }
        self.from.remove()
    }
}
