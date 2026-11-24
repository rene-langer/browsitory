//! External filter drivers (#1063).
//!
//! libgit2 runs only its built-in filters (CRLF/eol, `ident`). A path whose `filter` attribute
//! names a configured driver (`filter.<name>.clean`, `.smudge`, `.process`, or `.required`),
//! such as git-crypt or Git LFS, would be staged or checked out raw: git-crypt's plaintext
//! committed, LFS pointers written over the real files. Browsitory can't run drivers, so every
//! operation that would pass such a path through a filter is refused before it writes anything.
//!
//! Checkouts are refused while libgit2 plans them, from the notify callback that
//! `hardlink::install` puts on every checkout (see [`FilterGuard`]); the other writers check
//! their paths with [`refuse_paths`] first.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::Path;
use std::rc::Rc;

use git2::{AttrCheckFlags, AttrValue, Repository};

/// A path that uses an external filter driver, which Browsitory can't run (#1063).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "'{path}' uses the '{driver}' filter (for example Git LFS or git-crypt), which Browsitory \
     can't run, so it refused this change. Use git on the command line for it instead."
)]
pub struct ExternalFilter {
    pub path: String,
    pub driver: String,
}

/// The configured driver names, as `filter=<name>` spells them (the config subsection,
/// case-sensitive), and a handle of the repository's own to look attributes up with: a
/// checkout's callbacks can't borrow the caller's (`stash_apply` takes it mutably).
#[derive(Default)]
pub(crate) struct Drivers {
    names: BTreeSet<Vec<u8>>,
    lookup: Option<Repository>,
}

const COMMANDS: [&str; 3] = ["clean", "smudge", "process"];

impl Drivers {
    /// Every driver git would run or fail on: a non-empty `clean`, `smudge` or `process`
    /// (the last value wins, as in git; an empty one is no command), or `required` true (git
    /// fails without a command; a value that isn't a boolean counts as true). All config
    /// levels count, so a global `git lfs install` does.
    pub(crate) fn load(repo: &Repository) -> Result<Self, git2::Error> {
        let config = repo.config()?.snapshot()?;
        let mut names = BTreeSet::new();
        let mut entries = config.entries(Some(r"^filter\..*\.(clean|smudge|process|required)$"))?;
        while let Some(entry) = entries.next() {
            if let Some(name) = driver_name(entry?.name_bytes()) {
                names.insert(name.to_vec());
            }
        }
        drop(entries);
        let mut drivers = BTreeSet::new();
        for name in names {
            let Ok(text) = std::str::from_utf8(&name) else {
                // Not a key git2 can look up by name; it is configured, so it counts.
                drivers.insert(name);
                continue;
            };
            let has_command = COMMANDS.iter().any(|var| {
                config
                    .get_bytes(&format!("filter.{text}.{var}"))
                    .is_ok_and(|value| !value.is_empty())
            });
            let required = match config.get_bool(&format!("filter.{text}.required")) {
                Ok(required) => required,
                Err(error) => error.code() != git2::ErrorCode::NotFound,
            };
            if has_command || required {
                drivers.insert(name);
            }
        }
        // A bare repository has no work tree to filter into or from.
        let lookup = match repo.workdir() {
            // The git folder when the work tree has no `.git` of its own (`core.worktree`).
            Some(workdir) if !drivers.is_empty() => {
                Some(Repository::open(workdir).or_else(|_| Repository::open(repo.path()))?)
            }
            _ => None,
        };
        Ok(Self {
            names: drivers,
            lookup,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The refusal for `path` (relative, `/`-separated) when its `filter` attribute names one of
    /// the drivers. The work tree's `.gitattributes` come first, then the index's, as `git add`
    /// reads them.
    pub(crate) fn refusal(&self, path: &Path) -> Result<Option<ExternalFilter>, git2::Error> {
        let Some(repo) = &self.lookup else {
            return Ok(None);
        };
        let value = repo.get_attr_bytes(path, "filter", AttrCheckFlags::FILE_THEN_INDEX)?;
        let name = match AttrValue::from_bytes(value) {
            AttrValue::String(name) => name.as_bytes(),
            AttrValue::Bytes(name) => name,
            _ => return Ok(None),
        };
        Ok(self.names.contains(name).then(|| ExternalFilter {
            path: path.to_string_lossy().replace('\\', "/"),
            driver: String::from_utf8_lossy(name).into_owned(),
        }))
    }
}

/// `<name>` of a `filter.<name>.<var>` key.
fn driver_name(key: &[u8]) -> Option<&[u8]> {
    let rest = key.strip_prefix(b"filter.")?;
    let dot = rest.iter().rposition(|&byte| byte == b'.')?;
    Some(&rest[..dot])
}

/// The files `diff` writes: added or changed on its new side, links left out (git doesn't
/// filter a symbolic link), conflicts included.
fn written_paths(diff: &git2::Diff<'_>) -> Vec<std::path::PathBuf> {
    diff.deltas()
        .filter(|delta| delta.status() != git2::Delta::Deleted)
        .filter(|delta| delta.new_file().mode() != git2::FileMode::Link)
        .filter_map(|delta| delta.new_file().path().map(Path::to_path_buf))
        .collect()
}

/// Before libgit2 checks out the in-memory result `merged` of a merge, cherry-pick or revert
/// over HEAD's `ours`: refuses a file it would write that uses a configured driver. libgit2's
/// own merge and pick go on after a refused checkout (they write the index or their state
/// files), so these are checked before they start.
pub(crate) fn refuse_merged<E>(
    repo: &Repository,
    ours: &git2::Tree<'_>,
    merged: &git2::Index,
) -> Result<(), E>
where
    E: From<ExternalFilter> + From<git2::Error>,
{
    let drivers = Drivers::load(repo)?;
    if drivers.is_empty() {
        return Ok(());
    }
    let diff = repo.diff_tree_to_index(Some(ours), Some(merged), None)?;
    refuse_with(&drivers, written_paths(&diff))
}

/// Refuses a file that `new` adds or changes over `old` (`None`: every file of `new`) and that
/// uses a configured driver.
pub(crate) fn refuse_trees<E>(
    repo: &Repository,
    old: Option<&git2::Tree<'_>>,
    new: &git2::Tree<'_>,
) -> Result<(), E>
where
    E: From<ExternalFilter> + From<git2::Error>,
{
    let drivers = Drivers::load(repo)?;
    if drivers.is_empty() {
        return Ok(());
    }
    let diff = repo.diff_tree_to_tree(old, Some(new), None)?;
    refuse_with(&drivers, written_paths(&diff))
}

/// Refuses a file the patch `diff` adds or changes that uses a configured driver: libgit2
/// applies it to the raw work-tree bytes and writes the result unfiltered.
pub(crate) fn refuse_diff<E>(repo: &Repository, diff: &git2::Diff<'_>) -> Result<(), E>
where
    E: From<ExternalFilter> + From<git2::Error>,
{
    refuse_paths(repo, written_paths(diff))
}

fn refuse_with<E, S>(drivers: &Drivers, paths: impl IntoIterator<Item = S>) -> Result<(), E>
where
    E: From<ExternalFilter> + From<git2::Error>,
    S: AsRef<Path>,
{
    for path in paths {
        if let Some(refusal) = drivers.refusal(path.as_ref())? {
            return Err(refusal.into());
        }
    }
    Ok(())
}

/// Refuses the first of `paths` (relative, `/`-separated) that uses a configured driver.
pub(crate) fn refuse_paths<E, S>(
    repo: &Repository,
    paths: impl IntoIterator<Item = S>,
) -> Result<(), E>
where
    E: From<ExternalFilter> + From<git2::Error>,
    S: AsRef<Path>,
{
    let drivers = Drivers::load(repo)?;
    if drivers.is_empty() {
        return Ok(());
    }
    refuse_with(&drivers, paths)
}

/// What a checkout's notify callback refused (see `hardlink::install`). libgit2 stops planning
/// at the refusal and writes nothing, but reports success (git2 gets a positive code, which it
/// takes for `Ok`), so the caller checks this right after the libgit2 call.
#[derive(Debug, Clone, Default)]
#[must_use = "a refused checkout reports success; check the guard after it"]
pub(crate) struct FilterGuard(Rc<RefCell<Option<ExternalFilter>>>);

impl FilterGuard {
    pub(crate) fn refuse(&self, refusal: ExternalFilter) {
        self.0.borrow_mut().get_or_insert(refusal);
    }

    pub(crate) fn refused(&self) -> bool {
        self.0.borrow().is_some()
    }

    /// `Err` when the checkout was refused.
    pub(crate) fn check(&self) -> Result<(), ExternalFilter> {
        match self.0.borrow().clone() {
            Some(refusal) => Err(refusal),
            None => Ok(()),
        }
    }
}
