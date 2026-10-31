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
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
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

/// The code points HFS+ ignores in names (the list git's `is_hfs_dotgit` uses).
fn hfs_ignorable(c: char) -> bool {
    matches!(
        c,
        '\u{200c}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{206a}'..='\u{206f}' | '\u{feff}'
    )
}

/// `name` (git's bytes) as a case-insensitive file system compares it: valid UTF-8 lower-, upper-
/// then lowercased per character (so `ſ`/`s`, `ς`/`σ`, `µ`/`μ` and `ı`/`i` meet, as under APFS's
/// case folding and NTFS's upcase table; `ẞ` lowercases to `ß`, which the round trip takes on
/// to `ss`, so the fold is a fixed point (#166); `ß` meeting `ss` over-matches, which only
/// refuses more) without the HFS+ ignorable code points, anything else lowercased as ASCII. NFC/NFD
/// normalization is not folded (a documented residual, decision D4).
pub(crate) fn fold_name(name: &[u8]) -> Vec<u8> {
    match std::str::from_utf8(name) {
        Ok(text) => text
            .chars()
            .filter(|c| !hfs_ignorable(*c))
            .flat_map(char::to_lowercase)
            .flat_map(char::to_uppercase)
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .into_bytes(),
        Err(_) => name.to_ascii_lowercase(),
    }
}

/// Whether case collisions matter for `repo`: `core.ignorecase`, or always on macOS and Windows
/// (decision D1), where a repository copied from elsewhere can carry a wrong setting.
pub(crate) fn folds_case(repo: &git2::Repository) -> bool {
    cfg!(any(target_os = "macos", windows))
        || repo
            .config()
            .and_then(|config| config.get_bool("core.ignorecase"))
            .unwrap_or(false)
}

/// When a fresh checkout (clone, submodule) checks for case collisions. Tests can't preset a
/// fresh clone's config, so they force the check with `Always`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CasePolicy {
    /// [`folds_case`] of the repository being checked out.
    FromRepo,
    #[cfg_attr(not(test), allow(dead_code))]
    Always,
}

impl CasePolicy {
    pub(crate) fn applies(self, repo: &git2::Repository) -> bool {
        self == CasePolicy::Always || folds_case(repo)
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
/// matters for `repo` ([`folds_case`]).
pub(crate) fn refuse_case_collision<E, P>(
    repo: &git2::Repository,
    source: &GuardSource,
    paths: Option<&[P]>,
) -> Result<(), E>
where
    E: From<git2::Error> + From<CaseCollision>,
    P: AsRef<[u8]>,
{
    if !folds_case(repo) {
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
    E: From<git2::Error> + From<io::Error> + From<CaseCollision>,
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
    E: From<git2::Error> + From<io::Error> + From<CaseCollision>,
    P: AsRef<[u8]>,
{
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    // A collision the target brings in is refused; one HEAD already holds (aborting a merge,
    // `reset --hard` to HEAD, discarding) isn't, so such a repository can still be cleaned up.
    // Every collision of the target must already be known, not just the first one found.
    if let (Some(target), true) = (target, folds_case(repo)) {
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
    let ignore_case = repo
        .config()
        .and_then(|config| config.get_bool("core.ignorecase"))
        .unwrap_or(false);
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
    E: From<git2::Error> + From<LinkedFolder> + From<CaseCollision>,
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
    E: From<git2::Error> + From<LinkedFolder> + From<CaseCollision>,
{
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    const ALL: Option<&[&str]> = None;
    let head_tree = repo.head().and_then(|head| head.peel_to_tree()).ok();
    let index = repo.index()?;
    let mut sources = Vec::new();
    if let Some(head_tree) = &head_tree {
        sources.push((GuardSource::Tree(head_tree), false));
    }
    sources.push((GuardSource::Index(&index), false));
    for source in incoming {
        sources.push(match source {
            Incoming::Checkout(tree) => (GuardSource::Tree(tree), true),
            Incoming::Merge(GuardSource::Tree(tree)) => (GuardSource::Tree(tree), false),
            Incoming::Merge(GuardSource::Index(index)) => (GuardSource::Index(index), false),
        });
    }
    // Only folder links matter here, so files are never examined.
    let mut scanner = Scanner::new(workdir, false);
    let mut found = None;
    for (source, skip_head_links) in &sources {
        let mut hazards = BTreeSet::new();
        scanner.scan(source, ALL, false, &mut hazards)?;
        found = hazards.into_iter().find_map(|hazard| match hazard {
            Hazard::Ancestor(dir)
                if !(*skip_head_links && tracked_as_link(head_tree.as_ref(), &dir)) =>
            {
                Some(dir)
            }
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
    if folds_case(repo) {
        let sources: Vec<GuardSource> = incoming
            .iter()
            .map(|source| match source {
                Incoming::Checkout(tree) => GuardSource::Tree(tree),
                Incoming::Merge(GuardSource::Tree(tree)) => GuardSource::Tree(tree),
                Incoming::Merge(GuardSource::Index(index)) => GuardSource::Index(index),
            })
            .collect();
        let incoming = find_case_collisions_in(&sources, ALL)?;
        if !incoming.is_empty() {
            let known = known_collisions(repo, ALL, also_known)?;
            if let Some(collision) = brought_in(incoming, &sources, &known) {
                return Err(collision.into());
            }
        }
    }
    Ok(())
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

/// Writes `bytes` to `rel` as a regular file, first removing a symlink at `rel` (never writing
/// through it). Returns `Ok(false)` and writes nothing when an ancestor is a symlink or `rel` is
/// not a plain relative path. An existing file keeps its mode; a FIFO or device is an error,
/// never written.
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
    },
    /// The link's target (`/`-separated on Windows).
    Link(Vec<u8>),
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

/// Removes the file, link or empty folder `rel`, then the folders that emptied, up to (never
/// including) the work tree. Does nothing beyond a link: a path below one doesn't exist for git.
pub(crate) fn remove_and_prune_beneath(workdir: &Path, rel: &str) -> io::Result<()> {
    if !plain_relative(Path::new(rel)) {
        return Ok(());
    }
    beneath::remove_and_prune(workdir, Path::new(rel), &mut no_step)
}

/// Replaces whatever is at `rel` (a file, link or empty folder) with `content`. A link that
/// replaced a folder on the way is removed first and the folder recreated (never written
/// through); missing folders are created.
pub(crate) fn replace_beneath(workdir: &Path, rel: &str, content: Content<'_>) -> io::Result<()> {
    if !plain_relative(Path::new(rel)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be relative to the repository",
        ));
    }
    beneath::replace(workdir, Path::new(rel), content, &mut no_step)
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
    fn folds_case_reads_core_ignorecase() {
        let (_dir, repo) = repo();
        let mut config = repo.config().unwrap();
        config.set_bool("core.ignorecase", false).unwrap();
        assert_eq!(folds_case(&repo), cfg!(any(target_os = "macos", windows)));
        config.set_bool("core.ignorecase", true).unwrap();
        assert!(folds_case(&repo));
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
                .map(|leaf| matches!(leaf, Leaf::Link(_))),
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
        remove_and_prune_beneath(dir.path(), "d/f").unwrap();
        assert!(!dir.path().join("d").exists());
        assert!(dir.path().join("a.txt").exists());
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
}
