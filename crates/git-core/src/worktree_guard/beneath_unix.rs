//! Race-free raw work-tree I/O on unix (#149).
//!
//! Every walk opens one component at a time relative to the previous folder's descriptor
//! (`openat` with `O_NOFOLLOW | O_DIRECTORY`) and acts on the leaf through its parent's
//! descriptor (`unlinkat`, `openat`, `symlinkat`, `readlinkat`, `fstatat` without following).
//! A folder swapped for a link at any moment therefore fails the next `openat` (`ELOOP` /
//! `ENOTDIR`) instead of being followed, and an act on the leaf lands in the folder that was
//! opened, never in a link's target. The work tree root itself is opened by path: the user
//! chose it, links included.

use std::ffi::{CString, OsStr, OsString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

#[cfg(test)]
use super::no_step;
use super::{Content, Leaf, OnLink, OnMissing, Step, StepFn, Walk};

/// The folders from the work tree root down to the leaf's parent, each held open.
pub(super) struct Beneath {
    /// `dirs[0]` is the work tree root; `dirs[i + 1]` is `names[i]` opened in `dirs[i]`.
    dirs: Vec<OwnedFd>,
    /// Every component of the path, the leaf last (`names.len() == dirs.len()`).
    names: Vec<OsString>,
}

impl Beneath {
    fn parent(&self) -> RawFd {
        self.dirs[self.dirs.len() - 1].as_raw_fd()
    }

    fn leaf(&self) -> &OsStr {
        &self.names[self.names.len() - 1]
    }

    #[cfg(test)]
    pub(super) fn parent_fd(&self) -> RawFd {
        self.parent()
    }
}

fn cstr(name: &OsStr) -> io::Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
}

fn errno(error: &io::Error) -> i32 {
    error.raw_os_error().unwrap_or(0)
}

fn check(ret: libc::c_int) -> io::Result<()> {
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// `openat(dir, name, flags | O_CLOEXEC, mode)`.
pub(super) fn open_at(
    dir: RawFd,
    name: &OsStr,
    flags: libc::c_int,
    mode: libc::mode_t,
) -> io::Result<OwnedFd> {
    let name = cstr(name)?;
    // SAFETY: `dir` is an open directory descriptor the caller holds for the duration of the
    // call, and `name` is a NUL-terminated string that outlives it.
    let fd = unsafe {
        libc::openat(
            dir,
            name.as_ptr(),
            flags | libc::O_CLOEXEC,
            libc::c_uint::from(mode),
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `openat` just returned this descriptor; nothing else owns it.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// `fstatat(dir, name, AT_SYMLINK_NOFOLLOW)`; `None` when the name is missing.
pub(super) fn stat_at(dir: RawFd, name: &OsStr) -> io::Result<Option<libc::stat>> {
    let name = cstr(name)?;
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `dir` is a held directory descriptor, `name` is NUL-terminated and `stat` points
    // to writable memory of the right size for the call to fill.
    let ret = unsafe {
        libc::fstatat(
            dir,
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    match check(ret) {
        // SAFETY: `fstatat` succeeded, so it initialized `stat`.
        Ok(()) => Ok(Some(unsafe { stat.assume_init() })),
        Err(error) if matches!(errno(&error), libc::ENOENT | libc::ENOTDIR) => Ok(None),
        Err(error) => Err(error),
    }
}

fn fstat(fd: RawFd) -> io::Result<libc::stat> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: `fd` is an open descriptor held by the caller; `stat` is writable memory of the
    // right size.
    check(unsafe { libc::fstat(fd, stat.as_mut_ptr()) })?;
    // SAFETY: `fstat` succeeded, so it initialized `stat`.
    Ok(unsafe { stat.assume_init() })
}

fn file_type(stat: &libc::stat) -> libc::mode_t {
    stat.st_mode & libc::S_IFMT
}

fn is_link(stat: &libc::stat) -> bool {
    file_type(stat) == libc::S_IFLNK
}

fn is_dir(stat: &libc::stat) -> bool {
    file_type(stat) == libc::S_IFDIR
}

/// `unlinkat(dir, name, AT_REMOVEDIR when folder)`. Never follows a link: a link is removed
/// itself.
pub(super) fn unlink_at(dir: RawFd, name: &OsStr, folder: bool) -> io::Result<()> {
    let name = cstr(name)?;
    let flags = if folder { libc::AT_REMOVEDIR } else { 0 };
    // SAFETY: `dir` is a held directory descriptor and `name` is NUL-terminated.
    check(unsafe { libc::unlinkat(dir, name.as_ptr(), flags) })
}

fn mkdir_at(dir: RawFd, name: &OsStr) -> io::Result<()> {
    let name = cstr(name)?;
    // SAFETY: `dir` is a held directory descriptor and `name` is NUL-terminated.
    check(unsafe { libc::mkdirat(dir, name.as_ptr(), 0o777) })
}

/// The target of the link `name` in `dir`, as bytes.
pub(super) fn readlink_at(dir: RawFd, name: &OsStr) -> io::Result<Vec<u8>> {
    let name = cstr(name)?;
    let mut buf = vec![0u8; 256];
    loop {
        // SAFETY: `dir` is a held directory descriptor, `name` is NUL-terminated and `buf` has
        // `buf.len()` writable bytes.
        let len = unsafe {
            libc::readlinkat(
                dir,
                name.as_ptr(),
                buf.as_mut_ptr().cast::<libc::c_char>(),
                buf.len(),
            )
        };
        if len < 0 {
            return Err(io::Error::last_os_error());
        }
        let len = len as usize;
        if len < buf.len() {
            buf.truncate(len);
            return Ok(buf);
        }
        // Possibly truncated: retry with more room.
        buf.resize(buf.len() * 2, 0);
    }
}

/// Creates the link `name` in `dir` pointing at `target`.
pub(super) fn symlink_at(target: &[u8], dir: RawFd, name: &OsStr) -> io::Result<()> {
    let target = CString::new(target).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let name = cstr(name)?;
    // SAFETY: both strings are NUL-terminated and `dir` is a held directory descriptor.
    check(unsafe { libc::symlinkat(target.as_ptr(), dir, name.as_ptr()) })
}

/// How often a walk retries one component that keeps changing under it before giving up.
const ATTEMPTS: usize = 8;

/// Walks `rel` (a plain relative path, see `plain_relative`) below `workdir` down to its last
/// component's parent, holding each folder open. See the module docs.
pub(super) fn walk(
    workdir: &Path,
    rel: &Path,
    on_link: OnLink,
    on_missing: OnMissing,
    step: StepFn<'_>,
) -> io::Result<Walk<Beneath>> {
    use std::os::unix::fs::OpenOptionsExt;
    let names: Vec<OsString> = rel
        .components()
        .map(|c| c.as_os_str().to_os_string())
        .collect();
    if names.is_empty() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let root = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(workdir)?;
    let mut dirs: Vec<OwnedFd> = vec![root.into()];
    for name in &names[..names.len() - 1] {
        let mut next = None;
        for _ in 0..ATTEMPTS {
            let dir = dirs[dirs.len() - 1].as_raw_fd();
            match open_at(
                dir,
                name,
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
                0,
            ) {
                Ok(fd) => {
                    step(Step::Opened(name));
                    next = Some(fd);
                    break;
                }
                Err(error) if errno(&error) == libc::ENOENT => match on_missing {
                    OnMissing::Stop => return Ok(Walk::Missing),
                    OnMissing::Create => match mkdir_at(dir, name) {
                        Ok(()) => step(Step::Created(name)),
                        Err(error) if errno(&error) == libc::EEXIST => {}
                        Err(error) => return Err(error),
                    },
                },
                // A link (`ELOOP`; `EMLINK` on FreeBSD) or not a folder (`ENOTDIR`).
                Err(error)
                    if matches!(errno(&error), libc::ELOOP | libc::ENOTDIR | libc::EMLINK) =>
                {
                    match stat_at(dir, name)? {
                        Some(stat) if is_link(&stat) => match on_link {
                            OnLink::Refuse => return Ok(Walk::Link),
                            OnLink::Clear => {
                                match unlink_at(dir, name, false) {
                                    Ok(()) => step(Step::Cleared(name)),
                                    Err(error) if errno(&error) == libc::ENOENT => {}
                                    Err(error) => return Err(error),
                                }
                                if on_missing == OnMissing::Stop {
                                    return Ok(Walk::Link);
                                }
                            }
                        },
                        // Swapped back to a folder, or gone: try again.
                        Some(stat) if is_dir(&stat) => {}
                        None => {}
                        Some(_) => return Ok(Walk::NotDir),
                    }
                }
                Err(error) => return Err(error),
            }
        }
        match next {
            Some(fd) => dirs.push(fd),
            None => {
                return Err(io::Error::other(
                    "the working tree kept changing while it was being written",
                ))
            }
        }
    }
    Ok(Walk::Found(Beneath { dirs, names }))
}

fn gone(error: &io::Error) -> bool {
    matches!(
        errno(error),
        libc::ELOOP | libc::ENOTDIR | libc::ENOENT | libc::EMLINK | libc::ENXIO
    )
}

/// See `worktree_guard::open_beneath`.
pub(super) fn open_file(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<Option<File>> {
    let Walk::Found(at) = walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step)? else {
        return Ok(None);
    };
    let file = match open_at(
        at.parent(),
        at.leaf(),
        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        0,
    ) {
        Ok(fd) => File::from(fd),
        Err(error) if gone(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(file.metadata()?.is_file().then_some(file))
}

/// See `worktree_guard::write_beneath`.
pub(super) fn write(
    workdir: &Path,
    rel: &Path,
    bytes: &[u8],
    step: StepFn<'_>,
) -> io::Result<bool> {
    let at = match walk(workdir, rel, OnLink::Refuse, OnMissing::Create, step)? {
        Walk::Found(at) => at,
        Walk::Link | Walk::NotDir | Walk::Missing => return Ok(false),
    };
    // No O_TRUNC: only a regular file is truncated, after `fstat` says it is one. O_NONBLOCK:
    // a FIFO without a reader fails (`ENXIO`) instead of blocking.
    let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_NONBLOCK;
    let fd = match open_at(at.parent(), at.leaf(), flags, 0o666) {
        Ok(fd) => fd,
        Err(error) if matches!(errno(&error), libc::ELOOP | libc::EMLINK) => {
            // The leaf is a link: remove the link itself, then create a fresh file.
            match unlink_at(at.parent(), at.leaf(), false) {
                Ok(()) => {}
                Err(error) if errno(&error) == libc::ENOENT => {}
                Err(error) => return Err(error),
            }
            open_at(at.parent(), at.leaf(), flags | libc::O_EXCL, 0o666)?
        }
        Err(error) => return Err(error),
    };
    let mut file = File::from(fd);
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    // An existing file keeps its mode, like `fs::write`.
    file.set_len(0)?;
    file.write_all(bytes)?;
    Ok(true)
}

/// `unlinkat` of a leaf that isn't a folder; `Ok(false)` when it is (or became) one.
fn unlink_leaf(at: &Beneath) -> io::Result<bool> {
    match unlink_at(at.parent(), at.leaf(), false) {
        Ok(()) => Ok(true),
        Err(error) if errno(&error) == libc::ENOENT => Ok(true),
        // Linux says EISDIR, macOS EPERM for a folder.
        Err(error) if matches!(errno(&error), libc::EISDIR | libc::EPERM) => {
            match stat_at(at.parent(), at.leaf())? {
                Some(stat) if is_dir(&stat) => Ok(false),
                _ => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

/// See `worktree_guard::remove_beneath`.
pub(super) fn remove(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<bool> {
    let at = match walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step)? {
        Walk::Found(at) => at,
        Walk::Link => return Ok(false),
        Walk::Missing | Walk::NotDir => return Ok(true),
    };
    match stat_at(at.parent(), at.leaf())? {
        None => Ok(true),
        Some(stat) if is_dir(&stat) => Ok(false),
        Some(_) => unlink_leaf(&at),
    }
}

/// Removes the link `rel` (a hazard) itself, or the first folder on the way to it that became
/// a link. Anything that is no longer a link is left alone.
pub(super) fn clear_hazard(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<()> {
    let Walk::Found(at) = walk(workdir, rel, OnLink::Clear, OnMissing::Stop, step)? else {
        return Ok(());
    };
    match stat_at(at.parent(), at.leaf())? {
        Some(stat) if is_link(&stat) => unlink_leaf(&at).map(|_| ()),
        _ => Ok(()),
    }
}

/// See `worktree_guard::read_beneath`.
pub(super) fn read(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<Option<Leaf>> {
    let Walk::Found(at) = walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step)? else {
        return Ok(None);
    };
    let stat = match stat_at(at.parent(), at.leaf())? {
        None => return Ok(None),
        Some(stat) => stat,
    };
    if is_link(&stat) {
        return match readlink_at(at.parent(), at.leaf()) {
            Ok(target) => Ok(Some(Leaf::Link(target))),
            Err(error) if gone(&error) || errno(&error) == libc::EINVAL => Ok(None),
            Err(error) => Err(error),
        };
    }
    if is_dir(&stat) {
        return Ok(None);
    }
    let fd = match open_at(
        at.parent(),
        at.leaf(),
        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        0,
    ) {
        Ok(fd) => fd,
        Err(error) if gone(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    // Kind and exec bit from the opened descriptor, never from the name.
    let opened = fstat(fd.as_raw_fd())?;
    if file_type(&opened) != libc::S_IFREG {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    File::from(fd).read_to_end(&mut bytes)?;
    Ok(Some(Leaf::File {
        bytes,
        executable: opened.st_mode & 0o111 != 0,
        stat: file_stat(&opened),
    }))
}

/// `stat`'s fields as git's index records them. The casts are no-ops on some targets.
#[allow(clippy::unnecessary_cast)]
fn file_stat(stat: &libc::stat) -> super::FileStat {
    super::FileStat {
        ctime: (stat.st_ctime as i64, stat.st_ctime_nsec as u32),
        mtime: (stat.st_mtime as i64, stat.st_mtime_nsec as u32),
        dev: stat.st_dev as u64,
        ino: stat.st_ino as u64,
        uid: stat.st_uid as u32,
        gid: stat.st_gid as u32,
        size: stat.st_size as u64,
    }
}

/// See `worktree_guard::remove_and_prune_beneath`.
pub(super) fn remove_and_prune(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<()> {
    let Walk::Found(at) = walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step)? else {
        return Ok(());
    };
    if let Some(stat) = stat_at(at.parent(), at.leaf())? {
        match unlink_at(at.parent(), at.leaf(), is_dir(&stat)) {
            Ok(()) => {}
            Err(error) if errno(&error) == libc::ENOENT => {}
            Err(error) => return Err(error),
        }
    }
    // Upwards through the held folders: each is removed by name in its own held parent, only
    // while empty (`AT_REMOVEDIR`), and never the work tree root.
    for i in (1..at.dirs.len()).rev() {
        if unlink_at(at.dirs[i - 1].as_raw_fd(), &at.names[i - 1], true).is_err() {
            break;
        }
    }
    Ok(())
}

/// See `worktree_guard::replace_beneath`.
pub(super) fn replace(
    workdir: &Path,
    rel: &Path,
    content: Content<'_>,
    step: StepFn<'_>,
) -> io::Result<()> {
    let at = match walk(workdir, rel, OnLink::Clear, OnMissing::Create, step)? {
        Walk::Found(at) => at,
        Walk::NotDir => return Err(io::Error::from_raw_os_error(libc::ENOTDIR)),
        // Not returned with `OnMissing::Create`.
        Walk::Link | Walk::Missing => return Err(io::Error::from_raw_os_error(libc::ENOENT)),
    };
    if let Some(stat) = stat_at(at.parent(), at.leaf())? {
        match unlink_at(at.parent(), at.leaf(), is_dir(&stat)) {
            Ok(()) => {}
            Err(error) if errno(&error) == libc::ENOENT => {}
            Err(error) => return Err(error),
        }
    }
    match content {
        Content::Link(target) => symlink_at(target, at.parent(), at.leaf()),
        Content::File { bytes, executable } => {
            let fd = open_at(
                at.parent(),
                at.leaf(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW,
                0o666,
            )?;
            let mode = fstat(fd.as_raw_fd())?.st_mode & 0o7777;
            let wanted = if executable {
                mode | 0o111
            } else {
                mode & !0o111
            };
            if wanted != mode {
                // SAFETY: `fd` is the descriptor just opened above and still owned here.
                check(unsafe { libc::fchmod(fd.as_raw_fd(), wanted) })?;
            }
            File::from(fd).write_all(bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use tempfile::TempDir;

    fn tree() -> (TempDir, TempDir) {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("d/e")).unwrap();
        std::fs::write(dir.path().join("d/f"), "inner\n").unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("f"), "OUTSIDE\n").unwrap();
        (dir, outside)
    }

    fn assert_outside_untouched(outside: &Path) {
        let names: Vec<_> = std::fs::read_dir(outside)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["f"]);
        assert_eq!(
            std::fs::read_to_string(outside.join("f")).unwrap(),
            "OUTSIDE\n"
        );
    }

    fn walk_to(
        dir: &Path,
        rel: &str,
        on_link: OnLink,
        on_missing: OnMissing,
        step: StepFn<'_>,
    ) -> Walk<Beneath> {
        walk(dir, Path::new(rel), on_link, on_missing, step).unwrap()
    }

    #[test]
    fn parent_beneath_refuses_a_link_on_the_way() {
        let (dir, outside) = tree();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("l")).unwrap();
        let walked = walk_to(
            dir.path(),
            "l/f",
            OnLink::Refuse,
            OnMissing::Stop,
            &mut no_step,
        );
        assert!(matches!(walked, Walk::Link));
        assert!(std::fs::symlink_metadata(dir.path().join("l")).is_ok());
    }

    #[test]
    fn parent_beneath_creates_missing_folders() {
        let (dir, _outside) = tree();
        let mut steps = Vec::new();
        let walked = walk_to(
            dir.path(),
            "n/m/x",
            OnLink::Refuse,
            OnMissing::Create,
            &mut |step| steps.push(format!("{step:?}")),
        );
        assert!(matches!(walked, Walk::Found(_)));
        assert!(dir.path().join("n/m").is_dir());
        assert_eq!(
            steps,
            [
                "Created(\"n\")",
                "Opened(\"n\")",
                "Created(\"m\")",
                "Opened(\"m\")"
            ]
        );
        assert!(matches!(
            walk_to(
                dir.path(),
                "q/x",
                OnLink::Refuse,
                OnMissing::Stop,
                &mut no_step
            ),
            Walk::Missing
        ));
        std::fs::write(dir.path().join("file"), "").unwrap();
        assert!(matches!(
            walk_to(
                dir.path(),
                "file/x",
                OnLink::Refuse,
                OnMissing::Create,
                &mut no_step
            ),
            Walk::NotDir
        ));
    }

    #[test]
    fn parent_beneath_clears_a_link_on_the_way() {
        let (dir, outside) = tree();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("l")).unwrap();
        let walked = walk_to(
            dir.path(),
            "l/f",
            OnLink::Clear,
            OnMissing::Create,
            &mut no_step,
        );
        assert!(matches!(walked, Walk::Found(_)));
        let meta = std::fs::symlink_metadata(dir.path().join("l")).unwrap();
        assert!(meta.is_dir() && !meta.file_type().is_symlink());
        assert_outside_untouched(outside.path());
    }

    #[test]
    fn a_folder_swapped_for_a_link_after_it_was_opened_is_not_followed() {
        let (dir, outside) = tree();
        let root = dir.path().to_path_buf();
        let out = outside.path().to_path_buf();
        let walked = walk_to(
            dir.path(),
            "d/f",
            OnLink::Refuse,
            OnMissing::Stop,
            &mut |step| {
                if step == Step::Opened(OsStr::new("d")) {
                    std::fs::rename(root.join("d"), root.join("d-moved")).unwrap();
                    std::os::unix::fs::symlink(&out, root.join("d")).unwrap();
                }
            },
        );
        let Walk::Found(at) = walked else {
            panic!("walk refused")
        };
        let held = fstat(at.parent_fd()).unwrap();
        let moved = std::fs::metadata(dir.path().join("d-moved")).unwrap();
        assert_eq!(
            (held.st_dev as u64, held.st_ino as u64),
            (moved.dev(), moved.ino())
        );
    }

    /// A step that, once `name` is held, renames it to `<name>-moved` and puts a link to
    /// `outside` in its place.
    fn swap_on<'a>(root: &'a Path, name: &'a str, outside: &'a Path) -> impl FnMut(Step<'_>) + 'a {
        move |step| {
            if step == Step::Opened(OsStr::new(name)) {
                let full = root.join(name);
                std::fs::rename(&full, root.join(format!("{name}-moved"))).unwrap();
                std::os::unix::fs::symlink(outside, &full).unwrap();
            }
        }
    }

    fn mkfifo(path: &Path) {
        let name = CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: `name` is a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    }

    /// Runs `f` on another thread and fails (instead of hanging) after 5 s.
    fn within_5s<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("blocked")
    }

    fn mode(path: &Path) -> u32 {
        std::fs::symlink_metadata(path).unwrap().mode() & 0o7777
    }

    #[test]
    fn write_lands_in_the_opened_folder_when_it_is_swapped_mid_write() {
        let (dir, outside) = tree();
        let wrote = write(
            dir.path(),
            Path::new("d/f"),
            b"new",
            &mut swap_on(dir.path(), "d", outside.path()),
        )
        .unwrap();
        assert!(wrote);
        assert_eq!(std::fs::read(dir.path().join("d-moved/f")).unwrap(), b"new");
        assert_outside_untouched(outside.path());
    }

    #[test]
    fn remove_does_not_follow_a_folder_swapped_mid_walk() {
        let (dir, outside) = tree();
        let removed = remove(
            dir.path(),
            Path::new("d/f"),
            &mut swap_on(dir.path(), "d", outside.path()),
        )
        .unwrap();
        assert!(removed);
        assert!(!dir.path().join("d-moved/f").exists());
        assert_outside_untouched(outside.path());
    }

    #[test]
    fn write_keeps_the_exec_bit_of_an_existing_file() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, _outside) = tree();
        let f = dir.path().join("d/f");
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(write(dir.path(), Path::new("d/f"), b"x", &mut no_step).unwrap());
        assert_eq!(mode(&f), 0o755);
        assert_eq!(std::fs::read(&f).unwrap(), b"x");
    }

    #[test]
    fn write_on_a_fifo_errors_without_blocking() {
        let (dir, _outside) = tree();
        mkfifo(&dir.path().join("pipe"));
        let root = dir.path().to_path_buf();
        let result = within_5s(move || write(&root, Path::new("pipe"), b"x", &mut no_step));
        assert!(result.is_err());
    }

    #[test]
    fn remove_returns_false_for_a_leaf_that_became_a_folder() {
        let (dir, _outside) = tree();
        let root = dir.path().to_path_buf();
        let removed = remove(dir.path(), Path::new("d/f"), &mut |step| {
            if step == Step::Opened(OsStr::new("d")) {
                std::fs::remove_file(root.join("d/f")).unwrap();
                std::fs::create_dir(root.join("d/f")).unwrap();
            }
        })
        .unwrap();
        assert!(!removed);
        assert!(dir.path().join("d/f").is_dir());
    }

    #[test]
    fn clear_hazards_removes_a_parent_that_became_a_link_instead_of_following_it() {
        let (dir, outside) = tree();
        std::fs::create_dir_all(dir.path().join("a/d")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("f"), dir.path().join("a/d/f")).unwrap();
        let root = dir.path().to_path_buf();
        let out = outside.path().to_path_buf();
        clear_hazard(dir.path(), Path::new("a/d/f"), &mut |step| {
            if step == Step::Opened(OsStr::new("a")) {
                std::fs::rename(root.join("a/d"), root.join("a/d-moved")).unwrap();
                std::os::unix::fs::symlink(&out, root.join("a/d")).unwrap();
            }
        })
        .unwrap();
        assert!(std::fs::symlink_metadata(dir.path().join("a/d")).is_err());
        assert_outside_untouched(outside.path());
    }

    #[test]
    fn clear_hazards_leaves_a_leaf_that_became_a_folder() {
        let (dir, outside) = tree();
        std::os::unix::fs::symlink(outside.path().join("f"), dir.path().join("d/tc")).unwrap();
        let root = dir.path().to_path_buf();
        clear_hazard(dir.path(), Path::new("d/tc"), &mut |step| {
            if step == Step::Opened(OsStr::new("d")) {
                std::fs::remove_file(root.join("d/tc")).unwrap();
                std::fs::create_dir(root.join("d/tc")).unwrap();
                std::fs::write(root.join("d/tc/keep"), "k").unwrap();
            }
        })
        .unwrap();
        assert!(dir.path().join("d/tc/keep").is_file());
        assert_outside_untouched(outside.path());
    }

    #[test]
    fn read_does_not_follow_a_swapped_folder() {
        let (dir, outside) = tree();
        let leaf = read(
            dir.path(),
            Path::new("d/f"),
            &mut swap_on(dir.path(), "d", outside.path()),
        )
        .unwrap();
        assert!(
            matches!(
                &leaf,
                Some(Leaf::File {
                    bytes,
                    executable: false,
                    ..
                }) if bytes == b"inner\n"
            ),
            "{leaf:?}"
        );
    }

    #[test]
    fn read_returns_none_for_a_fifo_without_blocking() {
        let (dir, _outside) = tree();
        mkfifo(&dir.path().join("pipe"));
        let root = dir.path().to_path_buf();
        let leaf = within_5s(move || read(&root, Path::new("pipe"), &mut no_step).unwrap());
        assert_eq!(leaf, None);
    }

    #[test]
    fn read_reads_a_link_target_and_the_exec_bit() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, outside) = tree();
        std::os::unix::fs::symlink("../elsewhere", dir.path().join("d/l")).unwrap();
        assert_eq!(
            read(dir.path(), Path::new("d/l"), &mut no_step).unwrap(),
            Some(Leaf::Link(b"../elsewhere".to_vec()))
        );
        std::fs::set_permissions(
            dir.path().join("d/f"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        assert!(matches!(
            read(dir.path(), Path::new("d/f"), &mut no_step).unwrap(),
            Some(Leaf::File {
                executable: true,
                ..
            })
        ));
        assert_eq!(
            read(dir.path(), Path::new("d/e"), &mut no_step).unwrap(),
            None
        );
        std::os::unix::fs::symlink(outside.path(), dir.path().join("o")).unwrap();
        assert_eq!(
            read(dir.path(), Path::new("o/f"), &mut no_step).unwrap(),
            None
        );
    }

    #[test]
    fn replace_writes_into_the_opened_folder_when_swapped() {
        let (dir, outside) = tree();
        replace(
            dir.path(),
            Path::new("d/f"),
            Content::File {
                bytes: b"new",
                executable: false,
            },
            &mut swap_on(dir.path(), "d", outside.path()),
        )
        .unwrap();
        assert_eq!(std::fs::read(dir.path().join("d-moved/f")).unwrap(), b"new");
        assert_outside_untouched(outside.path());
    }

    #[test]
    fn replace_clears_a_linked_folder_on_the_way() {
        let (dir, outside) = tree();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("l")).unwrap();
        replace(
            dir.path(),
            Path::new("l/f"),
            Content::File {
                bytes: b"new",
                executable: false,
            },
            &mut no_step,
        )
        .unwrap();
        assert_eq!(std::fs::read(dir.path().join("l/f")).unwrap(), b"new");
        assert_outside_untouched(outside.path());
    }

    #[test]
    fn replace_sets_and_clears_the_exec_bit() {
        let (dir, _outside) = tree();
        let f = dir.path().join("d/f");
        let file = |executable| Content::File {
            bytes: b"x",
            executable,
        };
        replace(dir.path(), Path::new("d/f"), file(true), &mut no_step).unwrap();
        assert_eq!(mode(&f) & 0o111, 0o111);
        replace(dir.path(), Path::new("d/f"), file(false), &mut no_step).unwrap();
        assert_eq!(mode(&f) & 0o111, 0);
        assert_eq!(std::fs::read(&f).unwrap(), b"x");
    }

    #[test]
    fn replace_creates_a_symlink_leaf_and_replaces_an_empty_folder() {
        let (dir, _outside) = tree();
        replace(
            dir.path(),
            Path::new("d/e"),
            Content::Link(b"f"),
            &mut no_step,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_link(dir.path().join("d/e")).unwrap(),
            Path::new("f")
        );
    }

    #[test]
    fn remove_and_prune_removes_emptied_parents_only_inside() {
        let (dir, _outside) = tree();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("a/b/c"), "c").unwrap();
        remove_and_prune(dir.path(), Path::new("a/b/c"), &mut no_step).unwrap();
        assert!(!dir.path().join("a").exists());
        assert!(dir.path().join("d/f").exists());

        // A work tree that empties is kept.
        let lone = TempDir::new().unwrap();
        std::fs::create_dir(lone.path().join("x")).unwrap();
        std::fs::write(lone.path().join("x/y"), "y").unwrap();
        remove_and_prune(lone.path(), Path::new("x/y"), &mut no_step).unwrap();
        assert!(lone.path().is_dir());
        assert!(!lone.path().join("x").exists());
    }

    #[test]
    fn remove_and_prune_does_not_act_beyond_a_swapped_folder() {
        let (dir, outside) = tree();
        remove_and_prune(
            dir.path(),
            Path::new("d/f"),
            &mut swap_on(dir.path(), "d", outside.path()),
        )
        .unwrap();
        assert!(!dir.path().join("d-moved/f").exists());
        assert_outside_untouched(outside.path());
    }

    /// Stress test for #149: a thread swaps `rd` between a real folder and a link to an outside
    /// folder while `write` writes `rd/f`. Run manually:
    /// `cargo test -p git-core write_swap_race -- --ignored --nocapture`.
    #[test]
    #[ignore = "stress: run manually"]
    fn write_swap_race_never_writes_outside() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let (dir, outside) = tree();
        let rd = dir.path().join("rd");
        let real = dir.path().join("rd-real");
        std::fs::create_dir(&real).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let swapper = {
            let (stop, rd, real, out) = (
                stop.clone(),
                rd.clone(),
                real.clone(),
                outside.path().to_path_buf(),
            );
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = std::fs::rename(&real, &rd);
                    let _ = std::fs::rename(&rd, &real);
                    let _ = std::os::unix::fs::symlink(&out, &rd);
                    let _ = std::fs::remove_file(&rd);
                }
            })
        };
        let mut written = 0;
        for _ in 0..200_000 {
            if let Ok(true) = write(dir.path(), Path::new("rd/f"), b"INSIDE\n", &mut no_step) {
                written += 1;
            }
        }
        stop.store(true, Ordering::Relaxed);
        swapper.join().unwrap();
        let outside_f = std::fs::read_to_string(outside.path().join("f")).unwrap();
        eprintln!("{written} writes inside, outside f = {outside_f:?}");
        assert_eq!(outside_f, "OUTSIDE\n");
    }
}
