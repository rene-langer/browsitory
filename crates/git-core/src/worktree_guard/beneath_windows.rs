//! Race-free raw work-tree I/O on Windows (#149, #150).
//!
//! Win32 has no `openat`. Instead every folder on the way is opened by full path, *as itself*
//! (`FILE_FLAG_OPEN_REPARSE_POINT`, so a link is never followed and its target, a network share
//! included, never contacted), and held open without `FILE_SHARE_DELETE` until the act is done.
//! A folder held that way can't be renamed, deleted or replaced (all need `DELETE` access, which
//! the share mode denies), so once components `0..k` are held, the full path of component `k+1`
//! resolves through exactly those folders. A process that already holds `DELETE` access makes
//! the open fail with a sharing violation: the walk fails closed.
//!
//! Each component's attributes and reparse tag come from its own handle
//! (`GetFileInformationByHandleEx`): a name surrogate (symbolic link, junction, WSL link, …) is
//! refused or removed itself; another reparse point (a OneDrive placeholder) is a folder to
//! walk through or a file to reopen without the flag, which must have the same file ID.
//! Deleting acts by path under held parents: `DeleteFileW`/`RemoveDirectoryW` never follow a
//! final reparse point.

use std::ffi::{c_void, OsString};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

use super::reparse::{leaf_action, LeafAction, FILE_ATTRIBUTE_DIRECTORY};
use super::{Content, Leaf, OnLink, OnMissing, Step, StepFn, Walk};

const FILE_LIST_DIRECTORY: u32 = 0x0001;
const FILE_READ_ATTRIBUTES: u32 = 0x0080;
const SYNCHRONIZE: u32 = 0x0010_0000;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ: u32 = 0x1;
const FILE_SHARE_WRITE: u32 = 0x2;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_PATH_NOT_FOUND: i32 = 3;
const ERROR_INVALID_NAME: i32 = 123;
const ERROR_ALREADY_EXISTS: i32 = 183;
const ERROR_DIRECTORY: i32 = 267;

/// `FILE_INFO_BY_HANDLE_CLASS` values.
const FILE_ATTRIBUTE_TAG_INFO_CLASS: i32 = 9;
const FILE_ID_INFO_CLASS: i32 = 18;

/// `FILE_ATTRIBUTE_TAG_INFO`.
#[repr(C)]
#[derive(Default)]
struct AttributeTagInfo {
    file_attributes: u32,
    reparse_tag: u32,
}

/// `FILE_ID_INFO`: a 64-bit volume serial number and a 128-bit file ID.
#[repr(C)]
#[derive(Default, PartialEq, Eq)]
struct FileIdInfo {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

#[link(name = "kernel32")]
extern "system" {
    fn GetFileInformationByHandleEx(
        file: *mut c_void,
        class: i32,
        info: *mut c_void,
        size: u32,
    ) -> i32;
    fn GetFinalPathNameByHandleW(file: *mut c_void, path: *mut u16, len: u32, flags: u32) -> u32;
}

fn attribute_tag(file: &File) -> io::Result<AttributeTagInfo> {
    let mut info = AttributeTagInfo::default();
    // SAFETY: the handle belongs to `file`, which outlives the call; `info` is a writable
    // `FILE_ATTRIBUTE_TAG_INFO` (two u32s, `repr(C)`) of exactly the size passed.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FILE_ATTRIBUTE_TAG_INFO_CLASS,
            (&mut info as *mut AttributeTagInfo).cast(),
            std::mem::size_of::<AttributeTagInfo>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

fn file_id(file: &File) -> io::Result<FileIdInfo> {
    let mut info = FileIdInfo::default();
    // SAFETY: as in `attribute_tag`; `info` is a writable `FILE_ID_INFO` (u64 + 16 bytes,
    // `repr(C)`) of exactly the size passed.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FILE_ID_INFO_CLASS,
            (&mut info as *mut FileIdInfo).cast(),
            std::mem::size_of::<FileIdInfo>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

/// The normalized `\\?\` DOS path of an open handle (`GetFinalPathNameByHandleW`), the same
/// resolver `std::fs::canonicalize` uses.
fn final_path(file: &File) -> io::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
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
            return Ok(PathBuf::from(OsString::from_wide(&buf)));
        }
        // Too small: `len` is the size needed, including the terminating NUL.
        buf.resize(len, 0);
    }
}

/// Opens `path` as itself (a link is never followed), held against rename and delete.
fn open_pinned(path: &Path, access: u32) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(access | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
}

fn missing(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND)
    )
}

/// The folders from the work tree root down to the leaf's parent, each held open.
pub(super) struct Beneath {
    /// The canonical work tree (`\\?\` path, links resolved: the user chose it).
    root: PathBuf,
    /// `dirs[0]` is the work tree root; `dirs[i + 1]` is `names[..=i]` below it.
    dirs: Vec<File>,
    /// Every component of the path, the leaf last (`names.len() == dirs.len()`).
    names: Vec<OsString>,
}

impl Beneath {
    /// The full path of the first `n` components.
    fn path_of(&self, n: usize) -> PathBuf {
        let mut path = self.root.clone();
        for name in &self.names[..n] {
            path.push(name);
        }
        path
    }

    fn leaf_path(&self) -> PathBuf {
        self.path_of(self.names.len())
    }

    #[cfg(test)]
    pub(super) fn parent_file(&self) -> &File {
        &self.dirs[self.dirs.len() - 1]
    }
}

/// What stands at a leaf, judged on a handle opened as itself.
enum Found {
    Missing,
    Dir,
    /// A name surrogate; `dir` when it is a folder link (removed with `RemoveDirectoryW`).
    Link {
        dir: bool,
    },
    /// A file, maybe a non-surrogate reparse point (reading reopens it).
    File,
}

/// Classifies the leaf at `path` (its parent held). The handle is closed again, so the leaf can
/// be deleted; whatever replaces it meanwhile is still never followed by the delete.
fn inspect(path: &Path) -> io::Result<Found> {
    let file = match open_pinned(path, 0) {
        Ok(file) => file,
        Err(error) if missing(&error) => return Ok(Found::Missing),
        Err(error) => return Err(error),
    };
    let info = attribute_tag(&file)?;
    let dir = info.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    Ok(match leaf_action(info.file_attributes, info.reparse_tag) {
        LeafAction::Refuse => Found::Link { dir },
        _ if dir => Found::Dir,
        _ => Found::File,
    })
}

/// Removes the link at `path` itself: `RemoveDirectoryW` for a folder link or junction,
/// `DeleteFileW` otherwise. Neither follows the link.
fn remove_link_at(path: &Path, dir: bool) -> io::Result<()> {
    let result = if dir {
        std::fs::remove_dir(path)
    } else {
        std::fs::remove_file(path)
    };
    match result {
        Err(error) if !missing(&error) => Err(error),
        _ => Ok(()),
    }
}

/// How often a walk retries one component that keeps changing under it before giving up.
const ATTEMPTS: usize = 8;

/// Walks `rel` (a plain relative path) below `workdir` down to its last component's parent,
/// holding each folder. See the module docs.
pub(super) fn walk(
    workdir: &Path,
    rel: &Path,
    on_link: OnLink,
    on_missing: OnMissing,
    step: StepFn<'_>,
) -> io::Result<Walk<Beneath>> {
    let names: Vec<OsString> = rel
        .components()
        .map(|c| c.as_os_str().to_os_string())
        .collect();
    if names.is_empty() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let root = std::fs::canonicalize(workdir)?;
    let root_dir = OpenOptions::new()
        .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(&root)?;
    let mut at = Beneath {
        root,
        dirs: vec![root_dir],
        names,
    };
    for k in 0..at.names.len() - 1 {
        let path = at.path_of(k + 1);
        let name = at.names[k].clone();
        let mut next = None;
        for _ in 0..ATTEMPTS {
            let dir = match open_pinned(&path, FILE_LIST_DIRECTORY) {
                Ok(dir) => dir,
                Err(error) if missing(&error) => match on_missing {
                    OnMissing::Stop => return Ok(Walk::Missing),
                    OnMissing::Create => {
                        match std::fs::create_dir(&path) {
                            Ok(()) => step(Step::Created(&name)),
                            Err(error) if error.raw_os_error() == Some(ERROR_ALREADY_EXISTS) => {}
                            Err(error) => return Err(error),
                        }
                        continue;
                    }
                },
                Err(error) => return Err(error),
            };
            let info = attribute_tag(&dir)?;
            match leaf_action(info.file_attributes, info.reparse_tag) {
                LeafAction::Refuse => {
                    let is_dir = info.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
                    drop(dir);
                    match on_link {
                        OnLink::Refuse => return Ok(Walk::Link),
                        OnLink::Clear => {
                            remove_link_at(&path, is_dir)?;
                            step(Step::Cleared(&name));
                            if on_missing == OnMissing::Stop {
                                return Ok(Walk::Link);
                            }
                        }
                    }
                }
                // A plain folder, or one a file system filter resolves (a OneDrive folder).
                _ if info.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 => {
                    step(Step::Opened(&name));
                    next = Some(dir);
                    break;
                }
                _ => return Ok(Walk::NotDir),
            }
        }
        match next {
            Some(dir) => at.dirs.push(dir),
            None => {
                return Err(io::Error::other(
                    "the working tree kept changing while it was being written",
                ))
            }
        }
    }
    Ok(Walk::Found(at))
}

fn unreadable(error: &io::Error) -> bool {
    missing(error)
        || matches!(
            error.kind(),
            io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound
        )
        || matches!(
            error.raw_os_error(),
            Some(ERROR_INVALID_NAME | ERROR_DIRECTORY)
        )
}

/// Opens the regular file at `at`'s leaf for reading: as itself, or (a non-surrogate reparse
/// point, such as an online-only OneDrive file) reopened normally and checked to be the same
/// file. `None` for a link, a folder or anything missing.
fn open_leaf_for_reading(at: &Beneath) -> io::Result<Option<File>> {
    let path = at.leaf_path();
    let first = match OpenOptions::new()
        .access_mode(GENERIC_READ | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if unreadable(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let info = attribute_tag(&first)?;
    if info.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        return Ok(None);
    }
    let file = match leaf_action(info.file_attributes, info.reparse_tag) {
        LeafAction::Plain => first,
        LeafAction::Refuse => return Ok(None),
        LeafAction::Reopen => {
            // `first` stays open without FILE_SHARE_DELETE, so the name can't be swapped for
            // another file before the reopen; the file ID check proves it wasn't.
            let reopened = match OpenOptions::new()
                .access_mode(GENERIC_READ | SYNCHRONIZE)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .open(&path)
            {
                Ok(file) => file,
                Err(error) if unreadable(&error) => return Ok(None),
                Err(error) => return Err(error),
            };
            if file_id(&reopened)? != file_id(&first)? {
                return Ok(None);
            }
            reopened
        }
    };
    // Belt and braces: the opened file resolves below the work tree.
    if !final_path(&file)?.starts_with(&at.root) {
        return Ok(None);
    }
    Ok(Some(file))
}

/// See `worktree_guard::open_beneath`.
pub(super) fn open_file(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<Option<File>> {
    match walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step) {
        Ok(Walk::Found(at)) => open_leaf_for_reading(&at),
        Ok(_) => Ok(None),
        Err(error) if unreadable(&error) => Ok(None),
        Err(error) => Err(error),
    }
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
    let path = at.leaf_path();
    if let Found::Link { dir } = inspect(&path)? {
        remove_link_at(&path, dir)?;
    }
    // Opened as itself, so a link that appears meanwhile is the handle, never its target; it
    // is then refused below instead of written.
    // `write(true)` lets std accept `create`; the access itself is `access_mode`.
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(GENERIC_READ | GENERIC_WRITE | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .create(true)
        .open(&path)?;
    let info = attribute_tag(&file)?;
    if leaf_action(info.file_attributes, info.reparse_tag) == LeafAction::Refuse
        || info.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    file.set_len(0)?;
    file.write_all(bytes)?;
    Ok(true)
}

/// See `worktree_guard::remove_beneath`.
pub(super) fn remove(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<bool> {
    let at = match walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step)? {
        Walk::Found(at) => at,
        Walk::Link => return Ok(false),
        Walk::Missing | Walk::NotDir => return Ok(true),
    };
    let path = at.leaf_path();
    match inspect(&path)? {
        Found::Missing => Ok(true),
        Found::Dir => Ok(false),
        Found::Link { dir } => remove_link_at(&path, dir).map(|()| true),
        // `DeleteFileW` fails on a folder that appeared meanwhile.
        Found::File => match std::fs::remove_file(&path) {
            Err(error) if !missing(&error) => Err(error),
            _ => Ok(true),
        },
    }
}

/// See `beneath_unix::clear_hazard`.
pub(super) fn clear_hazard(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<()> {
    let Walk::Found(at) = walk(workdir, rel, OnLink::Clear, OnMissing::Stop, step)? else {
        return Ok(());
    };
    let path = at.leaf_path();
    match inspect(&path)? {
        Found::Link { dir } => remove_link_at(&path, dir),
        _ => Ok(()),
    }
}

/// See `worktree_guard::read_beneath`.
pub(super) fn read(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<Option<Leaf>> {
    let at = match walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step) {
        Ok(Walk::Found(at)) => at,
        Ok(_) => return Ok(None),
        Err(error) if unreadable(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let path = at.leaf_path();
    match inspect(&path)? {
        Found::Missing | Found::Dir => Ok(None),
        // `read_link` opens the link as itself (FSCTL_GET_REPARSE_POINT): never followed.
        Found::Link { .. } => match std::fs::read_link(&path) {
            Ok(target) => Ok(Some(Leaf::Link(
                target.to_string_lossy().replace('\\', "/").into_bytes(),
            ))),
            Err(_) => Ok(None),
        },
        Found::File => {
            let Some(mut file) = open_leaf_for_reading(&at)? else {
                return Ok(None);
            };
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            Ok(Some(Leaf::File {
                bytes,
                executable: false,
            }))
        }
    }
}

/// See `worktree_guard::remove_and_prune_beneath`.
pub(super) fn remove_and_prune(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<()> {
    let Walk::Found(mut at) = walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step)? else {
        return Ok(());
    };
    let path = at.leaf_path();
    match inspect(&path)? {
        Found::Missing => {}
        Found::Dir => std::fs::remove_dir(&path)?,
        Found::Link { dir } => remove_link_at(&path, dir)?,
        Found::File => match std::fs::remove_file(&path) {
            Err(error) if !missing(&error) => return Err(error),
            _ => {}
        },
    }
    // Upwards: each folder's own handle is closed (a held folder can't be removed), while its
    // parents stay held, then it is removed by path only while empty. Never the work tree root.
    for i in (1..at.dirs.len()).rev() {
        at.dirs.pop();
        if std::fs::remove_dir(at.path_of(i)).is_err() {
            break;
        }
    }
    Ok(())
}

/// See `worktree_guard::replace_beneath`. Links are written as plain files holding the target,
/// as before.
pub(super) fn replace(
    workdir: &Path,
    rel: &Path,
    content: Content<'_>,
    step: StepFn<'_>,
) -> io::Result<()> {
    let at = match walk(workdir, rel, OnLink::Clear, OnMissing::Create, step)? {
        Walk::Found(at) => at,
        Walk::NotDir => {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "a file is in the way",
            ))
        }
        Walk::Link | Walk::Missing => return Err(io::Error::from(io::ErrorKind::NotFound)),
    };
    let path = at.leaf_path();
    match inspect(&path)? {
        Found::Missing => {}
        Found::Dir => std::fs::remove_dir(&path)?,
        Found::Link { dir } => remove_link_at(&path, dir)?,
        Found::File => std::fs::remove_file(&path)?,
    }
    let bytes = match content {
        Content::File { bytes, .. } | Content::Link(bytes) => bytes,
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&path)?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worktree_guard::no_step;
    use std::ffi::OsStr;
    use tempfile::TempDir;

    /// A junction `link` -> `target` (no privilege needed).
    pub(crate) fn junction(link: &Path, target: &Path) {
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");
    }

    #[test]
    fn a_held_folder_cannot_be_renamed_or_replaced() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::fs::write(dir.path().join("d/f"), "inner").unwrap();
        let root = dir.path().to_path_buf();
        let mut rename = None;
        let walked = walk(
            dir.path(),
            Path::new("d/f"),
            OnLink::Refuse,
            OnMissing::Stop,
            &mut |step| {
                if step == Step::Opened(OsStr::new("d")) {
                    rename = Some(std::fs::rename(root.join("d"), root.join("d-moved")));
                }
            },
        )
        .unwrap();
        assert!(matches!(walked, Walk::Found(_)));
        assert!(rename.unwrap().is_err(), "a held folder was renamed");
        drop(walked);
        // Released after the walk.
        std::fs::rename(dir.path().join("d"), dir.path().join("d-moved")).unwrap();
    }

    #[test]
    fn the_walk_refuses_and_clears_a_junction_on_the_way() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("f"), "OUTSIDE").unwrap();
        junction(&dir.path().join("j"), outside.path());
        assert!(matches!(
            walk(
                dir.path(),
                Path::new("j/f"),
                OnLink::Refuse,
                OnMissing::Stop,
                &mut no_step
            )
            .unwrap(),
            Walk::Link
        ));
        let walked = walk(
            dir.path(),
            Path::new("j/f"),
            OnLink::Clear,
            OnMissing::Create,
            &mut no_step,
        )
        .unwrap();
        let Walk::Found(at) = walked else {
            panic!("not found")
        };
        assert!(attribute_tag(at.parent_file()).unwrap().file_attributes & 0x400 == 0);
        assert_eq!(
            std::fs::read_to_string(outside.path().join("f")).unwrap(),
            "OUTSIDE"
        );
    }

    #[test]
    fn file_id_info_has_the_documented_layout() {
        assert_eq!(std::mem::size_of::<AttributeTagInfo>(), 8);
        assert_eq!(std::mem::size_of::<FileIdInfo>(), 24);
        assert_eq!(std::mem::align_of::<FileIdInfo>(), 8);
    }
}
