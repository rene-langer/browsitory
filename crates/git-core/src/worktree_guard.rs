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
pub(crate) fn scan<P: AsRef<[u8]>>(
    workdir: &Path,
    source: &GuardSource,
    paths: Option<&[P]>,
) -> Result<Vec<Hazard>, git2::Error> {
    scan_with_case(workdir, source, paths, false)
}

/// `scan` with the `paths` filter ignoring ASCII case when `ignore_case`.
fn scan_with_case<P: AsRef<[u8]>>(
    workdir: &Path,
    source: &GuardSource,
    paths: Option<&[P]>,
    ignore_case: bool,
) -> Result<Vec<Hazard>, git2::Error> {
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
    let mut dirs: HashMap<PathBuf, Entry> = HashMap::new();
    let mut hazards = BTreeSet::new();
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
        let components: Vec<_> = rel.components().collect();
        let mut current = PathBuf::new();
        let mut blocked = false;
        for component in &components[..components.len() - 1] {
            current.push(component);
            let kind = *dirs
                .entry(current.clone())
                .or_insert_with(|| entry_kind(&workdir.join(&current)));
            match kind {
                Entry::Dir => {}
                Entry::Link => {
                    hazards.insert(Hazard::Ancestor(current.clone()));
                    blocked = true;
                    break;
                }
                Entry::Other => {
                    blocked = true;
                    break;
                }
            }
        }
        if !blocked && entry.mode != MODE_LINK && is_link(&workdir.join(&rel)) {
            hazards.insert(Hazard::Leaf(rel));
        }
    }
    Ok(hazards.into_iter().collect())
}

/// Removes each hazard's link, never its target and never recursively. A path that is no longer
/// a symlink (or already gone) is left alone. Call immediately before the forced checkout.
pub(crate) fn clear_hazards(workdir: &Path, hazards: &[Hazard]) -> io::Result<()> {
    for hazard in hazards {
        let full = workdir.join(hazard.path());
        if !is_link(&full) {
            continue;
        }
        remove_link(&full)?;
    }
    Ok(())
}

/// Clears every hazard of a forced checkout in `repo`'s worktree, optionally limited to the
/// literal `paths`: those of the checkout's `target` and those of the current HEAD tree and
/// index. The latter matter because a forced checkout also *deletes* what the baseline tracks
/// and the target lacks (`d/f` through a symlinked `d` would delete a file outside the
/// repository). Call immediately before the checkout, after anything that must record the
/// links (a stash).
pub(crate) fn prepare_forced_checkout<E, P>(
    repo: &git2::Repository,
    target: Option<&GuardSource>,
    paths: Option<&[P]>,
) -> Result<(), E>
where
    E: From<git2::Error> + From<io::Error>,
    P: AsRef<[u8]>,
{
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    // libgit2 matches checkout paths case-insensitively under `core.ignorecase`; so does the
    // filter here, or `TC` would scan nothing for a tracked `tc`.
    let ignore_case = repo
        .config()
        .and_then(|config| config.get_bool("core.ignorecase"))
        .unwrap_or(false);
    let mut hazards = BTreeSet::new();
    if let Some(target) = target {
        hazards.extend(scan_with_case(workdir, target, paths, ignore_case)?);
    }
    if let Ok(head_tree) = repo.head().and_then(|head| head.peel_to_tree()) {
        hazards.extend(scan_with_case(
            workdir,
            &GuardSource::Tree(&head_tree),
            paths,
            ignore_case,
        )?);
    }
    let index = repo.index()?;
    hazards.extend(scan_with_case(
        workdir,
        &GuardSource::Index(&index),
        paths,
        ignore_case,
    )?);
    clear_hazards(workdir, &hazards.into_iter().collect::<Vec<_>>())?;
    Ok(())
}

/// Refuses, as a checkout conflict, while a symlink stands where HEAD or the index tracks a
/// folder. libgit2's *safe* checkouts (switch, checkout, merge, cherry-pick, revert, rebase,
/// stash apply, pull) refuse to modify a file below such a link but still *create* new ones
/// through it, truncating whatever the link's target folder holds under that name. Call it right
/// before each of them. (A link where nothing tracked a folder already conflicts in libgit2.)
pub(crate) fn refuse_linked_folders(repo: &git2::Repository) -> Result<(), git2::Error> {
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    const ALL: Option<&[&str]> = None;
    let mut hazards = Vec::new();
    if let Ok(head_tree) = repo.head().and_then(|head| head.peel_to_tree()) {
        hazards.extend(scan(workdir, &GuardSource::Tree(&head_tree), ALL)?);
    }
    let index = repo.index()?;
    hazards.extend(scan(workdir, &GuardSource::Index(&index), ALL)?);
    match hazards.into_iter().find_map(|hazard| match hazard {
        Hazard::Ancestor(dir) => Some(dir),
        Hazard::Leaf(_) => None,
    }) {
        Some(dir) => Err(git2::Error::new(
            git2::ErrorCode::Conflict,
            git2::ErrorClass::Checkout,
            format!(
                "'{}' is a symbolic link where the repository has a folder. Move the link away or \
                 restore the folder first.",
                dir.display()
            ),
        )),
        None => Ok(()),
    }
}

/// Removes the symlink (or Windows junction) `full` itself.
fn remove_link(full: &Path) -> io::Result<()> {
    match std::fs::remove_file(full) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        // A Windows directory symlink or junction is removed with `RemoveDirectoryW`, which
        // removes the link and not the directory it points to.
        #[cfg(windows)]
        Err(_) if is_link(full) => match std::fs::remove_dir(full) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        },
        Err(error) => Err(error),
    }
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

/// Removes the first symlink among the strict ancestors of `rel` (the link only), so a write to
/// `rel` lands in a real directory inside the repository.
pub(crate) fn clear_symlinked_ancestors(workdir: &Path, rel: &str) -> io::Result<()> {
    let rel = Path::new(rel);
    if !plain_relative(rel) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path must be relative to the repository",
        ));
    }
    let mut current = workdir.to_path_buf();
    let components: Vec<_> = rel.components().collect();
    for component in &components[..components.len() - 1] {
        current.push(component);
        match entry_kind(&current) {
            Entry::Dir => {}
            Entry::Link => return remove_link(&current),
            Entry::Other => return Ok(()),
        }
    }
    Ok(())
}

/// Removes the file or link `rel` without following a link at the leaf or above it. Returns
/// `Ok(false)` and does nothing when an ancestor is a symlink, `rel` is a directory, or `rel` is
/// not a plain relative path. A missing `rel` counts as removed.
pub(crate) fn remove_beneath(workdir: &Path, rel: &str) -> io::Result<bool> {
    if !plain_relative(Path::new(rel)) || has_symlinked_ancestor(workdir, rel) {
        return Ok(false);
    }
    let full = workdir.join(rel);
    match std::fs::symlink_metadata(&full) {
        Ok(meta) if meta.is_dir() => Ok(false),
        Ok(meta) if meta.file_type().is_symlink() => remove_link(&full).map(|()| true),
        Ok(_) => match std::fs::remove_file(&full) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(true),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
    }
}

/// Writes `bytes` to `rel` as a regular file, first removing a symlink at `rel` (never writing
/// through it). Returns `Ok(false)` and writes nothing when an ancestor is a symlink or `rel` is
/// not a plain relative path.
pub(crate) fn write_beneath(workdir: &Path, rel: &str, bytes: &[u8]) -> io::Result<bool> {
    if !plain_relative(Path::new(rel)) || has_symlinked_ancestor(workdir, rel) {
        return Ok(false);
    }
    let full = workdir.join(rel);
    if is_link(&full) {
        remove_link(&full)?;
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&full, bytes)?;
    Ok(true)
}

/// Opens the regular file `rel` below `workdir` for reading without following a symlink at any
/// component below `workdir`, and without blocking on a FIFO. `None` when any component is a
/// symlink, a component is not a directory, the path is missing, or the leaf is not a regular
/// file. It never falls back to an open by path.
///
/// Each step opens relative to the directory handle of the previous step (`openat` with
/// `O_NOFOLLOW`), so a directory swapped for a symlink at any depth, at any moment, cannot
/// redirect the read. `workdir` itself may be reached through a symlink (the user chose it).
#[cfg(unix)]
pub(crate) fn open_beneath(workdir: &Path, rel: &Path) -> io::Result<Option<std::fs::File>> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;

    if !plain_relative(rel) {
        return Ok(None);
    }
    let gone = |e: &io::Error| {
        matches!(
            e.raw_os_error(),
            Some(libc::ELOOP | libc::ENOTDIR | libc::ENOENT)
        )
    };
    let open_at =
        |dir: libc::c_int, name: &std::ffi::OsStr, flags: libc::c_int| -> io::Result<OwnedFd> {
            let name = CString::new(name.as_bytes())
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
            // SAFETY: `dir` is an open directory descriptor owned by the caller for the duration
            // of the call and `name` is a NUL-terminated string that outlives it.
            let fd = unsafe { libc::openat(dir, name.as_ptr(), flags | libc::O_CLOEXEC) };
            if fd < 0 {
                Err(io::Error::last_os_error())
            } else {
                // SAFETY: `openat` just returned this descriptor; nothing else owns it.
                Ok(unsafe { OwnedFd::from_raw_fd(fd) })
            }
        };

    let root = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY)
            .open(workdir)?
    };
    let mut dir: OwnedFd = root.into();
    let parts: Vec<&std::ffi::OsStr> = rel.components().map(|c| c.as_os_str()).collect();
    let Some((last, parents)) = parts.split_last() else {
        return Ok(None);
    };
    for part in parents {
        match open_at(
            dir.as_raw_fd(),
            part,
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        ) {
            Ok(next) => dir = next,
            Err(e) if gone(&e) => return Ok(None),
            Err(e) => return Err(e),
        }
    }
    let file = match open_at(
        dir.as_raw_fd(),
        last,
        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK,
    ) {
        Ok(fd) => std::fs::File::from(fd),
        Err(e) if gone(&e) => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(file.metadata()?.is_file().then_some(file))
}

/// Windows: opens `rel` without following a final reparse point, then checks the path the
/// *opened handle* resolves to: a junction or directory symlink anywhere on the way that leads
/// out of the repository is detected on the handle itself, so a swap before the open is caught
/// and a swap after it cannot change what was opened.
#[cfg(windows)]
pub(crate) fn open_beneath(workdir: &Path, rel: &Path) -> io::Result<Option<std::fs::File>> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const ERROR_INVALID_NAME: i32 = 123;
    const ERROR_DIRECTORY: i32 = 267;

    if !plain_relative(rel) {
        return Ok(None);
    }
    let root = std::fs::canonicalize(workdir)?;
    // No FILE_FLAG_BACKUP_SEMANTICS: only files can be opened, a directory fails (access denied).
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(workdir.join(rel))
    {
        Ok(file) => file,
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
            ) || matches!(e.raw_os_error(), Some(ERROR_INVALID_NAME | ERROR_DIRECTORY)) =>
        {
            return Ok(None)
        }
        Err(e) => return Err(e),
    };
    let meta = file.metadata()?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Ok(None);
    }
    let resolved = final_path(&file)?;
    Ok(resolved.starts_with(&root).then_some(file))
}

/// The normalized `\\?\` DOS path of an open handle (`GetFinalPathNameByHandleW`), the same
/// resolver `std::fs::canonicalize` uses.
#[cfg(windows)]
fn final_path(file: &std::fs::File) -> io::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetFinalPathNameByHandleW(
            file: *mut core::ffi::c_void,
            path: *mut u16,
            len: u32,
            flags: u32,
        ) -> u32;
    }

    let mut buf = vec![0u16; 512];
    loop {
        let capacity = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        // SAFETY: the handle belongs to `file`, which outlives the call; `buf` has `capacity`
        // writable u16 slots. Flags 0 = FILE_NAME_NORMALIZED | VOLUME_NAME_DOS.
        let len = unsafe {
            GetFinalPathNameByHandleW(file.as_raw_handle(), buf.as_mut_ptr(), capacity, 0)
        } as usize;
        if len == 0 {
            return Err(io::Error::last_os_error());
        }
        if len < buf.len() {
            buf.truncate(len);
            return Ok(PathBuf::from(std::ffi::OsString::from_wide(&buf)));
        }
        // Too small: `len` is the size needed, including the terminating NUL.
        buf.resize(len, 0);
    }
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
        assert!(clear_symlinked_ancestors(dir.path(), "../x").is_err());
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
