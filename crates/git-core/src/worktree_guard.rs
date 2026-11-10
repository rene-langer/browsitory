//! Every "never through a symlink" rule for the working tree (#114, #138).
//!
//! libgit2's *forced* checkouts write a blob through a working-tree symlink that replaced a
//! tracked file (a typechange) and through a symlink that replaced a tracked directory, so the
//! write can land outside the repository. Callers run [`scan`] + [`clear_hazards`] right before
//! each forced checkout: only the links themselves are removed (never their targets), and only
//! links a path of the checkout's source goes through. Browsitory's own raw writes and deletes
//! use [`write_beneath`] / [`remove_beneath`], and work-tree reads use [`open_beneath`].
//!
//! Residual risk: a concurrent local process can still swap a link in between
//! [`clear_hazards`] and libgit2's write; libgit2 offers no `openat`-style checkout.

use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::{Component, Path, PathBuf};

#[cfg(unix)]
mod beneath_unix;
#[cfg(windows)]
mod beneath_windows;
#[cfg(any(windows, test))]
mod reparse;
#[cfg(unix)]
use beneath_unix as beneath;
#[cfg(windows)]
use beneath_windows as beneath;

#[cfg(any(unix, windows))]
mod walk_types {
    //! The shared vocabulary of the platform walkers (`beneath_unix`, `beneath_windows`).
    use std::ffi::OsStr;

    /// How a walk treats a link where it needs a folder.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum OnLink {
        /// Stop: the path is beyond a link.
        Refuse,
        /// Remove the link itself (never its target), then go on as [`OnMissing`] says.
        Clear,
    }

    /// How a walk treats a missing folder.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum OnMissing {
        Stop,
        /// Create it.
        Create,
    }

    /// A walk's progress, for tests that change the work tree at a chosen moment.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Step<'a> {
        /// The folder was opened and is now held.
        Opened(&'a OsStr),
        Created(&'a OsStr),
        Cleared(&'a OsStr),
    }

    /// The walk's test seam; production passes [`no_step`].
    pub(crate) type StepFn<'s> = &'s mut dyn FnMut(Step<'_>);

    pub(crate) fn no_step(_: Step<'_>) {}

    /// Where a walk ended.
    pub(crate) enum Walk<B> {
        Found(B),
        /// A component on the way is (or, with [`OnLink::Clear`] and [`OnMissing::Stop`], was) a
        /// link.
        Link,
        /// A component on the way is missing ([`OnMissing::Stop`]).
        Missing,
        /// A component on the way is a file or something else that isn't a folder.
        NotDir,
    }
}
#[cfg(any(unix, windows))]
use walk_types::{no_step, OnLink, OnMissing, Step, StepFn, Walk};

/// Git's file modes for a symlink and a submodule (gitlink).
const MODE_LINK: u32 = 0o120000;
const MODE_GITLINK: u32 = 0o160000;

/// What a forced checkout is about to write: the entries of a tree or of the index.
pub(crate) enum GuardSource<'a> {
    Tree(&'a git2::Tree<'a>),
    Index(&'a git2::Index),
}

/// A working-tree symlink a forced checkout would write through.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Hazard {
    /// The path is a symlink in the worktree but a regular or executable file in the source.
    Leaf(PathBuf),
    /// A strict ancestor directory of a source path is a symlink (or a Windows junction).
    Ancestor(PathBuf),
}

impl Hazard {
    fn path(&self) -> &Path {
        match self {
            Hazard::Leaf(path) | Hazard::Ancestor(path) => path,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Link,
    Dir,
    /// Missing, a file, or unreadable: nothing below it exists for a checkout to write through.
    Other,
}

fn entry_kind(full: &Path) -> Entry {
    match std::fs::symlink_metadata(full) {
        Ok(meta) if meta.file_type().is_symlink() => Entry::Link,
        Ok(meta) if meta.is_dir() => Entry::Dir,
        _ => Entry::Other,
    }
}

fn is_link(full: &Path) -> bool {
    entry_kind(full) == Entry::Link
}

/// A relative path made only of normal components (no `..`, `.`, root or prefix).
fn plain_relative(rel: &Path) -> bool {
    rel.components().next().is_some() && rel.components().all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(unix)]
pub(crate) fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
pub(crate) fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

/// `path` (git's `/`-separated bytes) is one of `paths` or below one of them, the way a forced
/// checkout with `CheckoutBuilder::path` + `disable_pathspec_match` selects it.
/// With `ignore_case` (`core.ignorecase`, as libgit2's pathspec matching) ASCII case is ignored.
fn selected<P: AsRef<[u8]>>(path: &[u8], paths: &[P], ignore_case: bool) -> bool {
    let same = |a: &[u8], b: &[u8]| {
        if ignore_case {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };
    paths.iter().any(|wanted| {
        let mut wanted = wanted.as_ref();
        while let Some(rest) = wanted.strip_suffix(b"/") {
            wanted = rest;
        }
        path.len() >= wanted.len()
            && same(&path[..wanted.len()], wanted)
            && (path.len() == wanted.len() || path[wanted.len()] == b'/')
    })
}

/// Lists the hazards for a forced checkout of `source` (optionally limited to the literal
/// `paths`), without touching the worktree. Each directory is examined once.
///
/// A source entry that is itself a symlink is not a leaf hazard (libgit2 unlinks and recreates
/// links, which never follows), and submodule entries are skipped. A symlink no source path goes
/// through is the user's and is never reported.
#[cfg(test)]
pub(crate) fn scan<P: AsRef<[u8]>>(
    workdir: &Path,
    source: &GuardSource,
    paths: Option<&[P]>,
) -> Result<Vec<Hazard>, git2::Error> {
    scan_with_case(workdir, source, paths, false)
}

/// `scan` with the `paths` filter ignoring ASCII case when `ignore_case`.
#[cfg(test)]
fn scan_with_case<P: AsRef<[u8]>>(
    workdir: &Path,
    source: &GuardSource,
    paths: Option<&[P]>,
    ignore_case: bool,
) -> Result<Vec<Hazard>, git2::Error> {
    let mut hazards = BTreeSet::new();
    Scanner::new(workdir, true).scan(source, paths, ignore_case, &mut hazards)?;
    Ok(hazards.into_iter().collect())
}

/// Scans several sources against one worktree, examining each directory (and, with `leaves`,
/// each file) once across all of them.
struct Scanner<'w> {
    workdir: &'w Path,
    /// Whether leaf hazards matter; when only folder links do, files are never examined.
    leaves: bool,
    dirs: HashMap<PathBuf, Entry>,
    files: HashMap<PathBuf, bool>,
}

impl<'w> Scanner<'w> {
    fn new(workdir: &'w Path, leaves: bool) -> Self {
        Self {
            workdir,
            leaves,
            dirs: HashMap::new(),
            files: HashMap::new(),
        }
    }

    fn scan<P: AsRef<[u8]>>(
        &mut self,
        source: &GuardSource,
        paths: Option<&[P]>,
        ignore_case: bool,
        hazards: &mut BTreeSet<Hazard>,
    ) -> Result<(), git2::Error> {
        let owned;
        let index: &git2::Index = match source {
            GuardSource::Index(index) => index,
            GuardSource::Tree(tree) => {
                let mut index = git2::Index::new()?;
                index.read_tree(tree)?;
                owned = index;
                &owned
            }
        };
        for entry in index.iter() {
            if entry.mode == MODE_GITLINK {
                continue;
            }
            if paths.is_some_and(|paths| !selected(&entry.path, paths, ignore_case)) {
                continue;
            }
            let rel = path_from_bytes(&entry.path);
            if !plain_relative(&rel) {
                continue;
            }
            let mut blocked = false;
            if let Some(parent) = rel.parent() {
                let mut current = PathBuf::new();
                for component in parent.components() {
                    current.push(component);
                    let kind = match self.dirs.get(&current) {
                        Some(kind) => *kind,
                        None => {
                            let kind = entry_kind(&self.workdir.join(&current));
                            self.dirs.insert(current.clone(), kind);
                            kind
                        }
                    };
                    match kind {
                        Entry::Dir => {}
                        Entry::Link => {
                            hazards.insert(Hazard::Ancestor(current));
                            blocked = true;
                            break;
                        }
                        Entry::Other => {
                            blocked = true;
                            break;
                        }
                    }
                }
            }
            if blocked || !self.leaves || entry.mode == MODE_LINK {
                continue;
            }
            let workdir = self.workdir;
            let linked = *self
                .files
                .entry(rel.clone())
                .or_insert_with(|| is_link(&workdir.join(&rel)));
            if linked {
                hazards.insert(Hazard::Leaf(rel));
            }
        }
        Ok(())
    }
}

/// True when a symlink replaced a file or folder that HEAD or the index tracks: what a forced
/// checkout of either would have to clear.
pub(crate) fn has_hazard(repo: &git2::Repository) -> Result<bool, git2::Error> {
    let Some(workdir) = repo.workdir() else {
        return Ok(false);
    };
    const ALL: Option<&[&str]> = None;
    let mut scanner = Scanner::new(workdir, true);
    let mut hazards = BTreeSet::new();
    if let Ok(head_tree) = repo.head().and_then(|head| head.peel_to_tree()) {
        scanner.scan(&GuardSource::Tree(&head_tree), ALL, false, &mut hazards)?;
        if !hazards.is_empty() {
            return Ok(true);
        }
    }
    scanner.scan(
        &GuardSource::Index(&repo.index()?),
        ALL,
        false,
        &mut hazards,
    )?;
    Ok(!hazards.is_empty())
}

/// Removes each hazard's link, never its target and never recursively. A path that is no longer
/// a symlink (or already gone) is left alone. Call immediately before the forced checkout.
///
/// The walk to each hazard holds every folder on the way: a folder that became a link meanwhile
/// is itself removed (it would be a hazard too), never followed.
pub(crate) fn clear_hazards(workdir: &Path, hazards: &[Hazard]) -> io::Result<()> {
    for hazard in hazards {
        beneath::clear_hazard(workdir, hazard.path(), &mut no_step)?;
    }
    Ok(())
}

/// A symlink stands where the repository, or what an operation brings in, has a folder.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("'{0}' is a symbolic link where the repository has a folder. Remove or rename it first.")]
pub struct LinkedFolder(pub String);

/// A link (or submodule) entry and a folder of another entry name the same file once case is
/// ignored (#156). On a case-insensitive file system a write below the folder could go through
/// the link.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "'{link}' is a symbolic link and '{folder}' a folder whose names differ only in case. On \
     this file system they are the same name, so Browsitory won't check them out. Rename one of \
     them in the repository."
)]
pub struct CaseCollision {
    pub link: String,
    pub folder: String,
}

/// Two tracked paths that differ only in ASCII case (`A` and `a`, committed on a case-sensitive
/// drive) while `core.ignorecase` is on (#403). libgit2's ignore-case index takes them for one
/// entry: a forced checkout that includes them keeps only one in the index and, depending on
/// the order the directory lists them in, deletes the other's file (#392).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "'{path}' and '{other}' differ only in case, which this repository's core.ignorecase \
     setting takes for one name, so the checkout would remove one of them. Rename one of the \
     two in the repository first."
)]
pub struct CasePair {
    pub path: String,
    pub other: String,
}

/// A merge-like step (a rebase step, cherry-pick, revert, merge, pull or stash apply) whose
/// result holds a case pair (see [`CasePair`]) that HEAD doesn't, while `core.ignorecase` is on
/// (#522): the commits or the stash being applied add `a` next to HEAD's `A`. libgit2's
/// in-memory result holds both, but its ignore-case index takes `a` for `A`'s entry, so the
/// index, and the commit made from it, would keep only one of them.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "'{path}' and '{other}' differ only in case, which this repository's core.ignorecase \
     setting takes for one name, so the result would keep only one of them. Rename one of the \
     two where it was added, then try again."
)]
pub struct CasePairBroughtIn {
    pub path: String,
    pub other: String,
}

/// Refuses a merge-like step whose `result` (the in-memory `cherrypick_commit`,
/// `revert_commit`, `merge_commits` or `merge_trees`, or the tree a fast-forward checks out)
/// holds a case pair that `base` (HEAD's tree, which the step lands on) doesn't, when
/// `core.ignorecase` is on (#522). Switches, checkouts and a new worktree's checkout (no `base`)
/// use it too (#532). Call it before the step writes anything. A pair `base`
/// already holds is not this check's (#512); a case-only rename (`A` in `base`, only `a` in
/// `result`) is no pair and goes ahead.
pub(crate) fn refuse_case_pair_brought_in<E>(
    repo: &git2::Repository,
    result: &GuardSource,
    base: Option<&git2::Tree>,
) -> Result<(), E>
where
    E: From<git2::Error> + From<CasePairBroughtIn> + From<CaseConfigUnreadable>,
{
    if !core_ignorecase(repo)? {
        return Ok(());
    }
    let held: BTreeSet<Vec<u8>> = match base {
        Some(base) => case_pairs(&[&GuardSource::Tree(base)])?
            .into_iter()
            .filter_map(|names| names.first().map(|name| name.to_ascii_lowercase()))
            .collect(),
        None => BTreeSet::new(),
    };
    let brought_in = case_pairs(&[result])?.into_iter().find(|names| {
        names
            .first()
            .is_some_and(|name| !held.contains(&name.to_ascii_lowercase()))
    });
    match brought_in {
        Some(names) => {
            let mut names = names
                .into_iter()
                .map(|name| String::from_utf8_lossy(&name).into_owned());
            Err(CasePairBroughtIn {
                path: names.next().unwrap_or_default(),
                other: names.next().unwrap_or_default(),
            }
            .into())
        }
        None => Ok(()),
    }
}

/// The case pairs (see [`CasePair`]) among the entries of `sources`: groups of whole paths that
/// libgit2's ignore-case index takes for one (byte by byte, ASCII letters folded), each sorted.
/// Submodule entries count too: the index holds them like files.
pub(crate) fn case_pairs(sources: &[&GuardSource]) -> Result<Vec<BTreeSet<Vec<u8>>>, git2::Error> {
    let mut names = std::collections::BTreeMap::<Vec<u8>, BTreeSet<Vec<u8>>>::new();
    let mut add = |path: Vec<u8>| {
        names
            .entry(path.to_ascii_lowercase())
            .or_default()
            .insert(path);
    };
    for source in sources {
        match source {
            GuardSource::Index(index) => index.iter().for_each(|entry| add(entry.path)),
            GuardSource::Tree(tree) => {
                tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
                    if entry.kind() != Some(git2::ObjectType::Tree) {
                        add([root.as_bytes(), entry.name_bytes()].concat());
                    }
                    git2::TreeWalkResult::Ok
                })?;
            }
        }
    }
    Ok(names
        .into_values()
        .filter(|names| names.len() > 1)
        .collect())
}

/// [`case_pairs`] limited to the names a forced checkout of the literal `paths` selects
/// (case-insensitively, as `core.ignorecase` makes it). A name of a pair is selected exactly
/// when the other is, so only selected names are collected: the index is filtered, and a tree
/// walk skips every folder that neither leads to nor lies below a selected path (#512). On
/// 100,000 files, discarding one file no longer walks two whole trees.
fn selected_case_pairs<P: AsRef<[u8]>>(
    sources: &[&GuardSource],
    paths: &[P],
) -> Result<Vec<BTreeSet<Vec<u8>>>, git2::Error> {
    let wanted: Vec<Vec<u8>> = paths
        .iter()
        .map(|path| {
            let mut path = path.as_ref();
            while let Some(rest) = path.strip_suffix(b"/") {
                path = rest;
            }
            path.to_ascii_lowercase()
        })
        .collect();
    // `folded` is selected: a wanted path or below one.
    let inside = |folded: &[u8]| {
        wanted.iter().any(|wanted| {
            folded.starts_with(wanted)
                && (folded.len() == wanted.len() || folded[wanted.len()] == b'/')
        })
    };
    // `folded` is a folder on the way to a wanted path.
    let on_the_way = |folded: &[u8]| {
        wanted.iter().any(|wanted| {
            wanted.starts_with(folded)
                && wanted.len() > folded.len()
                && wanted[folded.len()] == b'/'
        })
    };
    let mut names = std::collections::BTreeMap::<Vec<u8>, BTreeSet<Vec<u8>>>::new();
    let mut add = |path: Vec<u8>| {
        names
            .entry(path.to_ascii_lowercase())
            .or_default()
            .insert(path);
    };
    for source in sources {
        match source {
            GuardSource::Index(index) => index
                .iter()
                .filter(|entry| inside(&entry.path.to_ascii_lowercase()))
                .for_each(|entry| add(entry.path)),
            GuardSource::Tree(tree) => {
                tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
                    let path = [root.as_bytes(), entry.name_bytes()].concat();
                    let folded = path.to_ascii_lowercase();
                    if entry.kind() == Some(git2::ObjectType::Tree) {
                        if inside(&folded) || on_the_way(&folded) {
                            git2::TreeWalkResult::Ok
                        } else {
                            git2::TreeWalkResult::Skip
                        }
                    } else {
                        if inside(&folded) {
                            add(path);
                        }
                        git2::TreeWalkResult::Ok
                    }
                })?;
            }
        }
    }
    Ok(names
        .into_values()
        .filter(|names| names.len() > 1)
        .collect())
}

/// The first of [`case_pairs`], optionally only one a forced checkout limited to the literal
/// `paths` would select.
fn find_case_pair<P: AsRef<[u8]>>(
    sources: &[&GuardSource],
    paths: Option<&[P]>,
) -> Result<Option<CasePair>, git2::Error> {
    let pairs = match paths {
        Some(paths) => selected_case_pairs(sources, paths)?,
        None => case_pairs(&distinct_trees(sources))?,
    };
    let pair = pairs.into_iter().next();
    Ok(pair.map(|names| {
        let mut names = names
            .into_iter()
            .map(|name| String::from_utf8_lossy(&name).into_owned());
        CasePair {
            path: names.next().unwrap_or_default(),
            other: names.next().unwrap_or_default(),
        }
    }))
}

/// `sources` without a tree another source already is (the target of a discard or a hard reset
/// to HEAD is HEAD's tree): walking it twice finds no other pair.
fn distinct_trees<'s, 'a>(sources: &[&'s GuardSource<'a>]) -> Vec<&'s GuardSource<'a>> {
    let mut seen = Vec::new();
    sources
        .iter()
        .copied()
        .filter(|source| match source {
            GuardSource::Tree(tree) if seen.contains(&tree.id()) => false,
            GuardSource::Tree(tree) => {
                seen.push(tree.id());
                true
            }
            GuardSource::Index(_) => true,
        })
        .collect()
}

/// Refuses a forced checkout of `sources` (optionally limited to `paths`) that would include a
/// case pair, when `core.ignorecase` is on (#403). HEAD's tree and the index count as well as
/// the target: the checkout compares against them, and a pair in either was dropped. Writes
/// nothing.
pub(crate) fn refuse_case_pair<E, P>(
    repo: &git2::Repository,
    target: Option<&GuardSource>,
    paths: Option<&[P]>,
) -> Result<(), E>
where
    E: From<git2::Error> + From<CasePair> + From<CaseConfigUnreadable>,
    P: AsRef<[u8]>,
{
    if !core_ignorecase(repo)? {
        return Ok(());
    }
    let head_tree = repo.head().and_then(|head| head.peel_to_tree()).ok();
    let head = head_tree.as_ref().map(GuardSource::Tree);
    let index = repo.index()?;
    let index = GuardSource::Index(&index);
    let sources: Vec<&GuardSource> = target
        .into_iter()
        .chain(head.as_ref())
        .chain(Some(&index))
        .collect();
    match find_case_pair(&sources, paths)? {
        Some(pair) => Err(pair.into()),
        None => Ok(()),
    }
}

/// The case pairs an abort meets (#511): paths that differ only in ASCII case, among the
/// target's, HEAD's and the index's entries and the files on disk, while `core.ignorecase` is
/// on. A picked commit can bring one in that the operation couldn't check up front: libgit2's
/// merge then records one name in the index and writes the other's file, so the pair may be
/// split between HEAD, the index and the disk. An abort leaves every name of a pair out of its
/// forced checkout (which would drop one of them from the index or delete the other's file,
/// #403) and puts them back with [`restore_case_pairs`].
#[derive(Debug, Default)]
pub(crate) struct CasePairs {
    /// Names the target, HEAD or the index holds.
    tracked: BTreeSet<Vec<u8>>,
    /// Names only on disk.
    untracked: BTreeSet<Vec<u8>>,
}

impl CasePairs {
    pub(crate) fn is_empty(&self) -> bool {
        self.tracked.is_empty() && self.untracked.is_empty()
    }

    /// The names a source (the target, HEAD or the index) holds.
    pub(crate) fn tracked(&self) -> impl Iterator<Item = &[u8]> {
        self.tracked.iter().map(Vec::as_slice)
    }

    /// Whether a forced checkout must leave `path` out (a name a source holds).
    pub(crate) fn contains(&self, path: &[u8]) -> bool {
        self.tracked.contains(path)
    }

    /// Whether `path` is one of the pairs' names in any spelling (libgit2's diff under
    /// `core.ignorecase` may report either).
    pub(crate) fn covers(&self, path: &[u8]) -> bool {
        let folded = path.to_ascii_lowercase();
        self.tracked
            .iter()
            .chain(&self.untracked)
            .any(|name| name.to_ascii_lowercase() == folded)
    }
}

/// For a rewind that must not lose local changes (`pick::rewind_to`, like `git reset --merge`):
/// the first name of `pairs` whose file [`restore_case_pairs`] would overwrite or remove
/// although it holds something that neither `head` nor `target` has under that very name. Only
/// names a source holds count: `restore_case_pairs` removes a name only on disk only when its
/// content is a blob the repository holds (what the operation wrote from a commit) and keeps
/// anything else. `None` when rewinding loses nothing.
pub(crate) fn case_pair_with_local_change<E>(
    repo: &git2::Repository,
    pairs: &CasePairs,
    head: &git2::Tree,
    target: &git2::Tree,
) -> Result<Option<String>, E>
where
    E: From<git2::Error> + From<io::Error>,
{
    let Some(workdir) = repo.workdir() else {
        return Ok(None);
    };
    for path in &pairs.tracked {
        let Ok(rel) = std::str::from_utf8(path) else {
            continue;
        };
        let content = match read_beneath(workdir, rel)? {
            Some(Leaf::File { bytes, .. }) => bytes,
            Some(Leaf::Link { target, .. }) => target,
            None => continue,
        };
        let id = git2::Oid::hash_object(git2::ObjectType::Blob, &content)?;
        let committed = [head, target].iter().any(|tree| {
            tree.get_path(&path_from_bytes(path))
                .is_ok_and(|entry| entry.id() == id)
        });
        if !committed {
            return Ok(Some(rel.to_string()));
        }
    }
    Ok(None)
}

/// The [`CasePairs`] of an abort's forced checkout of `target` (optionally limited to the
/// literal `paths`, matched ignoring case): empty unless `core.ignorecase` is on.
pub(crate) fn case_pairs_for_abort<E, P>(
    repo: &git2::Repository,
    target: &GuardSource,
    paths: Option<&[P]>,
) -> Result<CasePairs, E>
where
    E: From<git2::Error> + From<CaseConfigUnreadable>,
    P: AsRef<[u8]>,
{
    let mut pairs = CasePairs::default();
    if !core_ignorecase(repo)? {
        return Ok(pairs);
    }
    let Some(workdir) = repo.workdir() else {
        return Ok(pairs);
    };
    let head_tree = repo.head().and_then(|head| head.peel_to_tree()).ok();
    let mut index = repo.index()?;
    index.read(false)?;
    let mut entries: Vec<Vec<u8>> = Vec::new();
    for tree in [target_tree(target), head_tree.as_ref()]
        .into_iter()
        .flatten()
    {
        let mut flat = git2::Index::new()?;
        flat.read_tree(tree)?;
        entries.extend(flat.iter().map(|entry| entry.path));
    }
    if let GuardSource::Index(target) = target {
        entries.extend(target.iter().map(|entry| entry.path));
    }
    entries.extend(index.iter().map(|entry| entry.path));
    // Folded name -> (spelling -> whether a source holds it).
    let mut names = std::collections::BTreeMap::<Vec<u8>, HashMap<Vec<u8>, bool>>::new();
    for path in entries {
        if paths.is_none_or(|paths| selected(&path, paths, true)) {
            names
                .entry(path.to_ascii_lowercase())
                .or_default()
                .insert(path, true);
        }
    }
    // The names on disk below the folders the sources name (any spelling of them).
    let folders: BTreeSet<Vec<u8>> = names
        .keys()
        .flat_map(|name| {
            name.iter()
                .enumerate()
                .filter(|(_, byte)| **byte == b'/')
                .map(|(at, _)| name[..at].to_vec())
                .collect::<Vec<_>>()
        })
        .collect();
    let mut on_disk = Vec::new();
    disk_names(workdir, Vec::new(), &folders, &names, &mut on_disk);
    for name in on_disk {
        if let Some(spellings) = names.get_mut(&name.to_ascii_lowercase()) {
            spellings.entry(name).or_insert(false);
        }
    }
    for spellings in names.into_values().filter(|spellings| spellings.len() > 1) {
        for (name, held) in spellings {
            if held {
                pairs.tracked.insert(name);
            } else {
                pairs.untracked.insert(name);
            }
        }
    }
    Ok(pairs)
}

fn target_tree<'a>(source: &'a GuardSource) -> Option<&'a git2::Tree<'a>> {
    match source {
        GuardSource::Tree(tree) => Some(tree),
        GuardSource::Index(_) => None,
    }
}

/// Collects into `out` the names on disk below the work tree folder `prefix` (its on-disk
/// spelling, `/`-separated, empty for the root) whose folded name is one of `names`, walking
/// only into folders whose folded name is one of `folders`. Never follows a link; `.git` is
/// skipped. A folder that can't be read is skipped too: an abort must not stop at it.
fn disk_names<V>(
    workdir: &Path,
    prefix: Vec<u8>,
    folders: &BTreeSet<Vec<u8>>,
    names: &std::collections::BTreeMap<Vec<u8>, V>,
    out: &mut Vec<Vec<u8>>,
) {
    let Ok(entries) = std::fs::read_dir(workdir.join(path_from_bytes(&prefix))) else {
        return;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if prefix.is_empty() && file_name.eq_ignore_ascii_case(".git") {
            continue;
        }
        let mut path = prefix.clone();
        if !path.is_empty() {
            path.push(b'/');
        }
        path.extend_from_slice(file_name.as_bytes());
        let folded = path.to_ascii_lowercase();
        if names.contains_key(&folded) {
            out.push(path.clone());
        }
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) && folders.contains(&folded) {
            disk_names(workdir, path, folders, names, out);
        }
    }
}

/// Puts every name of `pairs` back as `tree` has it, written directly rather than by libgit2's
/// checkout (#403, #511): the blob (a link as a link where `core.symlinks` allows), or nothing
/// when `tree` lacks the name (the folders that emptied are removed too). A name only on disk
/// is removed only when its content is a blob the repository holds (what the operation wrote
/// from a commit, which stays there); anything else may be the user's own file and stays. No clean or smudge filter runs,
/// as for undo's raw writes. Never written through a link. A submodule, and a folder where
/// `tree` has no file, are left as they are. The index is the caller's.
pub(crate) fn restore_case_pairs<E>(
    repo: &git2::Repository,
    tree: &git2::Tree,
    pairs: &CasePairs,
) -> Result<(), E>
where
    E: From<git2::Error> + From<io::Error>,
{
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    // Removals first: on a drive where the names fold, a write would land in the other name.
    let odb = repo.odb()?;
    for path in &pairs.untracked {
        let Ok(rel) = std::str::from_utf8(path) else {
            continue;
        };
        let wrote = match read_beneath(workdir, rel)? {
            Some(Leaf::File { bytes, .. }) => {
                odb.exists(git2::Oid::hash_object(git2::ObjectType::Blob, &bytes)?)
            }
            Some(Leaf::Link { target, .. }) => {
                odb.exists(git2::Oid::hash_object(git2::ObjectType::Blob, &target)?)
            }
            None => false,
        };
        if wrote {
            remove_and_prune_beneath(workdir, Path::new(rel))?;
        }
    }
    let symlinks = repo
        .config()?
        .get_bool("core.symlinks")
        .unwrap_or(cfg!(unix));
    let mut writes = Vec::new();
    for path in &pairs.tracked {
        let rel = path_from_bytes(path);
        match tree.get_path(&rel) {
            Ok(entry) => writes.push((path, entry.id(), entry.filemode() as u32)),
            Err(error) if error.code() == git2::ErrorCode::NotFound => {
                let folder = rel
                    .to_str()
                    .map(|rel| probe_beneath(workdir, rel))
                    .transpose()?
                    .is_some_and(|probe| matches!(probe, Probe::Folder(_)));
                if !folder {
                    remove_and_prune_beneath(workdir, &rel)?;
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    for (path, id, mode) in writes {
        let Some(rel) = std::str::from_utf8(path)
            .ok()
            .filter(|_| mode != MODE_GITLINK)
        else {
            continue;
        };
        let blob = repo.find_blob(id)?;
        let content = if mode == MODE_LINK && symlinks {
            Content::Link(blob.content())
        } else {
            Content::File {
                bytes: blob.content(),
                executable: mode == 0o100755,
            }
        };
        replace_beneath(workdir, rel, content)?;
    }
    Ok(())
}

/// The code points HFS+ ignores in names (the list git's `is_hfs_dotgit` uses).
pub(crate) fn hfs_ignorable(c: char) -> bool {
    matches!(
        c,
        '\u{200c}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{206a}'..='\u{206f}' | '\u{feff}'
    )
}

/// `name` (git's bytes) as a case-insensitive file system compares it. Valid UTF-8 is
/// NFD-normalized (APFS and HFS+ compare names after normalization, so `é` as U+00E9 and as
/// `e` + U+0301 are one name; #166), stripped of the HFS+ ignorable code points, lower-, upper-
/// then lowercased per character (so `ſ`/`s`, `ς`/`σ`, `µ`/`μ` and `ı`/`i` meet, as under APFS's
/// case folding and NTFS's upcase table; `ẞ` lowercases to `ß`, which the round trip takes on to
/// `ss`), and NFD-normalized again (case mapping can produce composed forms), so the fold is a
/// fixed point. NTFS doesn't normalize and HFS+ uses an older decomposition, and `ß` meeting
/// `ss` over-matches too: the fold errs towards finding more collisions, which only refuses
/// more. Anything else is lowercased as ASCII.
pub(crate) fn fold_name(name: &[u8]) -> Vec<u8> {
    // Most names are ASCII, where the Unicode fold is plain ASCII lowercasing.
    if name.is_ascii() {
        return name.to_ascii_lowercase();
    }
    match std::str::from_utf8(name) {
        Ok(text) => fold_text(text),
        Err(_) => name.to_ascii_lowercase(),
    }
}

/// [`fold_name`]'s Unicode path.
fn fold_text(text: &str) -> Vec<u8> {
    use unicode_normalization::UnicodeNormalization;
    text.nfd()
        .filter(|c| !hfs_ignorable(*c))
        .flat_map(char::to_lowercase)
        .flat_map(char::to_uppercase)
        .flat_map(char::to_lowercase)
        .nfd()
        .collect::<String>()
        .into_bytes()
}

/// Whether case collisions matter for `repo`: `core.ignorecase`, or always on macOS and Windows
/// (decision D1), where a repository copied from elsewhere can carry a wrong setting.
///
/// A config that can't be read (or an invalid `core.ignorecase`) is an error, not "case doesn't
/// fold", which on Linux would skip the case-collision checks (#225, #369). On macOS and
/// Windows the setting isn't read; [`core_ignorecase`] reads it strictly everywhere.
pub(crate) fn try_folds_case(repo: &git2::Repository) -> Result<bool, CaseConfigUnreadable> {
    Ok(cfg!(any(target_os = "macos", windows)) || core_ignorecase(repo)?)
}

/// `core.ignorecase`, false when not set; an unreadable config or an invalid value is an error.
pub(crate) fn core_ignorecase(repo: &git2::Repository) -> Result<bool, CaseConfigUnreadable> {
    let unreadable = |error: git2::Error| CaseConfigUnreadable(error.message().to_string());
    match repo
        .config()
        .map_err(unreadable)?
        .get_bool("core.ignorecase")
    {
        Ok(value) => Ok(value),
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(false),
        Err(error) => Err(unreadable(error)),
    }
}

/// The repository's `core.ignorecase` can't be read (the config is unreadable, or the value
/// isn't a boolean), so whether names that differ only in case collide is unknown (#225). The
/// operation stops instead of skipping its case-collision check.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "Can't read core.ignorecase from the Git config: {0}. Set it to true or false (or remove \
     it), then try again."
)]
pub struct CaseConfigUnreadable(pub String);

/// Whether the file system holding `dir` (an existing folder) folds case (#162): creates a
/// hidden temp file there and looks it up with its ASCII case flipped. The file is removed
/// again. A worktree shares its main repository's config, whose `core.ignorecase` was probed
/// where the main repository lives; a worktree on a USB stick or in a casefold folder needs its
/// own answer.
pub(crate) fn dir_folds_case(dir: &Path) -> io::Result<bool> {
    let probe = tempfile::Builder::new()
        .prefix(".browsitory-case-probe-")
        .tempfile_in(dir)?;
    let name = probe
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("the case probe has no UTF-8 name"))?;
    let flipped: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect();
    Ok(std::fs::symlink_metadata(dir.join(flipped)).is_ok())
}

/// The nearest ancestor of `path` (itself included) that is an existing folder; a relative
/// path's ancestors end at `.`. Folders created below it are on the same file system (and, in an
/// ext4 casefold folder, inherit its flag).
pub(crate) fn nearest_existing(path: &Path) -> Option<&Path> {
    path.ancestors()
        .map(|ancestor| {
            if ancestor.as_os_str().is_empty() {
                Path::new(".")
            } else {
                ancestor
            }
        })
        .find(|ancestor| ancestor.is_dir())
}

/// When a fresh checkout (clone, submodule) checks for case collisions. Tests can't preset a
/// fresh clone's config, so they force the check with `Always`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CasePolicy {
    /// [`try_folds_case`] of the repository being checked out.
    FromRepo,
    #[cfg_attr(not(test), allow(dead_code))]
    Always,
}

impl CasePolicy {
    pub(crate) fn applies(self, repo: &git2::Repository) -> Result<bool, CaseConfigUnreadable> {
        Ok(self == CasePolicy::Always || try_folds_case(repo)?)
    }
}

/// The first link or gitlink entry whose folded path equals the folded path of a folder on the
/// way to another entry (a strict prefix of it), in either order. A gitlink is a folder too (the
/// submodule's checkout), so a link and a gitlink of the same folded name collide as well.
#[cfg(test)]
fn first_collision<P: AsRef<[u8]>>(
    entries: impl IntoIterator<Item = (P, u32)>,
) -> Option<CaseCollision> {
    first_collision_across(entries.into_iter().map(|(path, mode)| (path, mode, 0)))
}

/// [`first_collision`] over entries from several sources (the third field). Within one source
/// an exactly equal link and folder collide too (only a crafted tree holds both); across
/// sources they are one path changing kind (a link replaced by a folder), so only names that
/// differ in spelling collide there.
#[cfg(test)]
fn first_collision_across<P: AsRef<[u8]>>(
    entries: impl IntoIterator<Item = (P, u32, usize)>,
) -> Option<CaseCollision> {
    collisions_across(entries, false).into_iter().next()
}

/// Every collision among `entries` (or only the first, unless `all`), each reported once.
fn collisions_across<P: AsRef<[u8]>>(
    entries: impl IntoIterator<Item = (P, u32, usize)>,
    all: bool,
) -> Vec<CaseCollision> {
    let mut out: Vec<CaseCollision> = Vec::new();
    macro_rules! report {
        ($collision:expr) => {{
            let collision = $collision;
            if !out.contains(&collision) {
                out.push(collision);
            }
            if !all {
                return out;
            }
        }};
    }
    let found = |link: &[u8], folder: &[u8]| CaseCollision {
        link: String::from_utf8_lossy(link).into_owned(),
        folder: String::from_utf8_lossy(folder).into_owned(),
    };
    // Folded path -> the path as git spells it, its source (and for links, whether a gitlink).
    let mut links: HashMap<Vec<u8>, (Vec<u8>, bool, usize)> = HashMap::new();
    let mut folders: HashMap<Vec<u8>, Vec<(Vec<u8>, usize)>> = HashMap::new();
    let mut seen_folders: std::collections::HashSet<(Vec<u8>, usize)> =
        std::collections::HashSet::new();
    // Same spelling from another source is the same path, not a collision.
    let clash = |a: &[u8], a_src: usize, b: &[u8], b_src: usize| a_src == b_src || a != b;
    for (path, mode, src) in entries {
        let path = path.as_ref();
        if mode == MODE_LINK || mode == MODE_GITLINK {
            let gitlink = mode == MODE_GITLINK;
            let key = fold_name(path);
            if let Some((folder, _)) = folders
                .get(&key)
                .and_then(|all| all.iter().find(|(f, f_src)| clash(path, src, f, *f_src)))
            {
                report!(found(path, folder));
            }
            match links.get(&key) {
                // A link next to a submodule folder of the same name.
                Some((other, other_gitlink, other_src))
                    if *other_gitlink != gitlink && clash(path, src, other, *other_src) =>
                {
                    report!(if gitlink {
                        found(other, path)
                    } else {
                        found(path, other)
                    });
                }
                Some(_) => {}
                None => {
                    links.insert(key, (path.to_vec(), gitlink, src));
                }
            }
        }
        for (at, byte) in path.iter().enumerate() {
            if *byte != b'/' {
                continue;
            }
            let prefix = &path[..at];
            // Index entries are sorted, so most prefixes repeat; each is folded once per source.
            if !seen_folders.insert((prefix.to_vec(), src)) {
                continue;
            }
            let key = fold_name(prefix);
            if let Some((link, _, link_src)) = links.get(&key) {
                if clash(link, *link_src, prefix, src) {
                    report!(found(link, prefix));
                }
            }
            folders.entry(key).or_default().push((prefix.to_vec(), src));
        }
    }
    out
}

/// The first case collision among `source`'s entries, optionally limited to the literal `paths`
/// (matched ignoring case, as libgit2 matches checkout paths under `core.ignorecase`), or `None`.
/// Pure: never touches the work tree.
pub(crate) fn find_case_collision<P: AsRef<[u8]>>(
    source: &GuardSource,
    paths: Option<&[P]>,
) -> Result<Option<CaseCollision>, git2::Error> {
    find_case_collision_in(std::slice::from_ref(source), paths)
}

/// Every case collision among `source`'s entries (see [`find_case_collision`]).
fn find_case_collisions<P: AsRef<[u8]>>(
    source: &GuardSource,
    paths: Option<&[P]>,
) -> Result<Vec<CaseCollision>, git2::Error> {
    let owned;
    let index: &git2::Index = match source {
        GuardSource::Index(index) => index,
        GuardSource::Tree(tree) => {
            let mut index = git2::Index::new()?;
            index.read_tree(tree)?;
            owned = index;
            &owned
        }
    };
    Ok(collisions_across(
        index
            .iter()
            .filter(|entry| paths.is_none_or(|paths| selected(&entry.path, paths, true)))
            .map(|entry| (entry.path, entry.mode, 0)),
        true,
    ))
}

/// [`find_case_collision`] over the union of several sources an operation writes together (a
/// stash's work-tree, index and untracked trees), so a link in one and a folder in another
/// collide too.
pub(crate) fn find_case_collision_in<P: AsRef<[u8]>>(
    sources: &[GuardSource],
    paths: Option<&[P]>,
) -> Result<Option<CaseCollision>, git2::Error> {
    Ok(collisions_in(sources, paths, false)?.into_iter().next())
}

/// Every case collision across `sources` (as [`find_case_collision_in`], but all of them, in
/// entry order).
fn find_case_collisions_in<P: AsRef<[u8]>>(
    sources: &[GuardSource],
    paths: Option<&[P]>,
) -> Result<Vec<CaseCollision>, git2::Error> {
    collisions_in(sources, paths, true)
}

fn collisions_in<P: AsRef<[u8]>>(
    sources: &[GuardSource],
    paths: Option<&[P]>,
    all: bool,
) -> Result<Vec<CaseCollision>, git2::Error> {
    let mut entries = Vec::new();
    for (i, source) in sources.iter().enumerate() {
        let owned;
        let index: &git2::Index = match source {
            GuardSource::Index(index) => index,
            GuardSource::Tree(tree) => {
                let mut index = git2::Index::new()?;
                index.read_tree(tree)?;
                owned = index;
                &owned
            }
        };
        entries.extend(
            index
                .iter()
                .filter(|entry| paths.is_none_or(|paths| selected(&entry.path, paths, true)))
                .map(|entry| (entry.path, entry.mode, i)),
        );
    }
    Ok(collisions_across(entries, all))
}

/// A collision the user already has, with its link entry (blob id and mode) as known.
struct KnownCollision {
    collision: CaseCollision,
    link: (git2::Oid, u32),
}

/// The collisions an operation may write again because the user already has them: those of
/// HEAD's tree, plus those of `also_known` (a tree the operation restores, such as an abort's
/// original tree, #161), each with its link entry. One is exempt only with the exact
/// `(link, folder)` spelling *and* the same link entry ([`brought_in`]); the on-disk link scan
/// that runs first stays the guard against writing through a link (#167).
fn known_collisions<P: AsRef<[u8]>>(
    repo: &git2::Repository,
    paths: Option<&[P]>,
    also_known: Option<&git2::Tree>,
) -> Result<Vec<KnownCollision>, git2::Error> {
    let head = repo.head().and_then(|head| head.peel_to_tree()).ok();
    let mut known = Vec::new();
    for tree in head.iter().chain(also_known) {
        for collision in find_case_collisions(&GuardSource::Tree(tree), paths)? {
            if let Some(link) = entry_in(&GuardSource::Tree(tree), &collision.link) {
                known.push(KnownCollision { collision, link });
            }
        }
    }
    Ok(known)
}

/// `path`'s entry (blob id, mode) in `source`, if it has one.
fn entry_in(source: &GuardSource, path: &str) -> Option<(git2::Oid, u32)> {
    match source {
        GuardSource::Tree(tree) => tree
            .get_path(Path::new(path))
            .ok()
            .map(|entry| (entry.id(), entry.filemode() as u32)),
        GuardSource::Index(index) => index
            .get_path(Path::new(path), 0)
            .map(|entry| (entry.id, entry.mode)),
    }
}

/// The first of `collisions` (from `sources`) that isn't known: not the same spelling, or a
/// link that some source holds with another blob or mode than the known one (a retargeted link
/// is a new link, D9-B).
fn brought_in(
    collisions: Vec<CaseCollision>,
    sources: &[GuardSource],
    known: &[KnownCollision],
) -> Option<CaseCollision> {
    collisions.into_iter().find(|collision| {
        let links: Vec<_> = sources
            .iter()
            .filter_map(|source| entry_in(source, &collision.link))
            .collect();
        !known.iter().any(|known| {
            known.collision == *collision
                && !links.is_empty()
                && links.iter().all(|link| *link == known.link)
        })
    })
}

/// `rel` as git spells it: components joined with `/` on every platform.
fn git_path(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// True when HEAD's tree tracks `rel` as a symlink. A checkout replaces such a link itself (it
/// unlinks it, never following it), so it is the user's committed state, not a hazard.
fn tracked_as_link(head_tree: Option<&git2::Tree>, rel: &Path) -> bool {
    head_tree
        .and_then(|tree| tree.get_path(rel).ok())
        .is_some_and(|entry| entry.filemode() as u32 == MODE_LINK)
}

/// Refuses `source` (optionally limited to `paths`) when it holds a case collision and case
/// matters for `repo` ([`try_folds_case`]).
pub(crate) fn refuse_case_collision<E, P>(
    repo: &git2::Repository,
    source: &GuardSource,
    paths: Option<&[P]>,
) -> Result<(), E>
where
    E: From<git2::Error> + From<CaseCollision> + From<CaseConfigUnreadable>,
    P: AsRef<[u8]>,
{
    refuse_case_collision_if(try_folds_case(repo)?, source, paths)
}

/// [`refuse_case_collision`] with the caller's answer to whether case matters (`applies`).
pub(crate) fn refuse_case_collision_if<E, P>(
    applies: bool,
    source: &GuardSource,
    paths: Option<&[P]>,
) -> Result<(), E>
where
    E: From<git2::Error> + From<CaseCollision>,
    P: AsRef<[u8]>,
{
    if !applies {
        return Ok(());
    }
    match find_case_collision(source, paths)? {
        Some(collision) => Err(collision.into()),
        None => Ok(()),
    }
}

/// Clears every hazard of a forced checkout in `repo`'s worktree, optionally limited to the
/// literal `paths`: those of the checkout's `target` and those of the current HEAD tree and
/// index. The latter matter because a forced checkout also *deletes* what the baseline tracks
/// and the target lacks (`d/f` through a symlinked `d` would delete a file outside the
/// repository). A link HEAD itself tracks is left to libgit2, which replaces it without following
/// it (removing it first breaks the checkout). Call immediately before the checkout, after
/// anything that must record the links (a stash).
pub(crate) fn prepare_forced_checkout<E, P>(
    repo: &git2::Repository,
    target: Option<&GuardSource>,
    paths: Option<&[P]>,
) -> Result<(), E>
where
    E: From<git2::Error>
        + From<io::Error>
        + From<CaseCollision>
        + From<CasePair>
        + From<CaseConfigUnreadable>,
    P: AsRef<[u8]>,
{
    prepare_forced_checkout_known(repo, target, paths, None)
}

/// [`prepare_forced_checkout`] that also lets the target bring back the collisions of
/// `also_known`: a tree the user already had, which an abort restores (#161).
pub(crate) fn prepare_forced_checkout_known<E, P>(
    repo: &git2::Repository,
    target: Option<&GuardSource>,
    paths: Option<&[P]>,
    also_known: Option<&git2::Tree>,
) -> Result<(), E>
where
    E: From<git2::Error>
        + From<io::Error>
        + From<CaseCollision>
        + From<CasePair>
        + From<CaseConfigUnreadable>,
    P: AsRef<[u8]>,
{
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    // Read strictly, before anything is written, also on macOS and Windows and for a cleanup
    // (#225). libgit2's checkout counts an invalid value as false but its attribute and ignore
    // parsing refuses it, so going on could remove the links below and then stop half-way.
    let ignore_case = core_ignorecase(repo)?;
    // libgit2's ignore-case index keeps one of a case pair the checkout includes (#403).
    refuse_case_pair::<E, P>(repo, target, paths)?;
    let folds = cfg!(any(target_os = "macos", windows)) || ignore_case;
    // A collision the target brings in is refused; one HEAD already holds (aborting a merge,
    // `reset --hard` to HEAD, discarding) isn't, so such a repository can still be cleaned up.
    // Every collision of the target must already be known, not just the first one found.
    if let Some(target) = target.filter(|_| folds) {
        let brought = find_case_collisions(target, paths)?;
        if !brought.is_empty() {
            let known = known_collisions(repo, paths, also_known)?;
            if let Some(collision) = brought_in(brought, std::slice::from_ref(target), &known) {
                return Err(collision.into());
            }
        }
    }
    // libgit2 matches checkout paths case-insensitively under `core.ignorecase`; so does the
    // filter here, or `TC` would scan nothing for a tracked `tc`.
    let head_tree = repo.head().and_then(|head| head.peel_to_tree()).ok();
    let mut scanner = Scanner::new(workdir, true);
    let mut hazards = BTreeSet::new();
    if let Some(target) = target {
        scanner.scan(target, paths, ignore_case, &mut hazards)?;
    }
    if let Some(head_tree) = &head_tree {
        scanner.scan(
            &GuardSource::Tree(head_tree),
            paths,
            ignore_case,
            &mut hazards,
        )?;
    }
    let index = repo.index()?;
    scanner.scan(
        &GuardSource::Index(&index),
        paths,
        ignore_case,
        &mut hazards,
    )?;
    hazards.retain(|hazard| !tracked_as_link(head_tree.as_ref(), hazard.path()));
    clear_hazards(workdir, &hazards.into_iter().collect::<Vec<_>>())?;
    Ok(())
}

/// What a safe operation is about to write, for [`refuse_linked_folders`].
pub(crate) enum Incoming<'a> {
    /// A tree checkout (switch, checkout, fast-forward, rebase start, rewind). A folder link HEAD
    /// tracks is skipped: libgit2 replaces it with the folder (a typechange) without following
    /// it, and refuses when the user changed it.
    Checkout(&'a git2::Tree<'a>),
    /// The result of a merge-like operation (merge, cherry-pick, revert, rebase step, stash
    /// apply). Nothing is skipped: libgit2 merges a file below a folder HEAD tracks as a link
    /// without a conflict and writes it through the link.
    Merge(GuardSource<'a>),
    /// A merge result (merge, cherry-pick, revert, rebase step) whose file/folder collisions the
    /// caller writes as git's conflict (`file_folder::check_out_as_conflicts`, #155). As
    /// `Merge`, except that a folder link HEAD tracks and the result keeps unchanged is
    /// skipped: entries below it are a file/folder collision, which the conflict checkout
    /// writes as git does (the link moves to `<link>~HEAD`, the folder's files are written in
    /// a real folder), never through the link.
    MergeAsConflict(&'a git2::Index),
}

/// What [`refuse_linked_folders_known`] lets a source's folder links pass.
enum Skip<'a> {
    Nothing,
    /// Links HEAD tracks (`Incoming::Checkout`).
    HeadLinks,
    /// Links HEAD tracks that this merge result keeps (`Incoming::MergeAsConflict`).
    KeptHeadLinks(&'a git2::Index),
}

/// `result` holds HEAD's link `rel` unchanged (the same id and mode, merged at stage 0 or as the
/// "ours" side of a conflict at stage 2), and the link on disk is still HEAD's: it points where
/// HEAD's link does. A link the user retargeted is a local change that a checkout allowing
/// conflicts would leave in place and write through, so it is never skipped.
fn keeps_head_link(
    repo: &git2::Repository,
    workdir: &Path,
    head_tree: Option<&git2::Tree>,
    result: &git2::Index,
    rel: &Path,
) -> bool {
    let Some(head_entry) = head_tree.and_then(|tree| tree.get_path(rel).ok()) else {
        return false;
    };
    let kept = head_entry.filemode() as u32 == MODE_LINK
        && [0, 2].into_iter().any(|stage| {
            result
                .get_path(rel, stage)
                .is_some_and(|entry| entry.mode == MODE_LINK && entry.id == head_entry.id())
        });
    kept && std::fs::read_link(workdir.join(rel)).is_ok_and(|target| {
        repo.find_blob(head_entry.id())
            .is_ok_and(|blob| path_from_bytes(blob.content()) == target)
    })
}

/// Refuses while a symlink stands where HEAD, the index or what the operation brings in has a
/// folder. libgit2's *safe* checkouts (switch, checkout, merge, cherry-pick, revert, rebase,
/// stash apply, pull) refuse to modify a file below such a link but still *create* new ones
/// through it, truncating whatever the link's target folder holds under that name; that holds
/// for a link nothing tracks too, so `incoming` (everything the operation writes) is scanned as
/// well. Call it right before the operation.
pub(crate) fn refuse_linked_folders<E>(
    repo: &git2::Repository,
    incoming: &[Incoming],
) -> Result<(), E>
where
    E: From<git2::Error> + From<LinkedFolder> + From<CaseCollision> + From<CaseConfigUnreadable>,
{
    refuse_linked_folders_known(repo, incoming, None)
}

/// [`refuse_linked_folders`] that also lets `incoming` bring back the collisions of
/// `also_known`: a tree the user already had, which an abort restores (#161).
pub(crate) fn refuse_linked_folders_known<E>(
    repo: &git2::Repository,
    incoming: &[Incoming],
    also_known: Option<&git2::Tree>,
) -> Result<(), E>
where
    E: From<git2::Error> + From<LinkedFolder> + From<CaseCollision> + From<CaseConfigUnreadable>,
{
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    const ALL: Option<&[&str]> = None;
    let head_tree = repo.head().and_then(|head| head.peel_to_tree()).ok();
    let index = repo.index()?;
    let mut sources = Vec::new();
    if let Some(head_tree) = &head_tree {
        sources.push((GuardSource::Tree(head_tree), Skip::Nothing));
    }
    sources.push((GuardSource::Index(&index), Skip::Nothing));
    for source in incoming {
        sources.push(match source {
            Incoming::Checkout(tree) => (GuardSource::Tree(tree), Skip::HeadLinks),
            Incoming::Merge(GuardSource::Tree(tree)) => (GuardSource::Tree(tree), Skip::Nothing),
            Incoming::Merge(GuardSource::Index(index)) => {
                (GuardSource::Index(index), Skip::Nothing)
            }
            Incoming::MergeAsConflict(index) => {
                (GuardSource::Index(index), Skip::KeptHeadLinks(index))
            }
        });
    }
    // Only folder links matter here, so files are never examined.
    let mut scanner = Scanner::new(workdir, false);
    let mut found = None;
    for (source, skip) in &sources {
        let mut hazards = BTreeSet::new();
        scanner.scan(source, ALL, false, &mut hazards)?;
        let skipped = |dir: &Path| match skip {
            Skip::Nothing => false,
            Skip::HeadLinks => tracked_as_link(head_tree.as_ref(), dir),
            Skip::KeptHeadLinks(result) => {
                keeps_head_link(repo, workdir, head_tree.as_ref(), result, dir)
            }
        };
        found = hazards.into_iter().find_map(|hazard| match hazard {
            Hazard::Ancestor(dir) if !skipped(&dir) => Some(dir),
            _ => None,
        });
        if found.is_some() {
            break;
        }
    }
    if let Some(dir) = found {
        return Err(LinkedFolder(git_path(&dir)).into());
    }
    // Then case collisions (#156), after the more specific link on disk. Only what the operation
    // brings in: a collision HEAD (or `also_known`) already holds with the identical spelling
    // is exempt (#167), so a HEAD that holds one doesn't lock the repository; any other one is
    // refused. The link scan above stays the guard for the exempt ones: on a case-insensitive
    // file system the folder's name resolves to the on-disk link, which it refuses.
    if try_folds_case(repo)? {
        let sources: Vec<GuardSource> = incoming
            .iter()
            .map(|source| match source {
                Incoming::Checkout(tree) => GuardSource::Tree(tree),
                Incoming::Merge(GuardSource::Tree(tree)) => GuardSource::Tree(tree),
                Incoming::Merge(GuardSource::Index(index)) => GuardSource::Index(index),
                Incoming::MergeAsConflict(index) => GuardSource::Index(index),
            })
            .collect();
        // In a merge result written as git's conflict, a link and a folder of the identical
        // spelling are the file/folder collision that checkout writes as git does (the link
        // moved to `<link>~HEAD`, #155), not a case collision; only differing spellings are.
        let incoming: Vec<CaseCollision> = find_case_collisions_in(&sources, ALL)?
            .into_iter()
            .filter(|collision| {
                collision.link != collision.folder
                    || !incoming.iter().any(|source| {
                        matches!(source, Incoming::MergeAsConflict(index)
                            if holds_link(index, &collision.link))
                    })
            })
            .collect();
        if !incoming.is_empty() {
            let known = known_collisions(repo, ALL, also_known)?;
            if let Some(collision) = brought_in(incoming, &sources, &known) {
                return Err(collision.into());
            }
        }
    }
    Ok(())
}

/// `index` has `path` as a symlink entry, at any stage.
fn holds_link(index: &git2::Index, path: &str) -> bool {
    index
        .iter()
        .any(|entry| entry.mode == MODE_LINK && entry.path == path.as_bytes())
}

/// True when a directory on the way to `path` (relative to `workdir`) is a symlink. Fails closed:
/// a path that is not plain relative (`..`, absolute, a Windows prefix) counts as one.
pub(crate) fn has_symlinked_ancestor(workdir: &Path, path: &str) -> bool {
    let rel = Path::new(path);
    if !plain_relative(rel) {
        return true;
    }
    let mut current = workdir.to_path_buf();
    let components: Vec<_> = rel.components().collect();
    for component in &components[..components.len() - 1] {
        current.push(component);
        if is_link(&current) {
            return true;
        }
    }
    false
}

/// The first proper ancestor of `path` (relative to `workdir`) that exists as a regular file: a
/// folder on the way to `path` was replaced by a file (#154). A link on the way, or a missing
/// one, ends the walk with `None`; links are the symlink checks' business.
pub(crate) fn file_in_place_of_folder(workdir: &Path, path: &str) -> io::Result<Option<String>> {
    let rel = Path::new(path);
    if !plain_relative(rel) {
        return Ok(None);
    }
    let components: Vec<_> = rel.components().collect();
    let mut prefix = PathBuf::new();
    for component in &components[..components.len().saturating_sub(1)] {
        prefix.push(component);
        match workdir.join(&prefix).symlink_metadata() {
            Ok(meta) if meta.file_type().is_file() => return Ok(Some(git_path(&prefix))),
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => return Ok(None),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

/// Refuses when `rel` itself or a folder on the way to it (below `workdir`) is a symlink, for a
/// whole folder that is about to be written, such as a submodule's checkout.
pub(crate) fn refuse_linked_path(workdir: &Path, rel: &Path) -> Result<(), LinkedFolder> {
    let mut current = PathBuf::new();
    for component in rel.components() {
        current.push(component);
        if is_link(&workdir.join(&current)) {
            return Err(LinkedFolder(git_path(&current)));
        }
    }
    Ok(())
}

/// Removes the file or link `rel` without following a link at the leaf or above it. Returns
/// `Ok(false)` and does nothing when an ancestor is a symlink, `rel` is a directory, or `rel` is
/// not a plain relative path. A missing `rel` counts as removed.
pub(crate) fn remove_beneath(workdir: &Path, rel: &str) -> io::Result<bool> {
    if !plain_relative(Path::new(rel)) {
        return Ok(false);
    }
    beneath::remove(workdir, Path::new(rel), &mut no_step)
}

/// The error source [`write_beneath`] reports for an existing file it may not write because it
/// is read-only (#209).
#[derive(Debug)]
pub(crate) struct ReadOnlyFile;

impl std::fmt::Display for ReadOnlyFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the file is read-only")
    }
}

impl std::error::Error for ReadOnlyFile {}

/// A `PermissionDenied` error wrapping [`ReadOnlyFile`].
pub(crate) fn read_only_error() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, ReadOnlyFile)
}

/// Whether `error` is [`read_only_error`]'s.
pub(crate) fn is_read_only(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|source| source.is::<ReadOnlyFile>())
}

/// The error source [`write_beneath`] reports on Windows for a file another program keeps open
/// so it can't be replaced (#239): opened without sharing delete, as some editors do.
#[derive(Debug)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct FileInUse {
    pub(crate) name: String,
}

impl std::fmt::Display for FileInUse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "'{}' is open in another program. Close it, then try again.",
            self.name
        )
    }
}

impl std::error::Error for FileInUse {}

/// Whether `error` is the [`FileInUse`] error of [`write_beneath`].
pub(crate) fn is_in_use(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|source| source.is::<FileInUse>())
}

/// Whether `rel` is an existing regular file that [`write_beneath`] would refuse as read-only
/// (#209), by the same rule, looked at through the walker without following a link and
/// without opening the file (Unix `fstatat`; Windows the attributes of a handle with no data
/// access, so an online-only OneDrive file isn't downloaded). For acts that write a file some
/// other way (a forced checkout replaces it). A link, a folder, a missing file or one the walk
/// can't reach counts as not read-only.
pub(crate) fn is_read_only_beneath(workdir: &Path, rel: &str) -> bool {
    plain_relative(Path::new(rel))
        && beneath::is_read_only(workdir, Path::new(rel), &mut no_step).unwrap_or(false)
}

/// For a libgit2 checkout of `paths` that failed with `error`: the one of `paths` that another
/// program keeps open so it can't be replaced (Windows: opened without sharing delete, as some
/// editors do), the file to name in a "Close it, then try again" message (#268).
///
/// libgit2 reports such a file only as an OS error, "could not remove '<full path>': " plus the
/// system's reason in the user's language. Only for an OS or file-system error, the path is
/// taken from the quoted full path (either slash direction; the localized reason is never
/// read), and that file alone is checked. When no path of the checkout is quoted, each one is
/// checked in turn. A check opens the file through the walker (never through a link) for
/// deleting, sharing everything, and closes it at once: a sharing violation means another
/// program holds it. Anything else, and every other platform (where an open file never stops a
/// replace), is `None`, so the caller keeps libgit2's error.
///
/// Known misses, which keep libgit2's raw error: a program holding the file shared for reading
/// and deleting but not writing (the check's delete open succeeds, libgit2's write fails), and
/// a running program or a loaded library (`.exe`, `.dll`), which Windows refuses with access
/// denied rather than a sharing violation.
pub(crate) fn held_open_after_checkout<'a>(
    workdir: &Path,
    error: &git2::Error,
    paths: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    if !matches!(
        error.class(),
        git2::ErrorClass::Os | git2::ErrorClass::Filesystem
    ) {
        return None;
    }
    let paths: Vec<&'a str> = paths.into_iter().collect();
    match quoted_in_message(workdir, error.message(), &paths) {
        Some(named) => held_open_among(workdir, [named]),
        None => held_open_among(workdir, paths),
    }
}

/// The one of `paths` whose full path below `workdir` libgit2's `message` quotes
/// (`'C:/repo/d/f.txt'`), compared with forward slashes.
fn quoted_in_message<'a>(workdir: &Path, message: &str, paths: &[&'a str]) -> Option<&'a str> {
    let message = message.replace('\\', "/");
    let base = workdir.to_string_lossy().replace('\\', "/");
    let base = base.trim_end_matches('/');
    paths
        .iter()
        .copied()
        .find(|rel| message.contains(&format!("'{base}/{}'", rel.replace('\\', "/"))))
}

/// Whether another program holds `rel` open so it can't be replaced or removed (Windows; see
/// [`held_open_after_checkout`] for the probe). Always false elsewhere.
pub(crate) fn is_held_open(workdir: &Path, rel: &str) -> bool {
    held_open_among(workdir, [rel]).is_some()
}

#[cfg(windows)]
fn held_open_among<'a>(
    workdir: &Path,
    paths: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    paths.into_iter().find(|rel| {
        plain_relative(Path::new(rel))
            && beneath::is_held_open(workdir, Path::new(rel), &mut no_step).unwrap_or(false)
    })
}

#[cfg(not(windows))]
fn held_open_among<'a>(
    _workdir: &Path,
    _paths: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    None
}

/// The operation whose libgit2 checkout of a whole tree stopped at a file another program holds
/// open ([`HeldOpen`], #303). It decides what the message says about the files already
/// written. Each of these finishes when tried again once the file is closed (an abort when
/// aborted again; a clone starts over, its folder removed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldDuring {
    /// Switching to a branch, or checking out a commit.
    Switch,
    /// A hard reset.
    Reset,
    /// A fast-forward merge.
    FastForward,
    /// A rebase abort putting the original branch back.
    RebaseAbort,
    /// A clone's checkout.
    Clone,
    /// Updating a submodule.
    SubmoduleUpdate,
    /// A cherry-pick, revert, rebase start, pull or stash apply (or pop) that put back what its
    /// checkout had already written (#321), or an abort that hadn't put anything back yet:
    /// nothing was left changed.
    RolledBack,
    /// Aborting a merge, cherry-pick or revert (#321): the files before the held one are back as
    /// they were before the operation, the rest is still there to abort.
    Abort,
}

impl HeldDuring {
    /// What follows "'f.txt' is open in another program." in the message.
    fn advice(self) -> &'static str {
        match self {
            HeldDuring::Switch => {
                "Some files were already switched. Close it, then try again to finish."
            }
            HeldDuring::Reset => {
                "Some files were already reset. Close it, then try again to finish."
            }
            HeldDuring::FastForward | HeldDuring::SubmoduleUpdate => {
                "Some files were already updated. Close it, then try again to finish."
            }
            HeldDuring::RebaseAbort | HeldDuring::Abort => {
                "Some files were already put back. Close it, then abort again to finish."
            }
            HeldDuring::Clone | HeldDuring::RolledBack => "Close it, then try again.",
        }
    }
}

/// Windows: a libgit2 checkout of a whole tree stopped at `path` (relative to the work tree),
/// which another program keeps open so it can't be replaced (#303). The files before it may
/// already be written; `during` says how to finish.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("'{path}' is open in another program. {}", during.advice())]
pub struct HeldOpen {
    pub path: String,
    pub during: HeldDuring,
}

/// A `map_err` for a libgit2 checkout of a whole tree in `workdir` (#303): the file another
/// program holds open, as [`HeldOpen`], else libgit2's error unchanged. With no path list, the
/// file is the one libgit2's message quotes below `workdir` (see [`held_open_after_checkout`]
/// for how it is checked, and its known misses). `None` for a bare repository.
pub(crate) fn held_open<E>(
    workdir: Option<&Path>,
    during: HeldDuring,
) -> impl FnOnce(git2::Error) -> E + '_
where
    E: From<git2::Error> + From<HeldOpen>,
{
    move |error| {
        let held = workdir.and_then(|workdir| {
            held_open_in(workdir, &error, |workdir, rel| {
                held_open_among(workdir, [rel]).is_some()
            })
        });
        match held {
            Some(path) => HeldOpen { path, during }.into(),
            None => error.into(),
        }
    }
}

/// [`held_open`]'s path: of the paths below `workdir` that `error` (an OS or file-system error)
/// may quote, the longest one `is_held` says another program holds.
fn held_open_in(
    workdir: &Path,
    error: &git2::Error,
    is_held: impl Fn(&Path, &str) -> bool,
) -> Option<String> {
    if !matches!(
        error.class(),
        git2::ErrorClass::Os | git2::ErrorClass::Filesystem
    ) {
        return None;
    }
    quoted_candidates(workdir, error.message())
        .into_iter()
        .find(|rel| is_held(workdir, rel))
}

/// The paths, relative to `workdir` and with forward slashes, that libgit2's `message` may
/// quote below `workdir`, longest first. libgit2 words these errors many ways (`could not
/// remove '<p>': <reason>`, `failed remove - '<p>' is locked`, `could not find '<p>' to
/// remove`, `could not open '<p>' for writing`, ...) and a file name may itself hold `'`, so
/// every `'` after an opening `'<workdir>/` ends a candidate; the caller's probe decides which,
/// if any, is the file. A candidate that isn't a plain path below `workdir` (`..`, `.`, empty
/// components, a `:`, which no Windows name holds) is dropped, so nothing outside is probed.
/// `workdir` is matched with and without a `\\?\` (`\\?\UNC\`) prefix, either way round.
fn quoted_candidates(workdir: &Path, message: &str) -> Vec<String> {
    let message = message.replace('\\', "/");
    let mut candidates = Vec::new();
    for base in base_spellings(workdir) {
        let opening = format!("'{base}/");
        for (at, _) in message.match_indices(&opening) {
            let rest = &message[at + opening.len()..];
            for (end, _) in rest.match_indices('\'') {
                let rel = &rest[..end];
                if plain_below(rel) {
                    candidates.push(rel.to_string());
                }
            }
        }
    }
    candidates.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    candidates.dedup();
    candidates
}

/// `workdir` with forward slashes and no trailing slash, as written and with its `//?/`
/// (`//?/UNC/`) long-path prefix removed or added.
fn base_spellings(workdir: &Path) -> Vec<String> {
    let base = workdir.to_string_lossy().replace('\\', "/");
    let base = base.trim_end_matches('/').to_string();
    let mut spellings = vec![base.clone()];
    if let Some(unc) = base.strip_prefix("//?/UNC/") {
        spellings.push(format!("//{unc}"));
    } else if let Some(local) = base.strip_prefix("//?/") {
        spellings.push(local.to_string());
    } else if let Some(unc) = base.strip_prefix("//") {
        spellings.push(format!("//?/UNC/{unc}"));
    } else {
        spellings.push(format!("//?/{base}"));
    }
    spellings
}

/// Whether `rel` (forward slashes) names a path below the work tree: no empty, `.` or `..`
/// component, and no `:` (a drive or a stream; never part of a Windows file name).
fn plain_below(rel: &str) -> bool {
    !rel.contains(':') && rel.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

/// Writes `bytes` to `rel` as a regular file. Returns `Ok(false)` and writes nothing when an
/// ancestor is a symlink or `rel` is not a plain relative path.
///
/// The file is replaced whole (#239): the bytes go to a new temporary file in the same held
/// folder (`.{name}.browsitory-{pid}-{n}.tmp`), which is flushed and then renamed over `rel`.
/// On any failure the old file is left as it was and the temporary file is removed. A symlink
/// at `rel` is replaced itself, never written through; a hardlink is replaced too, so the
/// file's other names keep the old content. An existing file keeps its mode (Unix) or its
/// settable attributes (Windows) and the spelling of its name on disk (a case-insensitive
/// volume); ACLs, extended attributes and alternate data streams are not carried over. A
/// folder, FIFO or device is an error, never opened for writing.
///
/// An existing read-only file is an error for which [`is_read_only`] is true, and is left as it
/// is (#209): on Windows the read-only attribute, on Unix a file the user owns without write
/// permission for its owner. A file that is not writable for another reason (an ACL, another
/// owner) keeps the system's error. On Windows a file another program holds open without
/// sharing delete can't be replaced: an error for which [`is_in_use`] is true, nothing changed.
pub(crate) fn write_beneath(workdir: &Path, rel: &str, bytes: &[u8]) -> io::Result<bool> {
    if !plain_relative(Path::new(rel)) {
        return Ok(false);
    }
    beneath::write(workdir, Path::new(rel), bytes, &mut no_step)
}

/// A work-tree file or link as [`read_beneath`] found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Leaf {
    File {
        bytes: Vec<u8>,
        executable: bool,
        /// Taken from the descriptor the bytes were read from, before reading them.
        stat: FileStat,
    },
    Link {
        /// The link's target (`/`-separated on Windows).
        target: Vec<u8>,
        /// The link's own stat data (`lstat`), taken before its target was read. Zeroed on
        /// Windows, where status then reads the link again on every refresh (#225).
        stat: FileStat,
    },
}

/// A file's stat data as `git add` records it in the index (git's `fill_stat_data`), from the
/// descriptor [`read_beneath`] read it through. `dev` is `st_dev`, as the git CLI records it, so
/// the entry matches what `git add` writes; libgit2's `git_index_entry__init_from_stat`
/// records `st_rdev` there instead (libgit2 1.9.7 `index.c`), and its status never compares
/// the device. git compares the device number only if it was compiled to use it and
/// `core.checkStat` isn't `minimal` (git's `core.checkStat` docs); then `st_dev` is the value
/// that matches. On Windows, as libgit2 fills it there: the creation time as ctime, and no
/// device, inode or owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct FileStat {
    pub ctime: (i64, u32),
    pub mtime: (i64, u32),
    pub dev: u64,
    pub ino: u64,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
}

#[cfg(windows)]
impl FileStat {
    pub(crate) fn of(meta: &std::fs::Metadata) -> Self {
        let since_epoch = |time: io::Result<std::time::SystemTime>| {
            time.ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or((0, 0), |d| (d.as_secs() as i64, d.subsec_nanos()))
        };
        Self {
            ctime: since_epoch(meta.created()),
            mtime: since_epoch(meta.modified()),
            size: meta.len(),
            ..Self::default()
        }
    }
}

/// What [`replace_beneath`] writes.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Content<'a> {
    File {
        bytes: &'a [u8],
        /// Windows files have no exec bit.
        #[cfg_attr(windows, allow(dead_code))]
        executable: bool,
    },
    /// A symbolic link to the target (written as a plain file holding it where links aren't
    /// created, i.e. Windows).
    Link(&'a [u8]),
}

/// Reads the file or link `rel` without following a link at any component, and without blocking
/// on a FIFO. `None` when it is missing, a folder, not a regular file or link, or beyond a link
/// (git sees no file there).
pub(crate) fn read_beneath(workdir: &Path, rel: &str) -> io::Result<Option<Leaf>> {
    if !plain_relative(Path::new(rel)) {
        return Ok(None);
    }
    beneath::read(workdir, Path::new(rel), &mut no_step)
}

/// What [`probe_beneath`] found at a path, judged without following a link at any component.
pub(crate) enum Probe {
    /// Missing, or a component on the way is a file.
    Missing,
    /// A component on the way is a link.
    BeyondLink,
    File,
    Link,
    /// A FIFO, socket or device (never on Windows).
    #[cfg_attr(windows, allow(dead_code))]
    Other,
    /// A folder, now held open: what is read through it is in this folder, whatever happens to
    /// its path meanwhile.
    Folder(HeldFolder),
}

/// A folder held open by [`probe_beneath`] (or [`HeldFolder::resolve`]), with every folder above
/// it up to the work tree root. Reads below it walk from its handle (as [`read_beneath`] walks
/// from the work tree root), and `..` goes to the held folder above, so a path swapped for a
/// link, a FIFO or another folder after the probe is never looked at (#213). Nothing outside
/// the work tree is ever opened through it.
pub(crate) struct HeldFolder {
    /// From the work tree root (`chain[0]`) down to this folder (the last), each held.
    chain: Vec<beneath::Folder>,
    /// The work tree root's path as the caller spelled it.
    root: PathBuf,
}

/// What [`HeldFolder::resolve`] found.
pub(crate) enum Resolved {
    Held(HeldFolder),
    /// A component is missing.
    NotFound,
    /// A component is a link, a file or anything else that isn't a folder.
    NotAFolder,
    /// The path leads out of the work tree: an absolute path that isn't below the work tree root
    /// as it was opened, or `..` above it. Nothing was opened.
    Outside,
    /// More than [`RESOLVE_MAX_COMPONENTS`] names. Nothing was opened.
    TooDeep,
}

/// The most names [`HeldFolder::resolve`] takes from a path, counting only normal components
/// (not `/`, a drive prefix, `.` or `..`); a folder may be held for each.
const RESOLVE_MAX_COMPONENTS: usize = 64;

/// A file [`HeldFolder::read`] found larger than the caller's limit.
#[derive(Debug)]
pub(crate) struct TooLarge;

impl std::fmt::Display for TooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the file is larger than expected")
    }
}

impl std::error::Error for TooLarge {}

/// Whether `error` is [`TooLarge`]'s.
pub(crate) fn is_too_large(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|source| source.is::<TooLarge>())
}

/// Reads `file` to its end, at most `limit` bytes; more is [`TooLarge`].
#[cfg(any(unix, windows))]
fn read_capped(file: impl io::Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    io::Read::read_to_end(
        &mut io::Read::take(file, limit.saturating_add(1)),
        &mut bytes,
    )?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::new(io::ErrorKind::InvalidData, TooLarge));
    }
    Ok(bytes)
}

impl HeldFolder {
    /// `chain` from the work tree root `workdir` (`chain[0]`) down to the probed folder.
    fn in_work_tree(chain: Vec<beneath::Folder>, workdir: &Path) -> Self {
        Self {
            chain,
            root: workdir.to_path_buf(),
        }
    }

    fn last(&self) -> &beneath::Folder {
        &self.chain[self.chain.len() - 1]
    }

    /// What is at `rel` below this folder, judged like [`probe_beneath`] but without holding a
    /// folder found there.
    pub(crate) fn kind(&self, rel: &str) -> io::Result<Kind> {
        if !plain_relative(Path::new(rel)) {
            return Ok(Kind::Missing);
        }
        self.last().kind(Path::new(rel), &mut no_step)
    }

    /// [`read_beneath`] for `rel` below this folder, at most `limit` bytes: a larger file is an
    /// error for which [`is_too_large`] is true.
    pub(crate) fn read(&self, rel: &str, limit: u64) -> io::Result<Option<Leaf>> {
        if !plain_relative(Path::new(rel)) {
            return Ok(None);
        }
        self.last().read(Path::new(rel), limit, &mut no_step)
    }

    /// The folder's own stat data, as git's index records it.
    pub(crate) fn stat(&self) -> io::Result<FileStat> {
        self.last().stat()
    }

    /// The folder `target` (a `gitdir:` or `commondir`) names, relative to this one or absolute,
    /// held, inside the work tree only. Resolved one component at a time from held handles: a
    /// name is opened in the folder held before it (never a link), `..` is the held folder above
    /// (physical, never applied to the path's text). An absolute path counts only when it is
    /// below the work tree root exactly as that was opened, compared by component; the rest is
    /// then resolved the same way from the held root. Anything that leads out of the work tree
    /// (another absolute path, `..` above the root) is [`Resolved::Outside`], and more than
    /// [`RESOLVE_MAX_COMPONENTS`] names is [`Resolved::TooDeep`]; in both cases nothing is
    /// opened or looked up.
    pub(crate) fn resolve(&self, target: &Path) -> io::Result<Resolved> {
        let names = target
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count();
        if names > RESOLVE_MAX_COMPONENTS {
            return Ok(Resolved::TooDeep);
        }
        let (mut chain, rest) = if target.is_absolute() {
            let Ok(inside) = target.strip_prefix(&self.root) else {
                return Ok(Resolved::Outside);
            };
            (vec![self.chain[0].try_clone()?], inside)
        } else {
            let chain = self
                .chain
                .iter()
                .map(beneath::Folder::try_clone)
                .collect::<io::Result<Vec<_>>>()?;
            (chain, target)
        };
        for component in rest.components() {
            match component {
                Component::Normal(name) => {
                    let last = &chain[chain.len() - 1];
                    match last.child(name)? {
                        Some(folder) => chain.push(folder),
                        None => {
                            return Ok(match last.kind(Path::new(name), &mut no_step)? {
                                Kind::Missing => Resolved::NotFound,
                                _ => Resolved::NotAFolder,
                            })
                        }
                    }
                }
                Component::CurDir => {}
                Component::ParentDir if chain.len() > 1 => {
                    chain.pop();
                }
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Ok(Resolved::Outside)
                }
            }
        }
        Ok(Resolved::Held(HeldFolder {
            chain,
            root: self.root.clone(),
        }))
    }
}

/// What [`HeldFolder::kind`] found at a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Missing, or a component on the way is a file.
    Missing,
    /// A component on the way is a link.
    BeyondLink,
    File,
    Link,
    Folder,
    /// A FIFO, socket or device (never on Windows).
    #[cfg_attr(windows, allow(dead_code))]
    Other,
}

/// Says what is at `rel` and, for a folder, holds it open. Never opens a file, so a FIFO can't
/// block it.
pub(crate) fn probe_beneath(workdir: &Path, rel: &str) -> io::Result<Probe> {
    if !plain_relative(Path::new(rel)) {
        return Ok(Probe::Missing);
    }
    beneath::probe(workdir, Path::new(rel), &mut no_step)
}

/// Removes the file, link or empty folder `rel`, then the folders that emptied, up to (never
/// including) the work tree. Does nothing beyond a link: a path below one doesn't exist for git.
/// `rel` is a `Path`, so on Unix a name that isn't UTF-8 works too (#262).
pub(crate) fn remove_and_prune_beneath(workdir: &Path, rel: &Path) -> io::Result<()> {
    remove_and_prune_beneath_keeping(workdir, rel, 0)
}

/// [`remove_and_prune_beneath`], but the first `keep` folders of `rel` are never pruned (a
/// folder that was there before, empty or not, #329).
pub(crate) fn remove_and_prune_beneath_keeping(
    workdir: &Path,
    rel: &Path,
    keep: usize,
) -> io::Result<()> {
    if !plain_relative(rel) {
        return Ok(());
    }
    beneath::remove_and_prune(workdir, rel, keep, &mut no_step)
}

/// Replaces whatever is at `rel` (a file, link or empty folder) with `content`. A link that
/// replaced a folder on the way is removed first and the folder recreated (never written
/// through); missing folders are created.
///
/// A file is replaced whole (#251), as by [`write_beneath`]: the new file (or, on Unix, link)
/// is made under a temporary name in the same held folder (`.{name}.browsitory-{pid}-{n}.tmp`)
/// and renamed over `rel` only once it is complete, so a failure while making it leaves the old
/// file or link as it was and removes the temporary entry. Unlike [`write_beneath`], nothing of
/// the old file is kept: the new file's mode comes from `content` (Unix: the umask's, with the
/// exec bits set or cleared), Windows attributes are reset and a read-only file is replaced too.
/// An existing file keeps the spelling of its name on disk. A folder that isn't empty is an
/// error. An empty folder (Unix, Windows) or a link (Windows), which a rename can't replace
/// there, is deleted: on Unix the empty folder before the new file is made, on Windows either
/// one only once the new file is complete, just before the rename (a failure of that rename
/// then leaves neither). On Windows a file another program holds open without sharing delete
/// is an error for which [`is_in_use`] is true, after a short retry, nothing changed; a
/// read-only one too. Atomic per file only: a restore of several files that fails part-way
/// keeps the files already restored; restoring again finishes it (`undo::restore` counts a
/// path, or the index, already at its target as unchanged).
pub(crate) fn replace_beneath(workdir: &Path, rel: &str, content: Content<'_>) -> io::Result<()> {
    if !plain_relative(Path::new(rel)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be relative to the repository",
        ));
    }
    beneath::replace(workdir, Path::new(rel), content, &mut no_step)
}

/// How many names the regular file at `full` has; 1 for anything else and for a missing path
/// (see [`separate_hard_link`]).
pub(crate) fn link_count(full: &Path) -> io::Result<u64> {
    beneath::link_count(full)
}

/// Gives the regular file `rel` a name of its own when it has other hard links (#420): it is
/// read and written back whole through [`write_beneath`]'s path (a new file renamed over the
/// name), so the content and mode (Windows: attributes) stay, and the other names keep the
/// file as it was. libgit2 writes a file it checks out in place (`O_TRUNC`), which would change
/// every name; git's checkout and `git apply` replace the name. `Ok(true)` when the file was
/// replaced; `Ok(false)` for a file with one name, a missing path, a link or folder, or a path
/// beyond a link (nothing is written through one: the walk refuses a folder swapped for a
/// symlink or junction, as for any write).
///
/// The link count comes from an `lstat` of the path (Windows: the handle of the name itself,
/// opened for its attributes only), as libgit2's own stat of the file before writing it; the
/// copy then goes through the walker. The file is read into memory, as libgit2 holds the blob
/// it writes there.
pub(crate) fn separate_hard_link(workdir: &Path, rel: &Path) -> io::Result<bool> {
    if !plain_relative(rel) || beneath::link_count(&workdir.join(rel))? <= 1 {
        return Ok(false);
    }
    let Some(Leaf::File { bytes, .. }) = beneath::read(workdir, rel, &mut no_step)? else {
        return Ok(false);
    };
    beneath::write(workdir, rel, &bytes, &mut no_step)
}

/// Opens the regular file `rel` below `workdir` for reading without following a symlink at any
/// component below `workdir`, and without blocking on a FIFO. `None` when any component is a
/// symlink, a component is not a directory, the path is missing, or the leaf is not a regular
/// file. It never falls back to an open by path.
///
/// Each step opens relative to the directory handle of the previous step (`openat` with
/// `O_NOFOLLOW`), so a directory swapped for a symlink at any depth, at any moment, cannot
/// redirect the read. `workdir` itself may be reached through a symlink (the user chose it).
///
/// Windows holds each folder on the way without `FILE_SHARE_DELETE` and opens every component
/// as itself, so a junction or symbolic link anywhere (to a network share included) is refused
/// without being contacted; an online-only OneDrive file is reopened so it loads (#150).
pub(crate) fn open_beneath(workdir: &Path, rel: &Path) -> io::Result<Option<std::fs::File>> {
    if !plain_relative(rel) {
        return Ok(None);
    }
    beneath::open_file(workdir, rel, &mut no_step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;

    /// A tree with `entries` (path, content), built without touching any work tree.
    fn files_tree<'r>(repo: &'r Repository, entries: &[&str]) -> git2::Tree<'r> {
        let mut index = git2::Index::new().unwrap();
        for path in entries {
            let blob = repo.blob(path.as_bytes()).unwrap();
            index
                .add(&git2::IndexEntry {
                    ctime: git2::IndexTime::new(0, 0),
                    mtime: git2::IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode: 0o100644,
                    uid: 0,
                    gid: 0,
                    file_size: 0,
                    id: blob,
                    flags: 0,
                    flags_extended: 0,
                    path: path.as_bytes().to_vec(),
                })
                .unwrap();
        }
        repo.find_tree(index.write_tree_to(repo).unwrap()).unwrap()
    }

    #[test]
    fn a_path_limited_pair_search_finds_what_the_whole_search_selects() {
        // #512: the targeted walk must find exactly the pairs the whole-tree search would
        // keep for those paths, through folders that differ in case too.
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let tree = files_tree(
            &repo,
            &[
                "A",
                "a",
                "D/f",
                "d/f",
                "X/y/B",
                "x/Y/b",
                "keep/c",
                "other.txt",
            ],
        );
        let source = GuardSource::Tree(&tree);
        let sources = [&source];
        let cases: [&[&str]; 9] = [
            &["a"],
            &["A"],
            &["d"],
            &["D/F"],
            &["x"],
            &["x/y/b/"],
            &["keep"],
            &["other.txt", "keep/c"],
            &["d/f/g"],
        ];
        for paths in cases {
            let whole: Vec<_> = case_pairs(&sources)
                .unwrap()
                .into_iter()
                .filter(|names| names.iter().any(|name| selected(name, paths, true)))
                .collect();
            assert_eq!(
                selected_case_pairs(&sources, paths).unwrap(),
                whole,
                "{paths:?}"
            );
        }
        let mut index = git2::Index::new().unwrap();
        index.read_tree(&tree).unwrap();
        let source = GuardSource::Index(&index);
        assert_eq!(selected_case_pairs(&[&source], &["D"]).unwrap().len(), 1);
        assert!(selected_case_pairs(&[&source], &["keep"])
            .unwrap()
            .is_empty());
    }

    /// #512: what the case-pair check costs a forced checkout under `core.ignorecase` on a
    /// repository of 100,000 files (1,000 folders of 100), measured against the whole guard.
    /// Run with `cargo test -p git-core --release --lib -- --ignored --nocapture case_pair_cost`.
    #[test]
    #[ignore = "measurement, not a check"]
    fn case_pair_cost() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.ignorecase", true)
            .unwrap();
        let blob = repo.blob(b"x\n").unwrap();
        let mut root = repo.treebuilder(None).unwrap();
        for folder in 0..1000 {
            let mut inner = repo.treebuilder(None).unwrap();
            for file in 0..100 {
                inner
                    .insert(format!("File{file:03}.txt"), blob, 0o100644)
                    .unwrap();
            }
            root.insert(format!("dir{folder:04}"), inner.write().unwrap(), 0o040000)
                .unwrap();
        }
        let tree = repo.find_tree(root.write().unwrap()).unwrap();
        let sig = git2::Signature::now("T", "t@example.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "c", &tree, &[])
            .unwrap();
        let mut index = repo.index().unwrap();
        index.read_tree(&tree).unwrap();
        index.write().unwrap();
        let target = GuardSource::Tree(&tree);
        let time = |label: &str, run: &dyn Fn()| {
            let started = std::time::Instant::now();
            for _ in 0..5 {
                run();
            }
            eprintln!("{label}: {:?} per run", started.elapsed() / 5);
        };
        time("refuse_case_pair (target, HEAD, index)", &|| {
            refuse_case_pair::<crate::rebase::RebaseError, &str>(&repo, Some(&target), ALL).unwrap()
        });
        time("refuse_case_pair, one path", &|| {
            refuse_case_pair::<crate::rebase::RebaseError, &str>(
                &repo,
                Some(&target),
                Some(&["dir0500/File050.txt"]),
            )
            .unwrap()
        });
        time("prepare_forced_checkout, one path", &|| {
            prepare_forced_checkout::<crate::rebase::RebaseError, &str>(
                &repo,
                Some(&target),
                Some(&["dir0500/File050.txt"]),
            )
            .unwrap()
        });
        time("prepare_forced_checkout, whole tree", &|| {
            prepare_forced_checkout::<crate::rebase::RebaseError, &str>(&repo, Some(&target), ALL)
                .unwrap()
        });
    }

    const ALL: Option<&[&str]> = None;
    use tempfile::TempDir;

    fn write(dir: &Path, rel: &str, contents: &str) {
        let full = dir.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, contents).unwrap();
    }

    fn commit_all(repo: &Repository) {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, "c", &tree, &parents)
            .unwrap();
    }

    /// `tc` = "plain\n", `d/f` = "inner\n", `a.txt` = "base\n", committed on `refs/heads/main`.
    fn repo() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "T").unwrap();
            config.set_str("user.email", "t@example.com").unwrap();
            config.set_bool("core.autocrlf", false).unwrap();
            config.set_str("core.eol", "lf").unwrap();
            config.set_bool("core.safecrlf", false).unwrap();
            config.set_bool("commit.gpgSign", false).unwrap();
            let hooks = repo.path().join("hooks");
            config
                .set_str("core.hooksPath", hooks.to_str().unwrap())
                .unwrap();
        }
        repo.set_head("refs/heads/main").unwrap();
        write(dir.path(), "tc", "plain\n");
        write(dir.path(), "d/f", "inner\n");
        write(dir.path(), "a.txt", "base\n");
        commit_all(&repo);
        (dir, repo)
    }

    #[cfg(unix)]
    fn outside() -> TempDir {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "target.txt", "OUTSIDE\n");
        write(dir.path(), "f", "OUTSIDE\n");
        dir
    }

    #[cfg(unix)]
    fn assert_outside_untouched(out: &Path) {
        assert_eq!(
            std::fs::read_to_string(out.join("target.txt")).unwrap(),
            "OUTSIDE\n"
        );
        assert_eq!(std::fs::read_to_string(out.join("f")).unwrap(), "OUTSIDE\n");
    }

    #[cfg(unix)]
    fn link_tc_and_d(dir: &Path, out: &Path) {
        std::fs::remove_file(dir.join("tc")).unwrap();
        std::os::unix::fs::symlink(out.join("target.txt"), dir.join("tc")).unwrap();
        std::fs::remove_dir_all(dir.join("d")).unwrap();
        std::os::unix::fs::symlink(out, dir.join("d")).unwrap();
    }

    fn head_tree(repo: &Repository) -> git2::Tree<'_> {
        repo.head().unwrap().peel_to_tree().unwrap()
    }

    #[test]
    #[cfg(unix)]
    fn scan_finds_a_leaf_typechange_and_a_symlinked_directory() {
        let (dir, repo) = repo();
        let out = outside();
        link_tc_and_d(dir.path(), out.path());
        let tree = head_tree(&repo);

        let hazards = scan(dir.path(), &GuardSource::Tree(&tree), ALL).unwrap();
        assert_eq!(
            hazards,
            vec![Hazard::Leaf("tc".into()), Hazard::Ancestor("d".into())]
        );
        let index = repo.index().unwrap();
        let from_index = scan(dir.path(), &GuardSource::Index(&index), ALL).unwrap();
        assert_eq!(from_index, hazards);
    }

    #[test]
    #[cfg(unix)]
    fn scan_finds_a_nested_symlinked_directory() {
        let (dir, repo) = repo();
        let out = outside();
        write(dir.path(), "a/b/c/x", "x\n");
        commit_all(&repo);
        std::fs::remove_dir_all(dir.path().join("a/b")).unwrap();
        std::os::unix::fs::symlink(out.path(), dir.path().join("a/b")).unwrap();
        let tree = head_tree(&repo);

        let hazards = scan(dir.path(), &GuardSource::Tree(&tree), ALL).unwrap();
        assert_eq!(hazards, vec![Hazard::Ancestor("a/b".into())]);
    }

    #[test]
    #[cfg(unix)]
    fn a_link_the_source_also_has_as_a_link_is_not_a_hazard() {
        let (dir, repo) = repo();
        std::os::unix::fs::symlink("a.txt", dir.path().join("l")).unwrap();
        commit_all(&repo);
        let tree = head_tree(&repo);

        assert!(scan(dir.path(), &GuardSource::Tree(&tree), ALL)
            .unwrap()
            .is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn untracked_links_no_source_path_uses_are_left_alone() {
        let (dir, repo) = repo();
        let out = outside();
        std::os::unix::fs::symlink(out.path(), dir.path().join("extra")).unwrap();
        let tree = head_tree(&repo);

        let hazards = scan(dir.path(), &GuardSource::Tree(&tree), ALL).unwrap();
        assert!(hazards.is_empty());
        clear_hazards(dir.path(), &hazards).unwrap();
        assert!(is_link(&dir.path().join("extra")));
    }

    #[test]
    #[cfg(unix)]
    fn scan_limited_to_paths_ignores_other_hazards() {
        let (dir, repo) = repo();
        let out = outside();
        link_tc_and_d(dir.path(), out.path());
        let tree = head_tree(&repo);

        let only_tc = scan(dir.path(), &GuardSource::Tree(&tree), Some(&["tc"])).unwrap();
        assert_eq!(only_tc, vec![Hazard::Leaf("tc".into())]);
        let below_d = scan(dir.path(), &GuardSource::Tree(&tree), Some(&["d"])).unwrap();
        assert_eq!(below_d, vec![Hazard::Ancestor("d".into())]);
        let prefix_only = scan(dir.path(), &GuardSource::Tree(&tree), Some(&["t"])).unwrap();
        assert!(prefix_only.is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn clear_hazards_removes_only_the_links() {
        let (dir, repo) = repo();
        let out = outside();
        link_tc_and_d(dir.path(), out.path());
        let tree = head_tree(&repo);

        let hazards = scan(dir.path(), &GuardSource::Tree(&tree), ALL).unwrap();
        clear_hazards(dir.path(), &hazards).unwrap();

        assert!(std::fs::symlink_metadata(dir.path().join("tc")).is_err());
        assert!(std::fs::symlink_metadata(dir.path().join("d")).is_err());
        assert_outside_untouched(out.path());
    }

    #[test]
    #[cfg(unix)]
    fn clear_hazards_leaves_a_path_that_is_no_longer_a_link() {
        let (dir, _repo) = repo();
        clear_hazards(dir.path(), &[Hazard::Leaf("tc".into())]).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("tc")).unwrap(),
            "plain\n"
        );
    }

    #[test]
    #[cfg(unix)]
    fn remove_beneath_does_not_follow_a_symlinked_ancestor() {
        let (dir, _repo) = repo();
        let out = outside();
        link_tc_and_d(dir.path(), out.path());

        assert!(!remove_beneath(dir.path(), "d/f").unwrap());
        assert!(remove_beneath(dir.path(), "tc").unwrap());
        assert!(std::fs::symlink_metadata(dir.path().join("tc")).is_err());
        assert!(!remove_beneath(dir.path(), "../x").unwrap());
        assert_outside_untouched(out.path());
    }

    #[test]
    #[cfg(unix)]
    fn write_beneath_replaces_a_leaf_link_and_refuses_a_linked_ancestor() {
        let (dir, _repo) = repo();
        let out = outside();
        link_tc_and_d(dir.path(), out.path());

        assert!(write_beneath(dir.path(), "tc", b"new").unwrap());
        assert!(!is_link(&dir.path().join("tc")));
        assert_eq!(std::fs::read(dir.path().join("tc")).unwrap(), b"new");
        assert!(!write_beneath(dir.path(), "d/f", b"x").unwrap());
        assert!(!write_beneath(dir.path(), "d/../tc", b"x").unwrap());
        assert_outside_untouched(out.path());
    }

    #[test]
    fn selected_matches_paths_and_folders_and_honours_ignore_case() {
        assert!(selected(b"d/f", &["d"], false));
        assert!(selected(b"d/f", &["d/"], false));
        assert!(selected(b"tc", &["tc"], false));
        assert!(!selected(b"tcx", &["tc"], false));
        assert!(!selected(b"tc", &["TC"], false));
        assert!(selected(b"tc", &["TC"], true));
        assert!(selected(b"D/f", &["d"], true));
        assert!(!selected(b"d", &["d/f"], true));
    }

    #[test]
    fn odd_paths_count_as_beyond_a_link() {
        let (dir, _repo) = repo();
        assert!(has_symlinked_ancestor(dir.path(), "../x"));
        assert!(has_symlinked_ancestor(dir.path(), "/etc/passwd"));
        assert!(!has_symlinked_ancestor(dir.path(), "d/f"));
        assert!(replace_beneath(dir.path(), "../x", Content::Link(b"x")).is_err());
    }

    #[test]
    fn scan_on_a_clean_worktree_is_empty() {
        let (dir, repo) = repo();
        let tree = head_tree(&repo);
        assert!(scan(dir.path(), &GuardSource::Tree(&tree), ALL)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn quoted_candidates_find_the_path_in_every_libgit2_wording() {
        let workdir = Path::new("C:/w/repo/");
        let candidates = |message: &str| quoted_candidates(workdir, message);
        // libgit2 1.9's wordings for a file it can't remove, open or find.
        for message in [
            "could not remove 'C:/w/repo/d/b.txt': The process cannot access the file",
            r"could not remove 'C:\w\repo\d\b.txt': Der Prozess kann nicht zugreifen",
            "could not remove 'C:/w/repo/d/b.txt'",
            "failed remove - 'C:/w/repo/d/b.txt' is locked",
            "could not find 'C:/w/repo/d/b.txt' to remove",
            "could not open 'C:/w/repo/d/b.txt' for writing: Access is denied.",
            "failed to remove file 'C:/w/repo/d/b.txt'",
        ] {
            assert_eq!(candidates(message), ["d/b.txt"], "{message}");
        }
        // A name holding `'` (and even `': `) gives every reading, longest first; the probe
        // picks the one that exists.
        assert_eq!(
            candidates("could not remove 'C:/w/repo/it's.txt': in use"),
            ["it's.txt", "it"]
        );
        assert_eq!(
            candidates("failed remove - 'C:/w/repo/a' b.txt' is locked"),
            ["a' b.txt", "a"]
        );
        // Outside the work tree, a sibling folder sharing its prefix, `..`, a stream or drive,
        // no closing quote, or no path at all: nothing to probe.
        for message in [
            "could not remove 'C:/w/other/a.txt': x",
            "could not remove 'C:/w/repo2/a.txt': x",
            "could not remove 'C:/w/repo/../other/a.txt': x",
            "could not remove 'C:/w/repo/./a.txt': x",
            "could not remove 'C:/w/repo/d//a.txt': x",
            "could not remove 'C:/w/repo/': x",
            "could not remove 'C:/w/repo/a.txt",
            "failed to write",
        ] {
            assert!(candidates(message).is_empty(), "{message}");
        }
    }

    #[test]
    fn quoted_candidates_match_a_long_path_prefix_either_way_round() {
        let plain = Path::new(r"C:\w\repo");
        let long = Path::new(r"\\?\C:\w\repo");
        let unc = Path::new(r"\\server\share\repo");
        let long_unc = Path::new(r"\\?\UNC\server\share\repo");
        for (workdir, message) in [
            (plain, r"could not remove '\\?\C:\w\repo\d\b.txt': x"),
            (long, r"could not remove 'C:/w/repo/d/b.txt': x"),
            (long, r"could not remove '\\?\C:\w\repo\d\b.txt': x"),
            (
                unc,
                r"could not remove '\\?\UNC\server\share\repo\d\b.txt': x",
            ),
            (
                long_unc,
                r"could not remove '//server/share/repo/d/b.txt': x",
            ),
            (
                long_unc,
                r"could not remove '\\?\UNC\server\share\repo\d\b.txt': x",
            ),
        ] {
            assert_eq!(
                quoted_candidates(workdir, message),
                ["d/b.txt"],
                "{workdir:?} {message}"
            );
        }
        assert!(quoted_candidates(unc, r"could not remove '\\other\share\repo\a': x").is_empty());
    }

    #[test]
    fn held_open_in_names_the_longest_quoted_file_that_is_held() {
        let workdir = Path::new("/w/repo");
        let os_error = |message: &str| {
            git2::Error::new(git2::ErrorCode::GenericError, git2::ErrorClass::Os, message)
        };
        let held = |_: &Path, rel: &str| matches!(rel, "d/f.txt" | "it" | "it's.txt");
        assert_eq!(
            held_open_in(
                workdir,
                &os_error("could not remove '/w/repo/d/f.txt': x"),
                held
            ),
            Some("d/f.txt".to_string())
        );
        assert_eq!(
            held_open_in(
                workdir,
                &os_error("failed remove - '/w/repo/d/f.txt' is locked"),
                held
            ),
            Some("d/f.txt".to_string())
        );
        assert_eq!(
            held_open_in(
                workdir,
                &os_error("could not remove '/w/repo/it's.txt': x"),
                held
            ),
            Some("it's.txt".to_string())
        );
        assert_eq!(
            held_open_in(
                workdir,
                &os_error("could not remove '/w/repo/g.txt': x"),
                held
            ),
            None
        );
        // Only an OS or file-system error names a file in the way.
        let checkout = git2::Error::new(
            git2::ErrorCode::Conflict,
            git2::ErrorClass::Checkout,
            "could not remove '/w/repo/d/f.txt': x",
        );
        assert_eq!(held_open_in(workdir, &checkout, held), None);
    }

    /// Windows: the real probe finds nobody holding the file, so libgit2's error stays.
    #[cfg(windows)]
    #[test]
    fn held_open_keeps_libgit2s_error_for_a_file_nobody_holds() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f.txt"), "x").unwrap();
        let message = format!(
            "could not remove '{}': x",
            dir.path().join("f.txt").display()
        );
        let error = git2::Error::new(
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Os,
            &message,
        );
        let mapped: crate::branch::BranchError =
            held_open(Some(dir.path()), HeldDuring::Switch)(error);
        assert!(
            matches!(mapped, crate::branch::BranchError::Git(ref e) if e.message() == message),
            "{mapped:?}"
        );
    }

    /// Windows: a file another program holds without sharing delete is named.
    #[cfg(windows)]
    #[test]
    fn held_open_names_a_file_another_program_holds() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let file = dir.path().join("d").join("f.txt");
        std::fs::write(&file, "x").unwrap();
        let _editor = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(&file)
            .unwrap();
        let error = git2::Error::new(
            git2::ErrorCode::Locked,
            git2::ErrorClass::Os,
            format!("failed remove - '{}' is locked", file.display()),
        );
        let mapped: crate::branch::BranchError =
            held_open(Some(dir.path()), HeldDuring::Switch)(error);
        assert!(
            matches!(mapped, crate::branch::BranchError::HeldOpen(ref held) if held.path == "d/f.txt"),
            "{mapped:?}"
        );
    }

    #[test]
    fn held_open_messages_say_what_was_changed_and_how_to_finish() {
        let message = |during| {
            HeldOpen {
                path: "f.txt".to_string(),
                during,
            }
            .to_string()
        };
        assert_eq!(
            message(HeldDuring::Switch),
            "'f.txt' is open in another program. Some files were already switched. Close it, \
             then try again to finish."
        );
        assert_eq!(
            message(HeldDuring::Reset),
            "'f.txt' is open in another program. Some files were already reset. Close it, then \
             try again to finish."
        );
        for during in [HeldDuring::FastForward, HeldDuring::SubmoduleUpdate] {
            assert_eq!(
                message(during),
                "'f.txt' is open in another program. Some files were already updated. Close \
                 it, then try again to finish."
            );
        }
        assert_eq!(
            message(HeldDuring::RebaseAbort),
            "'f.txt' is open in another program. Some files were already put back. Close it, \
             then abort again to finish."
        );
        assert_eq!(
            message(HeldDuring::Clone),
            "'f.txt' is open in another program. Close it, then try again."
        );
    }

    #[test]
    fn quoted_in_message_finds_the_quoted_path_in_either_slash_direction() {
        let paths = ["a.txt", "d/b.txt"];
        let workdir = Path::new("C:/w/repo/");
        let quoted = |message: &str| quoted_in_message(workdir, message, &paths);
        assert_eq!(
            quoted("could not remove 'C:/w/repo/d/b.txt': in use"),
            Some("d/b.txt")
        );
        assert_eq!(
            quoted(r"could not remove 'C:\w\repo\d\b.txt': in use"),
            Some("d/b.txt")
        );
        assert_eq!(quoted("could not remove 'C:/w/repo/a.txt.bak': x"), None);
        assert_eq!(quoted("could not remove 'C:/w/repo/e/a.txt': x"), None);
        assert_eq!(quoted("failed to write"), None);
    }

    #[test]
    fn remove_and_write_beneath_work_on_plain_files() {
        let (dir, _repo) = repo();
        assert!(write_beneath(dir.path(), "new/dir/x.txt", b"x").unwrap());
        assert_eq!(
            std::fs::read(dir.path().join("new/dir/x.txt")).unwrap(),
            b"x"
        );
        assert!(remove_beneath(dir.path(), "new/dir/x.txt").unwrap());
        assert!(remove_beneath(dir.path(), "missing.txt").unwrap());
        assert!(!remove_beneath(dir.path(), "d").unwrap());
        assert!(dir.path().join("d/f").exists());
    }

    /// A tree from `(path, mode, content)` entries, built without touching the work tree.
    fn tree_of<'r>(repo: &'r Repository, entries: &[(&str, i32, &str)]) -> git2::Tree<'r> {
        let mut index = git2::Index::new().unwrap();
        for (path, mode, content) in entries {
            let id = if *mode == MODE_GITLINK as i32 {
                repo.head().unwrap().target().unwrap()
            } else {
                repo.blob(content.as_bytes()).unwrap()
            };
            index
                .add(&crate::undo::index_entry(path, id, *mode as u32))
                .unwrap();
        }
        let id = index.write_tree_to(repo).unwrap();
        repo.find_tree(id).unwrap()
    }

    const LINK: i32 = MODE_LINK as i32;
    const FILE: i32 = 0o100644;

    fn collision(link: &str, folder: &str) -> Option<CaseCollision> {
        Some(CaseCollision {
            link: link.into(),
            folder: folder.into(),
        })
    }

    #[test]
    fn a_link_and_a_folder_differing_in_case_collide() {
        let (_dir, repo) = repo();
        let tree = tree_of(&repo, &[("D", LINK, "/x"), ("d/x", FILE, "x")]);
        let found = find_case_collision(&GuardSource::Tree(&tree), ALL).unwrap();
        assert_eq!(found, collision("D", "d"));
    }

    #[test]
    fn the_reverse_order_collides() {
        let (_dir, repo) = repo();
        let tree = tree_of(&repo, &[("D/x", FILE, "x"), ("d", LINK, "/x")]);
        let found = find_case_collision(&GuardSource::Tree(&tree), ALL).unwrap();
        assert_eq!(found, collision("d", "D"));
    }

    #[test]
    fn a_nested_link_collides_with_a_folder_of_another_case() {
        let (_dir, repo) = repo();
        let tree = tree_of(&repo, &[("A/b/c", FILE, "c"), ("a/B", LINK, "/x")]);
        let found = find_case_collision(&GuardSource::Tree(&tree), ALL).unwrap();
        assert_eq!(found, collision("a/B", "A/b"));
    }

    #[test]
    fn a_gitlink_counts_as_a_link() {
        let (_dir, repo) = repo();
        let tree = tree_of(
            &repo,
            &[("Sub", MODE_GITLINK as i32, ""), ("sub/x", FILE, "x")],
        );
        let found = find_case_collision(&GuardSource::Tree(&tree), ALL).unwrap();
        assert_eq!(found, collision("Sub", "sub"));
    }

    #[test]
    fn the_same_name_as_link_and_folder_collides() {
        // `TreeBuilder` and `Index::add` both refuse such a pair, so only a crafted tree object
        // holds one; check the pure core directly.
        let found = first_collision([(&b"d"[..], MODE_LINK), (&b"d/x"[..], 0o100644)]);
        assert_eq!(found, collision("d", "d"));
    }

    #[test]
    fn unicode_case_and_hfs_ignorables_fold() {
        let (_dir, repo) = repo();
        let tree = tree_of(&repo, &[("Ä", LINK, "/x"), ("ä/x", FILE, "x")]);
        assert_eq!(
            find_case_collision(&GuardSource::Tree(&tree), ALL).unwrap(),
            collision("Ä", "ä")
        );
        let tree = tree_of(&repo, &[("d\u{200c}", LINK, "/x"), ("d/x", FILE, "x")]);
        assert_eq!(
            find_case_collision(&GuardSource::Tree(&tree), ALL).unwrap(),
            collision("d\u{200c}", "d")
        );
        assert_eq!(fold_name("Straße\u{feff}".as_bytes()), "strasse".as_bytes());
        assert_eq!(fold_name(b"AB\xff"), b"ab\xff");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_linux_temp_dir_does_not_fold_case() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(!dir_folds_case(dir.path()).unwrap());
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn the_temp_dir_folds_case() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(dir_folds_case(dir.path()).unwrap());
    }

    #[test]
    fn the_probe_leaves_nothing_behind() {
        let dir = tempfile::TempDir::new().unwrap();
        dir_folds_case(dir.path()).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn the_probe_uses_the_nearest_existing_ancestor() {
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("a").join("b").join("c");
        assert_eq!(nearest_existing(&missing), Some(dir.path()));
        assert_eq!(nearest_existing(dir.path()), Some(dir.path()));
        assert_eq!(
            nearest_existing(Path::new("no-such-dir/x")),
            Some(Path::new("."))
        );
    }

    #[test]
    fn the_ascii_fast_path_matches_the_unicode_fold() {
        for byte in 0u8..=0x7f {
            let text = (byte as char).to_string();
            assert_eq!(fold_name(&[byte]), fold_text(&text), "byte {byte:#04x}");
        }
        let all: String = (0u8..=0x7f).map(char::from).collect();
        assert_eq!(fold_name(all.as_bytes()), fold_text(&all));
    }

    #[test]
    fn nfc_and_nfd_spellings_fold_together() {
        assert_eq!(
            fold_name("\u{e9}".as_bytes()),
            fold_name("e\u{301}".as_bytes())
        );
        assert_eq!(
            fold_name("\u{c9}".as_bytes()),
            fold_name("e\u{301}".as_bytes())
        );
        // A precomposed Hangul syllable and its jamo.
        assert_eq!(
            fold_name("\u{ac00}".as_bytes()),
            fold_name("\u{1100}\u{1161}".as_bytes())
        );
        for (link, folder) in [("\u{e9}", "e\u{301}"), ("e\u{301}", "\u{e9}")] {
            let found = first_collision([
                (link.as_bytes().to_vec(), MODE_LINK),
                (format!("{folder}/x").into_bytes(), 0o100644),
            ]);
            assert_eq!(found, collision(link, folder), "{link:?} / {folder:?}");
        }
        // Different letters stay apart.
        assert_ne!(
            fold_name("\u{e9}".as_bytes()),
            fold_name("\u{e8}".as_bytes())
        );
    }

    #[test]
    fn sharp_s_capital_and_small_fold_together() {
        assert_eq!(fold_name("ẞ".as_bytes()), fold_name("ß".as_bytes()));
        assert_eq!(fold_name("ß".as_bytes()), fold_name(b"ss"));
        let found = first_collision([
            ("ẞ".as_bytes().to_vec(), MODE_LINK),
            ("ß/x".as_bytes().to_vec(), 0o100644),
        ]);
        assert_eq!(found, collision("ẞ", "ß"));
    }

    #[test]
    fn folding_is_idempotent_for_every_scalar_value() {
        let fold = |text: &str| String::from_utf8(fold_name(text.as_bytes())).unwrap();
        let mut failures = Vec::new();
        for c in '\0'..=char::MAX {
            let once = fold(c.encode_utf8(&mut [0; 4]));
            let upper: String = c.to_uppercase().collect();
            if fold(&once) != once || fold(&upper) != once {
                failures.push(c);
            }
        }
        let first: Vec<String> = failures
            .iter()
            .take(20)
            .map(|c| format!("U+{:04X}", u32::from(*c)))
            .collect();
        assert!(
            failures.is_empty(),
            "{} failures: {first:?}",
            failures.len()
        );
    }

    #[test]
    fn case_folding_follows_the_file_systems_not_just_lowercase() {
        for (link, folder) in [("s", "ſ"), ("σ", "ς"), ("µ", "μ"), ("i", "ı")] {
            let found = first_collision([
                (link.as_bytes().to_vec(), MODE_LINK),
                (format!("{folder}/x").into_bytes(), 0o100644),
            ]);
            assert_eq!(found, collision(link, folder), "{link} / {folder}");
        }
    }

    #[test]
    fn a_link_and_a_gitlink_of_the_same_folded_name_collide() {
        let found = first_collision([(&b"A"[..], MODE_GITLINK), (&b"a"[..], MODE_LINK)]);
        assert_eq!(found, collision("a", "A"));
        let two_links = first_collision([(&b"A"[..], MODE_LINK), (&b"a"[..], MODE_LINK)]);
        assert_eq!(two_links, None);
    }

    #[test]
    fn every_collision_is_listed_once() {
        let entries = [
            (&b"D"[..], MODE_LINK, 0),
            (&b"Z"[..], MODE_LINK, 0),
            (&b"d/x"[..], 0o100644, 0),
            (&b"d/y"[..], 0o100644, 0),
            (&b"z/x"[..], 0o100644, 0),
        ];
        assert_eq!(
            collisions_across(entries, true),
            [collision("D", "d").unwrap(), collision("Z", "z").unwrap()]
        );
        assert_eq!(collisions_across(entries, false).len(), 1);
    }

    #[test]
    fn sources_are_checked_together() {
        let (_dir, repo) = repo();
        let links = tree_of(&repo, &[("D", LINK, "/x")]);
        let folders = tree_of(&repo, &[("d/x", FILE, "x")]);
        let sources = [GuardSource::Tree(&links), GuardSource::Tree(&folders)];
        assert_eq!(
            find_case_collision_in(&sources, ALL).unwrap(),
            collision("D", "d")
        );
        // The same name as a link in one source and a folder in another is one path changing
        // kind (a stash of a folder that replaced a link), not a collision.
        let same = tree_of(&repo, &[("d", LINK, "/x")]);
        let sources = [GuardSource::Tree(&same), GuardSource::Tree(&folders)];
        assert_eq!(find_case_collision_in(&sources, ALL).unwrap(), None);
    }

    #[test]
    fn find_case_collisions_in_returns_every_collision_across_sources() {
        let (_dir, repo) = repo();
        let first = tree_of(
            &repo,
            &[("D", LINK, "/x"), ("K", LINK, "/x"), ("d/x", FILE, "x")],
        );
        let second = tree_of(
            &repo,
            &[("Z", LINK, "/x"), ("k/x", FILE, "x"), ("z/x", FILE, "x")],
        );
        let sources = [GuardSource::Tree(&first), GuardSource::Tree(&second)];
        assert_eq!(
            find_case_collisions_in(&sources, ALL).unwrap(),
            [
                collision("D", "d").unwrap(),
                collision("K", "k").unwrap(),
                collision("Z", "z").unwrap(),
            ]
        );
    }

    /// Commits `tree` on HEAD without touching the work tree, and folds case.
    fn commit_tree_on_head(repo: &Repository, tree: &git2::Tree) {
        let sig = repo.signature().unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "pair", tree, &[&parent])
            .unwrap();
        repo.config()
            .unwrap()
            .set_bool("core.ignorecase", true)
            .unwrap();
    }

    #[test]
    fn a_collision_head_already_holds_is_not_brought_in() {
        let (_dir, repo) = repo();
        let pair = tree_of(&repo, &[("Db", LINK, "/x"), ("dB/x", FILE, "x")]);
        commit_tree_on_head(&repo, &pair);
        let incoming = tree_of(
            &repo,
            &[
                ("Db", LINK, "/x"),
                ("dB/x", FILE, "x"),
                ("b.txt", FILE, "b"),
            ],
        );
        let result: Result<(), crate::merge::MergeError> =
            refuse_linked_folders(&repo, &[Incoming::Merge(GuardSource::Tree(&incoming))]);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn a_collision_spelled_differently_from_heads_is_not_known() {
        let (_dir, repo) = repo();
        let pair = tree_of(&repo, &[("Db", LINK, "/x"), ("dB/x", FILE, "x")]);
        commit_tree_on_head(&repo, &pair);
        let incoming = tree_of(&repo, &[("Db", LINK, "/x"), ("DB/x", FILE, "x")]);
        let result: Result<(), crate::merge::MergeError> =
            refuse_linked_folders(&repo, &[Incoming::Merge(GuardSource::Tree(&incoming))]);
        assert_eq!(
            result.unwrap_err().to_string(),
            collision("Db", "DB").unwrap().to_string()
        );
    }

    /// The verifier's probe (#167 follow-up F2): HEAD holds `Db -> ../outA` + `dB/x`; a branch
    /// retargets the link and adds `dB/evil`. Same spelling, but not the same link.
    #[test]
    fn a_known_collision_whose_link_changes_is_brought_in() {
        let (_dir, repo) = repo();
        let pair = tree_of(&repo, &[("Db", LINK, "../outA"), ("dB/x", FILE, "x")]);
        commit_tree_on_head(&repo, &pair);
        let retargeted = tree_of(
            &repo,
            &[
                ("Db", LINK, "../outB"),
                ("dB/evil", FILE, "evil"),
                ("dB/x", FILE, "x"),
            ],
        );
        let merged: Result<(), crate::merge::MergeError> =
            refuse_linked_folders(&repo, &[Incoming::Merge(GuardSource::Tree(&retargeted))]);
        assert_eq!(
            merged.unwrap_err().to_string(),
            collision("Db", "dB").unwrap().to_string()
        );
        let switched: Result<(), crate::merge::MergeError> =
            refuse_linked_folders(&repo, &[Incoming::Checkout(&retargeted)]);
        assert!(switched.is_err());
        let reset: Result<(), crate::reset::ResetError> =
            prepare_forced_checkout(&repo, Some(&GuardSource::Tree(&retargeted)), ALL);
        assert!(reset.is_err());
        // The same link with more below its folder stays exempt (the disk scan guards the
        // folder).
        let same = tree_of(
            &repo,
            &[
                ("Db", LINK, "../outA"),
                ("dB/evil", FILE, "e"),
                ("dB/x", FILE, "x"),
            ],
        );
        let unchanged: Result<(), crate::merge::MergeError> =
            refuse_linked_folders(&repo, &[Incoming::Merge(GuardSource::Tree(&same))]);
        assert!(unchanged.is_ok(), "{unchanged:?}");
    }

    #[test]
    fn an_also_known_collision_must_keep_its_link_too() {
        let (_dir, repo) = repo();
        repo.config()
            .unwrap()
            .set_bool("core.ignorecase", true)
            .unwrap();
        let known = tree_of(&repo, &[("D", LINK, "/a"), ("d/x", FILE, "x")]);
        let target = tree_of(&repo, &[("D", LINK, "/b"), ("d/x", FILE, "x")]);
        let result: Result<(), crate::reset::ResetError> = prepare_forced_checkout_known(
            &repo,
            Some(&GuardSource::Tree(&target)),
            ALL,
            Some(&known),
        );
        assert!(result.is_err());
    }

    /// An invalid `core.ignorecase` refuses the forced-checkout guard on every OS, with or
    /// without a target, before any link is removed (#225).
    #[test]
    fn the_forced_checkout_guard_refuses_an_invalid_ignorecase() {
        let (_dir, repo) = repo();
        repo.config()
            .unwrap()
            .set_str("core.ignorecase", "maybe")
            .unwrap();
        let plain = tree_of(&repo, &[("a", FILE, "a")]);
        for target in [Some(GuardSource::Tree(&plain)), None] {
            let result: Result<(), crate::reset::ResetError> =
                prepare_forced_checkout(&repo, target.as_ref(), ALL);
            assert!(
                matches!(result, Err(crate::reset::ResetError::CaseConfig(_))),
                "{result:?}"
            );
        }
        repo.config().unwrap().remove("core.ignorecase").unwrap();
        let result: Result<(), crate::reset::ResetError> =
            prepare_forced_checkout(&repo, Some(&GuardSource::Tree(&plain)), ALL);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn also_known_exempts_only_its_own_collisions() {
        let (_dir, repo) = repo();
        repo.config()
            .unwrap()
            .set_bool("core.ignorecase", true)
            .unwrap();
        let target = tree_of(
            &repo,
            &[
                ("D", LINK, "/x"),
                ("Z", LINK, "/x"),
                ("d/x", FILE, "x"),
                ("z/x", FILE, "x"),
            ],
        );
        let known = tree_of(&repo, &[("D", LINK, "/x"), ("d/x", FILE, "x")]);
        let result: Result<(), crate::reset::ResetError> = prepare_forced_checkout_known(
            &repo,
            Some(&GuardSource::Tree(&target)),
            ALL,
            Some(&known),
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            collision("Z", "z").unwrap().to_string()
        );
        let only_d = tree_of(&repo, &[("D", LINK, "/x"), ("d/x", FILE, "x")]);
        let result: Result<(), crate::reset::ResetError> = prepare_forced_checkout_known(
            &repo,
            Some(&GuardSource::Tree(&only_d)),
            ALL,
            Some(&known),
        );
        assert!(result.is_ok(), "{result:?}");
        let result: Result<(), crate::merge::MergeError> =
            refuse_linked_folders_known(&repo, &[Incoming::Checkout(&target)], Some(&known));
        assert_eq!(
            result.unwrap_err().to_string(),
            collision("Z", "z").unwrap().to_string()
        );
    }

    #[test]
    fn a_file_and_a_folder_differing_in_case_do_not_collide() {
        let (_dir, repo) = repo();
        let tree = tree_of(&repo, &[("D", FILE, "x"), ("d/x", FILE, "x")]);
        assert_eq!(
            find_case_collision(&GuardSource::Tree(&tree), ALL).unwrap(),
            None
        );
    }

    #[test]
    fn distinct_names_do_not_collide() {
        let (_dir, repo) = repo();
        let tree = tree_of(
            &repo,
            &[
                ("D", LINK, "/x"),
                ("e/x", FILE, "x"),
                ("Dd/x", FILE, "x"),
                ("dx", FILE, "x"),
            ],
        );
        assert_eq!(
            find_case_collision(&GuardSource::Tree(&tree), ALL).unwrap(),
            None
        );
    }

    #[test]
    fn paths_limit_the_check() {
        let (_dir, repo) = repo();
        let tree = tree_of(&repo, &[("D", LINK, "/x"), ("d/x", FILE, "x")]);
        let source = GuardSource::Tree(&tree);
        // Paths match ignoring case, as libgit2's checkout under `core.ignorecase` does: `d`
        // selects the link `D` too.
        assert_eq!(find_case_collision(&source, Some(&["d/x"])).unwrap(), None);
        assert_eq!(find_case_collision(&source, Some(&["e"])).unwrap(), None);
        assert_eq!(
            find_case_collision(&source, Some(&["d"])).unwrap(),
            collision("D", "d")
        );
        assert_eq!(
            find_case_collision(&source, Some(&["D", "d/x"])).unwrap(),
            collision("D", "d")
        );
    }

    #[test]
    fn an_index_source_is_checked_like_a_tree() {
        let (_dir, repo) = repo();
        let tree = tree_of(&repo, &[("D", LINK, "/x"), ("d/x", FILE, "x")]);
        let mut index = git2::Index::new().unwrap();
        index.read_tree(&tree).unwrap();
        assert_eq!(
            find_case_collision(&GuardSource::Index(&index), ALL).unwrap(),
            collision("D", "d")
        );
    }

    #[test]
    fn try_folds_case_reads_core_ignorecase() {
        let (_dir, repo) = repo();
        let mut config = repo.config().unwrap();
        config.set_bool("core.ignorecase", false).unwrap();
        assert_eq!(
            try_folds_case(&repo).unwrap(),
            cfg!(any(target_os = "macos", windows))
        );
        config.set_bool("core.ignorecase", true).unwrap();
        assert!(try_folds_case(&repo).unwrap());
        config.remove("core.ignorecase").unwrap();
        assert_eq!(
            try_folds_case(&repo).unwrap(),
            cfg!(any(target_os = "macos", windows))
        );
    }

    /// An invalid `core.ignorecase` is an error, not "case doesn't fold" (#225). On macOS and
    /// Windows case always folds, so the setting isn't read there.
    #[test]
    fn an_invalid_core_ignorecase_is_an_error() {
        let (_dir, repo) = repo();
        repo.config()
            .unwrap()
            .set_str("core.ignorecase", "maybe")
            .unwrap();
        let folds = try_folds_case(&repo);
        let applies = CasePolicy::FromRepo.applies(&repo);
        if cfg!(any(target_os = "macos", windows)) {
            assert!(folds.unwrap());
            assert!(applies.unwrap());
        } else {
            let error = folds.unwrap_err();
            assert_eq!(
                error.to_string(),
                "Can't read core.ignorecase from the Git config: failed to parse 'maybe' as a \
                 boolean. Set it to true or false (or remove it), then try again."
            );
            assert_eq!(applies, Err(error));
        }
        assert!(CasePolicy::Always.applies(&repo).unwrap());
    }

    #[test]
    fn case_collision_message_is_exact() {
        assert_eq!(
            collision("D", "d").unwrap().to_string(),
            "'D' is a symbolic link and 'd' a folder whose names differ only in case. On this \
             file system they are the same name, so Browsitory won't check them out. Rename one \
             of them in the repository."
        );
    }

    fn read_all(file: std::fs::File) -> String {
        use std::io::Read;
        let mut text = String::new();
        let mut file = file;
        file.read_to_string(&mut text).unwrap();
        text
    }

    #[test]
    fn open_beneath_opens_a_regular_file_in_a_subdirectory() {
        let (dir, _repo) = repo();
        let file = open_beneath(dir.path(), Path::new("d/f")).unwrap().unwrap();
        assert_eq!(read_all(file), "inner\n");
    }

    #[test]
    fn open_beneath_returns_none_for_a_missing_path_or_a_directory() {
        let (dir, _repo) = repo();
        assert!(open_beneath(dir.path(), Path::new("nope"))
            .unwrap()
            .is_none());
        assert!(open_beneath(dir.path(), Path::new("nope/x"))
            .unwrap()
            .is_none());
        assert!(open_beneath(dir.path(), Path::new("d")).unwrap().is_none());
        assert!(open_beneath(dir.path(), Path::new("../x"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn open_beneath_returns_none_when_a_component_is_a_file() {
        let (dir, _repo) = repo();
        assert!(open_beneath(dir.path(), Path::new("a.txt/x"))
            .unwrap()
            .is_none());
    }

    #[test]
    #[cfg(unix)]
    fn open_beneath_refuses_a_leaf_symlink() {
        let (dir, _repo) = repo();
        let out = outside();
        link_tc_and_d(dir.path(), out.path());
        assert!(open_beneath(dir.path(), Path::new("tc")).unwrap().is_none());
    }

    #[test]
    #[cfg(unix)]
    fn open_beneath_refuses_a_symlinked_intermediate_directory() {
        let (dir, _repo) = repo();
        let out = outside();
        write(out.path(), "secret.png", "OUTSIDE");
        std::os::unix::fs::symlink(out.path(), dir.path().join("rd")).unwrap();
        write(dir.path(), "x/y/z", "z");
        std::os::unix::fs::symlink(out.path(), dir.path().join("x/y/l")).unwrap();

        assert!(open_beneath(dir.path(), Path::new("rd/secret.png"))
            .unwrap()
            .is_none());
        assert!(open_beneath(dir.path(), Path::new("x/y/l/secret.png"))
            .unwrap()
            .is_none());
        assert!(open_beneath(dir.path(), Path::new("x/y/z"))
            .unwrap()
            .is_some());
    }

    #[test]
    #[cfg(unix)]
    fn open_beneath_does_not_block_on_a_fifo() {
        let (dir, _repo) = repo();
        let fifo = dir.path().join("pipe");
        let name = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        // SAFETY: `name` is a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(open_beneath(dir.path(), Path::new("pipe"))
            .unwrap()
            .is_none());
    }

    /// A junction `link` -> `target` (no privilege needed).
    #[cfg(windows)]
    fn junction(link: &Path, target: &Path) {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");
    }

    #[cfg(windows)]
    fn outside_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "f", "OUTSIDE\n");
        dir
    }

    #[cfg(windows)]
    fn assert_outside_dir_untouched(out: &Path) {
        let names: Vec<_> = std::fs::read_dir(out)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["f"]);
        assert_eq!(std::fs::read_to_string(out.join("f")).unwrap(), "OUTSIDE\n");
    }

    #[test]
    #[cfg(windows)]
    fn open_beneath_refuses_a_junction_to_an_inside_folder() {
        let (dir, _repo) = repo();
        junction(&dir.path().join("j"), &dir.path().join("d"));
        assert!(open_beneath(dir.path(), Path::new("j/f"))
            .unwrap()
            .is_none());
        assert!(open_beneath(dir.path(), Path::new("d/f"))
            .unwrap()
            .is_some());
    }

    #[test]
    #[cfg(windows)]
    fn open_beneath_refuses_a_junction_to_an_outside_folder() {
        let (dir, _repo) = repo();
        let out = outside_dir();
        junction(&dir.path().join("j"), out.path());
        write(dir.path(), "x/y/z", "z");
        // `mklink` takes backslashes only.
        junction(&dir.path().join("x").join("y").join("l"), out.path());
        assert!(open_beneath(dir.path(), Path::new("j/f"))
            .unwrap()
            .is_none());
        assert!(open_beneath(dir.path(), Path::new("x/y/l/f"))
            .unwrap()
            .is_none());
        assert_eq!(
            read_all(
                open_beneath(dir.path(), Path::new("x/y/z"))
                    .unwrap()
                    .unwrap()
            ),
            "z"
        );
    }

    #[test]
    #[cfg(windows)]
    fn open_beneath_refuses_a_file_symlink_leaf() {
        let (dir, _repo) = repo();
        let out = outside_dir();
        if let Err(error) =
            std::os::windows::fs::symlink_file(out.path().join("f"), dir.path().join("l"))
        {
            eprintln!("SKIP no symlink privilege: {error}");
            return;
        }
        assert!(open_beneath(dir.path(), Path::new("l")).unwrap().is_none());
        assert_eq!(
            read_beneath(dir.path(), "l")
                .unwrap()
                .map(|leaf| matches!(leaf, Leaf::Link { .. })),
            Some(true)
        );
    }

    #[test]
    #[cfg(windows)]
    fn write_beneath_refuses_a_junction_ancestor() {
        let (dir, _repo) = repo();
        let out = outside_dir();
        junction(&dir.path().join("j"), out.path());
        assert!(!write_beneath(dir.path(), "j/f", b"PWNED").unwrap());
        assert!(!write_beneath(dir.path(), "j/new", b"PWNED").unwrap());
        assert!(write_beneath(dir.path(), "d/f", b"new").unwrap());
        assert_eq!(std::fs::read(dir.path().join("d/f")).unwrap(), b"new");
        assert_outside_dir_untouched(out.path());
    }

    #[test]
    #[cfg(windows)]
    fn remove_beneath_refuses_a_junction_ancestor() {
        let (dir, _repo) = repo();
        let out = outside_dir();
        junction(&dir.path().join("j"), out.path());
        assert!(!remove_beneath(dir.path(), "j/f").unwrap());
        assert_outside_dir_untouched(out.path());
        // The junction itself is a link leaf: removed, never its target.
        assert!(remove_beneath(dir.path(), "j").unwrap());
        assert!(std::fs::symlink_metadata(dir.path().join("j")).is_err());
        assert_outside_dir_untouched(out.path());
    }

    #[test]
    #[cfg(windows)]
    fn clear_hazards_removes_the_junction_not_its_target() {
        let (dir, _repo) = repo();
        let out = outside_dir();
        std::fs::remove_dir_all(dir.path().join("d")).unwrap();
        junction(&dir.path().join("d"), out.path());
        clear_hazards(dir.path(), &[Hazard::Ancestor("d".into())]).unwrap();
        assert!(std::fs::symlink_metadata(dir.path().join("d")).is_err());
        assert_outside_dir_untouched(out.path());
    }

    #[test]
    #[cfg(windows)]
    fn replace_beneath_clears_a_junction_on_the_way() {
        let (dir, _repo) = repo();
        let out = outside_dir();
        std::fs::remove_dir_all(dir.path().join("d")).unwrap();
        junction(&dir.path().join("d"), out.path());
        replace_beneath(
            dir.path(),
            "d/f",
            Content::File {
                bytes: b"inside",
                executable: false,
            },
        )
        .unwrap();
        let meta = std::fs::symlink_metadata(dir.path().join("d")).unwrap();
        assert!(meta.is_dir() && !meta.file_type().is_symlink());
        assert_eq!(std::fs::read(dir.path().join("d/f")).unwrap(), b"inside");
        assert_outside_dir_untouched(out.path());
        // Pruning stops at the work tree.
        remove_and_prune_beneath(dir.path(), Path::new("d/f")).unwrap();
        assert!(!dir.path().join("d").exists());
        assert!(dir.path().join("a.txt").exists());
    }

    /// #213: reads below a probed folder stay in that folder after its path became a link to a
    /// folder outside or another folder, and never open a FIFO.
    #[test]
    #[cfg(unix)]
    fn a_held_folder_is_read_through_its_handle_after_its_path_is_swapped() {
        let dir = tempfile::TempDir::new().unwrap();
        let out = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("n/sub")).unwrap();
        std::fs::write(dir.path().join("n/sub/f"), "held\n").unwrap();
        std::fs::create_dir_all(out.path().join("sub")).unwrap();
        std::fs::write(out.path().join("sub/f"), "OUTSIDE\n").unwrap();
        let Probe::Folder(held) = probe_beneath(dir.path(), "n").unwrap() else {
            panic!("not probed as a folder");
        };
        std::fs::rename(dir.path().join("n"), dir.path().join("moved")).unwrap();
        std::os::unix::fs::symlink(out.path(), dir.path().join("n")).unwrap();
        assert_eq!(read_held(&held, "sub/f"), "held\n");
        assert!(matches!(
            probe_beneath(dir.path(), "n").unwrap(),
            Probe::Link
        ));
        assert!(matches!(
            probe_beneath(dir.path(), "n/sub/f").unwrap(),
            Probe::BeyondLink
        ));
        // A FIFO below the held folder is classified, never opened.
        let fifo = dir.path().join("moved/sub/p");
        let c_path = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        // SAFETY: a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) }, 0);
        assert_eq!(held.read("sub/p", u64::MAX).unwrap(), None);
        assert!(matches!(
            probe_beneath(dir.path(), "moved/sub/p").unwrap(),
            Probe::Other
        ));
        assert!(matches!(
            probe_beneath(dir.path(), "moved/sub/f").unwrap(),
            Probe::File
        ));
        assert!(matches!(
            probe_beneath(dir.path(), "moved/missing").unwrap(),
            Probe::Missing
        ));
    }

    /// The regular file `rel` below `held`, as text.
    #[cfg(unix)]
    fn read_held(held: &HeldFolder, rel: &str) -> String {
        match held.read(rel, u64::MAX).unwrap() {
            Some(Leaf::File { bytes, .. }) => String::from_utf8(bytes).unwrap(),
            other => panic!("{other:?}"),
        }
    }

    /// `resolve`'s answer as text, for comparing.
    #[cfg(any(unix, windows))]
    fn outcome(resolved: &Resolved) -> &'static str {
        match resolved {
            Resolved::Held(_) => "held",
            Resolved::NotFound => "not found",
            Resolved::NotAFolder => "not a folder",
            Resolved::Outside => "outside",
            Resolved::TooDeep => "too deep",
        }
    }

    #[cfg(unix)]
    fn held(resolved: Resolved) -> HeldFolder {
        match resolved {
            Resolved::Held(folder) => folder,
            other => panic!("{}", outcome(&other)),
        }
    }

    /// #213: `..` out of a held folder is the held folder above, not its name: the parent
    /// swapped for a link to a folder outside after the probe is never looked through. A name
    /// that is a link is no folder; `..` above the work tree root is outside, whatever is there.
    #[test]
    #[cfg(unix)]
    fn resolving_up_from_a_held_folder_goes_through_the_held_parent() {
        let dir = tempfile::TempDir::new().unwrap();
        let out = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("a/n")).unwrap();
        std::fs::create_dir_all(dir.path().join("a/x")).unwrap();
        std::fs::write(dir.path().join("a/x/f"), "held\n").unwrap();
        std::fs::create_dir_all(out.path().join("x")).unwrap();
        std::fs::write(out.path().join("x/f"), "OUTSIDE\n").unwrap();
        let Probe::Folder(folder) = probe_beneath(dir.path(), "a/n").unwrap() else {
            panic!("not probed as a folder");
        };
        std::fs::rename(dir.path().join("a"), dir.path().join("moved")).unwrap();
        std::os::unix::fs::symlink(out.path(), dir.path().join("a")).unwrap();
        let x = held(folder.resolve(Path::new("../x")).unwrap());
        assert_eq!(read_held(&x, "f"), "held\n");
        // An absolute path inside the work tree starts from the held work tree root, whose `a`
        // is now the link: not followed.
        let inside = folder.resolve(&dir.path().join("a/x")).unwrap();
        assert_eq!(outcome(&inside), "not a folder");
        let moved = held(folder.resolve(&dir.path().join("moved/x")).unwrap());
        assert_eq!(read_held(&moved, "f"), "held\n");
        // `..` in an absolute path is applied to held folders too: `moved/../moved/x`.
        let again = held(
            folder
                .resolve(&dir.path().join("moved/../moved/x"))
                .unwrap(),
        );
        assert_eq!(read_held(&again, "f"), "held\n");
        assert_eq!(
            outcome(&folder.resolve(Path::new("../../a")).unwrap()),
            "not a folder"
        );
        assert_eq!(
            outcome(&folder.resolve(Path::new("missing")).unwrap()),
            "not found"
        );
        // Out of the work tree by `..`, even to come back in: outside.
        let name = out.path().file_name().unwrap().to_str().unwrap();
        let work = dir.path().file_name().unwrap().to_str().unwrap();
        for target in [
            format!("../../../{name}/x"),
            format!("../../../{work}/moved/x"),
        ] {
            assert_eq!(
                outcome(&folder.resolve(Path::new(&target)).unwrap()),
                "outside",
                "{target}"
            );
        }
    }

    /// #213: an absolute path that isn't below the work tree root as it was opened is outside,
    /// and nothing is opened or looked up for it: not an existing folder, not a sibling whose
    /// name only starts like the work tree's (`b` and `bc`), not another spelling of the work
    /// tree (a symlinked one here).
    #[test]
    #[cfg(any(unix, windows))]
    fn an_absolute_path_not_below_the_opened_work_tree_root_is_outside() {
        let base = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(base.path().join("b/n")).unwrap();
        std::fs::create_dir_all(base.path().join("bc/git")).unwrap();
        let Probe::Folder(folder) = probe_beneath(&base.path().join("b"), "n").unwrap() else {
            panic!("not probed as a folder");
        };
        assert_eq!(
            outcome(&folder.resolve(&base.path().join("bc/git")).unwrap()),
            "outside"
        );
        assert_eq!(
            outcome(&folder.resolve(&base.path().join("b/../bc/git")).unwrap()),
            "outside"
        );
        assert_eq!(
            outcome(&folder.resolve(&base.path().join("b/n")).unwrap()),
            "held"
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.path().join("b"), base.path().join("alias")).unwrap();
            assert_eq!(
                outcome(&folder.resolve(&base.path().join("alias/n")).unwrap()),
                "outside"
            );
        }
        #[cfg(windows)]
        for target in [
            r"\\host.invalid\share\x",
            r"//host.invalid/share/x",
            r"\\?\UNC\host.invalid\share\x",
            r"\\.\pipe\x",
            r"\\?\C:\x",
        ] {
            let started = std::time::Instant::now();
            assert_eq!(
                outcome(&folder.resolve(Path::new(target)).unwrap()),
                "outside",
                "{target}"
            );
            assert!(
                started.elapsed() < std::time::Duration::from_secs(2),
                "{target}"
            );
        }
    }

    /// #213: a target with more names than the cap is refused before any folder is held; `.`,
    /// `..` and the root don't count.
    #[test]
    #[cfg(any(unix, windows))]
    fn resolving_a_too_deep_path_is_refused() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("n")).unwrap();
        let Probe::Folder(folder) = probe_beneath(dir.path(), "n").unwrap() else {
            panic!("not probed as a folder");
        };
        let deep = |names: usize| "x/".repeat(names);
        assert_eq!(
            outcome(
                &folder
                    .resolve(Path::new(&deep(RESOLVE_MAX_COMPONENTS)))
                    .unwrap()
            ),
            "not found"
        );
        assert_eq!(
            outcome(
                &folder
                    .resolve(Path::new(&format!(
                        "./{}../x",
                        deep(RESOLVE_MAX_COMPONENTS - 1)
                    )))
                    .unwrap()
            ),
            "not found"
        );
        assert_eq!(
            outcome(
                &folder
                    .resolve(Path::new(&deep(RESOLVE_MAX_COMPONENTS + 1)))
                    .unwrap()
            ),
            "too deep"
        );
        let names = dir
            .path()
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count();
        let mut path = dir.path().to_path_buf();
        for _ in names..=RESOLVE_MAX_COMPONENTS {
            path.push("x");
        }
        assert_eq!(outcome(&folder.resolve(&path).unwrap()), "too deep");
        path.pop();
        assert_eq!(outcome(&folder.resolve(&path).unwrap()), "not found");
    }

    /// #213: a read through a held folder stops past its limit.
    #[test]
    #[cfg(any(unix, windows))]
    fn a_held_folder_read_is_capped() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("n")).unwrap();
        std::fs::write(dir.path().join("n/f"), "12345").unwrap();
        let Probe::Folder(held) = probe_beneath(dir.path(), "n").unwrap() else {
            panic!("not probed as a folder");
        };
        assert!(matches!(
            held.read("f", 5).unwrap(),
            Some(Leaf::File { ref bytes, .. }) if bytes == b"12345"
        ));
        let error = held.read("f", 4).unwrap_err();
        assert!(is_too_large(&error), "{error:?}");
    }

    /// #213: Windows holds a probed folder without sharing delete, so it can't be renamed or
    /// deleted while held (nor a folder above it); its stat data comes from the held handle.
    #[test]
    #[cfg(windows)]
    fn a_held_folder_cannot_be_renamed_or_deleted_and_has_stat_data() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("a/n")).unwrap();
        let Probe::Folder(held) = probe_beneath(dir.path(), "a/n").unwrap() else {
            panic!("not probed as a folder");
        };
        assert!(std::fs::rename(dir.path().join("a/n"), dir.path().join("a/m")).is_err());
        assert!(std::fs::remove_dir(dir.path().join("a/n")).is_err());
        assert!(std::fs::rename(dir.path().join("a"), dir.path().join("b")).is_err());
        let stat = held.stat().unwrap();
        let meta = std::fs::metadata(dir.path().join("a/n")).unwrap();
        assert_eq!(stat, FileStat::of(&meta));
        assert_ne!(stat.mtime, (0, 0));
        drop(held);
        std::fs::rename(dir.path().join("a/n"), dir.path().join("a/m")).unwrap();
    }

    #[test]
    #[cfg(windows)]
    fn read_beneath_does_not_read_through_a_junction() {
        let (dir, _repo) = repo();
        let out = outside_dir();
        junction(&dir.path().join("j"), out.path());
        assert_eq!(read_beneath(dir.path(), "j/f").unwrap(), None);
        assert!(matches!(
            read_beneath(dir.path(), "d/f").unwrap(),
            Some(Leaf::File { .. })
        ));
    }

    /// Stress test for #138: a thread swaps `rd` between a real directory and a symlink to an
    /// outside directory while the preview reads `rd/secret.png`. Run manually:
    /// `cargo test -p git-core directory_swap_race -- --ignored`.
    #[test]
    #[cfg(unix)]
    #[ignore = "stress: run manually"]
    fn directory_swap_race_never_reads_outside() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let (dir, repo) = repo();
        let out = outside();
        write(out.path(), "secret.png", "OUTSIDE");
        let real = dir.path().join("real");
        write(&real, "secret.png", "inside");
        let rd = dir.path().join("rd");
        let real_dir = dir.path().join("rd-real");
        std::fs::rename(&real, &real_dir).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let swapper = {
            let (stop, rd, outside) = (stop.clone(), rd.clone(), out.path().to_path_buf());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = std::fs::rename(&real_dir, &rd);
                    let _ = std::fs::rename(&rd, &real_dir);
                    let _ = std::os::unix::fs::symlink(&outside, &rd);
                    let _ = std::fs::remove_file(&rd);
                }
            })
        };
        let mut leaks = 0;
        let mut loaded = 0;
        for _ in 0..200_000 {
            if let Ok(crate::diff::FileSides {
                new: crate::diff::FileSide::Loaded { bytes },
                ..
            }) = crate::diff::file_sides(
                &repo,
                &crate::tools::DiffSource::Unstaged,
                "rd/secret.png",
                1 << 20,
            ) {
                loaded += 1;
                if bytes == b"OUTSIDE" {
                    leaks += 1;
                }
            }
        }
        stop.store(true, Ordering::Relaxed);
        swapper.join().unwrap();
        eprintln!("loaded {loaded} inside reads, {leaks} leaks");
        assert_eq!(leaks, 0);
    }

    /// `h/x.sh` in a new work tree, a hard link to `other` in a folder outside it, both
    /// holding `old`.
    fn hard_linked() -> (TempDir, TempDir) {
        let dir = TempDir::new().unwrap();
        let out = TempDir::new().unwrap();
        std::fs::write(out.path().join("other"), "old").unwrap();
        std::fs::create_dir(dir.path().join("h")).unwrap();
        std::fs::hard_link(out.path().join("other"), dir.path().join("h/x.sh")).unwrap();
        (dir, out)
    }

    #[test]
    fn separate_hard_link_gives_the_file_a_name_of_its_own() {
        let (dir, out) = hard_linked();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::Permissions::from_mode(0o750);
            std::fs::set_permissions(dir.path().join("h/x.sh"), mode).unwrap();
        }
        assert_eq!(link_count(&dir.path().join("h/x.sh")).unwrap(), 2);

        assert!(separate_hard_link(dir.path(), Path::new("h/x.sh")).unwrap());

        assert_eq!(link_count(&dir.path().join("h/x.sh")).unwrap(), 1);
        assert_eq!(link_count(&out.path().join("other")).unwrap(), 1);
        assert_eq!(std::fs::read(dir.path().join("h/x.sh")).unwrap(), b"old");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(dir.path().join("h/x.sh")).unwrap();
            assert_eq!(meta.permissions().mode() & 0o777, 0o750);
        }
        std::fs::write(dir.path().join("h/x.sh"), "new").unwrap();
        assert_eq!(std::fs::read(out.path().join("other")).unwrap(), b"old");
        assert_eq!(
            std::fs::read_dir(dir.path().join("h")).unwrap().count(),
            1,
            "no temporary file left"
        );
    }

    #[test]
    fn separate_hard_link_leaves_a_file_with_one_name_and_a_missing_path() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f"), "one").unwrap();
        let before = std::fs::metadata(dir.path().join("f"))
            .unwrap()
            .modified()
            .unwrap();
        assert!(!separate_hard_link(dir.path(), Path::new("f")).unwrap());
        assert!(!separate_hard_link(dir.path(), Path::new("missing/f")).unwrap());
        assert!(!separate_hard_link(dir.path(), Path::new("../f")).unwrap());
        let after = std::fs::metadata(dir.path().join("f"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn separate_hard_link_writes_nothing_beyond_a_linked_folder() {
        // `l` is a link (Windows: a junction) to the outside folder holding the hard-linked
        // `other`: nothing is written there.
        let (dir, out) = hard_linked();
        #[cfg(unix)]
        std::os::unix::fs::symlink(out.path(), dir.path().join("l")).unwrap();
        #[cfg(windows)]
        junction(&dir.path().join("l"), out.path());
        assert!(!separate_hard_link(dir.path(), Path::new("l/other")).unwrap());
        assert_eq!(link_count(&out.path().join("other")).unwrap(), 2);
        assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 1);
    }
}
