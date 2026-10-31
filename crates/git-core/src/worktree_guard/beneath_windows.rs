//! Race-free raw work-tree I/O on Windows (#149, #150).
//!
//! Win32 has no `openat`. Instead every folder on the way is opened by full path, *as itself*
//! (`FILE_FLAG_OPEN_REPARSE_POINT`, so a link is never followed and its target, a network share
//! included, never contacted), and held open without `FILE_SHARE_DELETE` until the act is done.
//! A folder held that way can't be renamed, deleted or replaced (all need `DELETE` access, which
//! the share mode denies). A process that already holds `DELETE` access makes the open fail with
//! a sharing violation: the walk fails closed.
//!
//! A held folder that is *empty* can still be turned into a junction in place
//! (`FSCTL_SET_REPARSE_POINT` needs no `DELETE`), and `FILE_FLAG_OPEN_REPARSE_POINT` only
//! protects the last component of a path. So every component and every leaf opened by path is
//! checked on its own handle to sit directly in the held parent (`GetFinalPathNameByHandleW` of
//! both); a mismatch fails the act, and a file created by the failed act is deleted through its
//! handle. Leaves are deleted through their handle (`FileDispositionInfoEx` with POSIX
//! semantics, see `delete_by_handle`), never by path, and an existing file is written through a
//! handle whose file ID matches the one classified.
//!
//! Each component's attributes and reparse tag come from its own handle
//! (`GetFileInformationByHandleEx`): a name surrogate (symbolic link, junction, WSL link, …) is
//! refused or removed itself; another reparse point (a OneDrive placeholder) is a folder to
//! walk through or a file reopened without the flag (so the file system filter sees the read or
//! write), which must have the same file ID.
//!
//! A missing folder is created relative to the held parent's handle (`NtCreateFile` with
//! `RootDirectory`, see `create_dir_in`), not by path, so on a local NTFS or ReFS volume a
//! parent turned into a junction in that instant can't get a folder created in the junction's
//! target (#163). On an SMB share the redirector resolves a relative open by path on the server,
//! so there this narrows the race but doesn't close it. The new folder is then opened by path
//! and checked like any other component.

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
const FILE_SHARE_DELETE: u32 = 0x4;
const DELETE: u32 = 0x0001_0000;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_PATH_NOT_FOUND: i32 = 3;
const ERROR_INVALID_NAME: i32 = 123;
const ERROR_FILE_EXISTS: i32 = 80;
const ERROR_ALREADY_EXISTS: i32 = 183;
const ERROR_DIRECTORY: i32 = 267;

/// `FILE_INFO_BY_HANDLE_CLASS` values.
const FILE_ATTRIBUTE_TAG_INFO_CLASS: i32 = 9;
const FILE_ID_INFO_CLASS: i32 = 18;
const FILE_DISPOSITION_INFO_CLASS: i32 = 4;
const FILE_DISPOSITION_INFO_EX_CLASS: i32 = 21;

/// `FILE_DISPOSITION_INFO_EX` flags.
const FILE_DISPOSITION_FLAG_DELETE: u32 = 0x1;
const FILE_DISPOSITION_FLAG_POSIX_SEMANTICS: u32 = 0x2;

/// What a file system without POSIX deletes answers (see `delete_by_handle`).
const ERROR_INVALID_FUNCTION: i32 = 1;
const ERROR_NOT_SUPPORTED: i32 = 50;
const ERROR_INVALID_PARAMETER: i32 = 87;

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

/// `FILE_DISPOSITION_INFO`: a single `BOOLEAN`.
#[repr(C)]
struct DispositionInfo {
    delete_file: u8,
}

/// `FILE_DISPOSITION_INFO_EX`: a `ULONG` of `FILE_DISPOSITION_FLAG_*`.
#[repr(C)]
struct DispositionInfoEx {
    flags: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn SetFileInformationByHandle(
        file: *mut c_void,
        class: i32,
        info: *mut c_void,
        size: u32,
    ) -> i32;
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

/// Marks the file or folder `file` refers to for deletion. Acts on the object itself, never on a
/// name, never following a link; a folder must be empty. The handle needs `DELETE` access.
///
/// POSIX semantics (#168): the name leaves its folder as soon as *our* handle closes, even while
/// another program (an antivirus scanner, the search indexer) keeps the file open with delete
/// sharing, so a file can be created under that name at once and the folder can be removed.
/// Where the file system doesn't support that (FAT/exFAT, older Windows, some network shares),
/// it falls back to the classic disposition, as std's `remove_dir_all` does: the name then stays
/// until the last handle closes.
fn delete_by_handle(file: &File) -> io::Result<()> {
    let mut posix = DispositionInfoEx {
        flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
    };
    match set_information(file, FILE_DISPOSITION_INFO_EX_CLASS, &mut posix) {
        Err(error)
            if matches!(
                error.raw_os_error(),
                Some(ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED | ERROR_INVALID_PARAMETER)
            ) =>
        {
            set_information(
                file,
                FILE_DISPOSITION_INFO_CLASS,
                &mut DispositionInfo { delete_file: 1 },
            )
        }
        other => other,
    }
}

/// `SetFileInformationByHandle` with `info` as the `class` structure.
fn set_information<T>(file: &File, class: i32, info: &mut T) -> io::Result<()> {
    // SAFETY: the handle belongs to `file`, which outlives the call; `info` is a writable,
    // `repr(C)` structure of exactly the size passed, and each caller pairs it with its class.
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            class,
            (info as *mut T).cast(),
            std::mem::size_of::<T>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// `UNICODE_STRING`: lengths in bytes, no terminating NUL.
#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

/// `OBJECT_ATTRIBUTES`.
#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: *mut c_void,
    object_name: *mut UnicodeString,
    attributes: u32,
    security_descriptor: *mut c_void,
    security_quality_of_service: *mut c_void,
}

/// `IO_STATUS_BLOCK`: an `NTSTATUS`/pointer union and a `ULONG_PTR`.
#[repr(C)]
struct IoStatusBlock {
    status: usize,
    information: usize,
}

const OBJ_CASE_INSENSITIVE: u32 = 0x40;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
/// `NtCreateFile` create disposition: fail if the name exists.
const FILE_CREATE: u32 = 2;
/// `NtCreateFile` create options.
const FILE_DIRECTORY_FILE: u32 = 0x0000_0001;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;
const FILE_OPEN_FOR_BACKUP_INTENT: u32 = 0x0000_4000;
const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

#[link(name = "ntdll")]
extern "system" {
    fn NtCreateFile(
        handle: *mut *mut c_void,
        access: u32,
        attributes: *mut ObjectAttributes,
        io_status: *mut IoStatusBlock,
        allocation_size: *mut i64,
        file_attributes: u32,
        share_access: u32,
        create_disposition: u32,
        create_options: u32,
        ea_buffer: *mut c_void,
        ea_length: u32,
    ) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
}

/// Creates the folder `name` directly in the held folder `parent`, resolved against `parent`'s
/// handle rather than a path (#163): on a local NTFS/ReFS volume a parent turned into a junction
/// meanwhile is never followed (an SMB share resolves it by path, see the module docs). Only
/// `FILE_READ_ATTRIBUTES | SYNCHRONIZE` is asked for: the handle is closed at once. An existing name fails with `ERROR_ALREADY_EXISTS`, as `CreateDirectoryW` does.
fn create_dir_in(parent: &File, name: &std::ffi::OsStr) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{FromRawHandle, OwnedHandle};

    let mut wide: Vec<u16> = name.encode_wide().collect();
    let length = u16::try_from(wide.len() * 2)
        .ok()
        .filter(|&length| length > 0)
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let mut object_name = UnicodeString {
        length,
        maximum_length: length,
        buffer: wide.as_mut_ptr(),
    };
    let mut attributes = ObjectAttributes {
        length: std::mem::size_of::<ObjectAttributes>() as u32,
        root_directory: parent.as_raw_handle(),
        object_name: &mut object_name,
        attributes: OBJ_CASE_INSENSITIVE,
        security_descriptor: std::ptr::null_mut(),
        security_quality_of_service: std::ptr::null_mut(),
    };
    let mut io_status = IoStatusBlock {
        status: 0,
        information: 0,
    };
    let mut handle = std::ptr::null_mut();
    // SAFETY: every pointer is to a live local of the documented layout: `handle` receives the
    // new handle, `attributes` names `wide` (which outlives the call) relative to `parent`'s
    // handle (owned by `parent`, which outlives the call), `io_status` is writable. No
    // allocation size and no extended attributes (null, 0).
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            &mut attributes,
            &mut io_status,
            std::ptr::null_mut(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE
                | FILE_SYNCHRONOUS_IO_NONALERT
                | FILE_OPEN_FOR_BACKUP_INTENT
                | FILE_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
            0,
        )
    };
    if status < 0 {
        // SAFETY: a pure conversion of a status code.
        let code = unsafe { RtlNtStatusToDosError(status) };
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    // SAFETY: on success `handle` is a new, open handle that nothing else owns; it is closed
    // exactly once, here.
    drop(unsafe { OwnedHandle::from_raw_handle(handle) });
    Ok(())
}

/// True when `child` (opened by path) sits directly in the held folder `parent`: the path
/// didn't go through a held folder that became a junction meanwhile.
fn in_parent(child: &File, parent: &File) -> io::Result<bool> {
    Ok(final_path(child)?.parent() == Some(final_path(parent)?.as_path()))
}

fn redirected() -> io::Error {
    io::Error::other("a folder of the working tree changed while it was being written")
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Found {
    Dir,
    /// A name surrogate (symbolic link, junction, …).
    Link,
    /// A file; `reopen` for a non-surrogate reparse point (OneDrive), which is read and written
    /// through a second, normal handle.
    File {
        reopen: bool,
    },
}

/// Opens `path` (a child of `at`'s last held folder) as itself with `DELETE` access, checks it
/// is in that folder, and classifies it. `None` when missing. Shares delete: the handle is
/// what any later act uses, so a rename of the name meanwhile changes nothing.
fn open_leaf_at(at: &Beneath, path: &Path) -> io::Result<Option<(File, Found)>> {
    let file = match OpenOptions::new()
        .access_mode(DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if missing(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    if !in_parent(&file, &at.dirs[at.dirs.len() - 1])? {
        return Err(redirected());
    }
    let info = attribute_tag(&file)?;
    let found = match leaf_action(info.file_attributes, info.reparse_tag) {
        LeafAction::Refuse => Found::Link,
        _ if info.file_attributes & FILE_ATTRIBUTE_DIRECTORY != 0 => Found::Dir,
        action => Found::File {
            reopen: action == LeafAction::Reopen,
        },
    };
    Ok(Some((file, found)))
}

fn open_leaf(at: &Beneath) -> io::Result<Option<(File, Found)>> {
    open_leaf_at(at, &at.leaf_path())
}

/// Opens `at`'s leaf a second time, normally (through the file system filter for a OneDrive
/// file) or as itself, and checks it is the file `first` refers to.
fn reopen_same(at: &Beneath, first: &File, reopen: bool, access: u32) -> io::Result<Option<File>> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(access & GENERIC_WRITE != 0)
        .access_mode(access | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    if !reopen {
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let second = match options.open(at.leaf_path()) {
        Ok(file) => file,
        Err(error) if missing(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok((file_id(&second)? == file_id(first)?).then_some(second))
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
                        match create_dir_in(&at.dirs[at.dirs.len() - 1], &name) {
                            Ok(()) => step(Step::Created(&name)),
                            Err(error) if error.raw_os_error() == Some(ERROR_ALREADY_EXISTS) => {}
                            Err(error) => return Err(error),
                        }
                        continue;
                    }
                },
                Err(error) => return Err(error),
            };
            if !in_parent(&dir, &at.dirs[at.dirs.len() - 1])? {
                return Err(redirected());
            }
            let info = attribute_tag(&dir)?;
            match leaf_action(info.file_attributes, info.reparse_tag) {
                LeafAction::Refuse => {
                    drop(dir);
                    match on_link {
                        OnLink::Refuse => return Ok(Walk::Link),
                        OnLink::Clear => {
                            // Through its own handle, checked to be the link in the held parent.
                            if let Some((link, Found::Link)) = open_leaf_at(&at, &path)? {
                                delete_by_handle(&link)?;
                            }
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
/// point, such as an online-only OneDrive file) reopened normally so it hydrates. Either way the
/// read handle must be the file classified in the held parent. `None` for a link, a folder or
/// anything missing.
fn open_leaf_for_reading(at: &Beneath) -> io::Result<Option<File>> {
    let (first, found) = match open_leaf(at) {
        Ok(Some(leaf)) => leaf,
        Ok(None) => return Ok(None),
        Err(error) if unreadable(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    let Found::File { reopen } = found else {
        return Ok(None);
    };
    match reopen_same(at, &first, reopen, GENERIC_READ) {
        Ok(file) => Ok(file),
        Err(error) if unreadable(&error) => Ok(None),
        Err(error) => Err(error),
    }
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

/// Creates `at`'s leaf as a new empty file, checked to be in the held parent (deleted again
/// through its handle when it isn't). `None` when something already stands there.
fn create_leaf(at: &Beneath) -> io::Result<Option<File>> {
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .access_mode(GENERIC_READ | GENERIC_WRITE | DELETE | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(at.leaf_path())
    {
        Ok(file) => file,
        Err(error)
            if error.raw_os_error() == Some(ERROR_FILE_EXISTS)
                || error.kind() == io::ErrorKind::AlreadyExists =>
        {
            return Ok(None)
        }
        Err(error) => return Err(error),
    };
    if !in_parent(&file, &at.dirs[at.dirs.len() - 1])? {
        delete_by_handle(&file)?;
        return Err(redirected());
    }
    Ok(Some(file))
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
    for _ in 0..ATTEMPTS {
        let mut file = match open_leaf(&at)? {
            None => match create_leaf(&at)? {
                Some(file) => file,
                None => continue,
            },
            Some((link, Found::Link)) => {
                delete_by_handle(&link)?;
                drop(link);
                continue;
            }
            Some((_, Found::Dir)) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "not a regular file",
                ))
            }
            Some((first, Found::File { reopen })) => {
                match reopen_same(&at, &first, reopen, GENERIC_READ | GENERIC_WRITE)? {
                    Some(file) => file,
                    None => continue,
                }
            }
        };
        // An existing file keeps its attributes, like `fs::write`.
        file.set_len(0)?;
        file.write_all(bytes)?;
        return Ok(true);
    }
    Err(redirected())
}

/// See `worktree_guard::remove_beneath`.
pub(super) fn remove(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<bool> {
    let at = match walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step)? {
        Walk::Found(at) => at,
        Walk::Link => return Ok(false),
        Walk::Missing | Walk::NotDir => return Ok(true),
    };
    match open_leaf(&at)? {
        None => Ok(true),
        Some((_, Found::Dir)) => Ok(false),
        Some((leaf, _)) => delete_by_handle(&leaf).map(|()| true),
    }
}

/// See `beneath_unix::clear_hazard`.
pub(super) fn clear_hazard(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<()> {
    let Walk::Found(at) = walk(workdir, rel, OnLink::Clear, OnMissing::Stop, step)? else {
        return Ok(());
    };
    match open_leaf(&at)? {
        Some((link, Found::Link)) => delete_by_handle(&link),
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
    let found = match open_leaf(&at) {
        Ok(Some((_, found))) => found,
        Ok(None) => return Ok(None),
        Err(error) if unreadable(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    match found {
        Found::Dir => Ok(None),
        // `read_link` opens the link as itself (FSCTL_GET_REPARSE_POINT): never followed.
        Found::Link => match std::fs::read_link(at.leaf_path()) {
            Ok(target) => Ok(Some(Leaf::Link(
                target.to_string_lossy().replace('\\', "/").into_bytes(),
            ))),
            Err(_) => Ok(None),
        },
        Found::File { .. } => {
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
    if let Some((leaf, _)) = open_leaf(&at)? {
        delete_by_handle(&leaf)?;
    }
    // Upwards: each folder's own pinning handle is closed (a held folder can't be removed),
    // while its parents stay held; it is then opened again, checked to be the plain folder in
    // its held parent, and deleted through that handle, which fails unless it is empty. Never
    // the work tree root.
    for i in (1..at.dirs.len()).rev() {
        at.dirs.pop();
        at.names.truncate(i);
        let Ok(Some((dir, Found::Dir))) = open_leaf(&at) else {
            break;
        };
        if delete_by_handle(&dir).is_err() {
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
    let bytes = match content {
        Content::File { bytes, .. } | Content::Link(bytes) => bytes,
    };
    for _ in 0..ATTEMPTS {
        if let Some((leaf, _)) = open_leaf(&at)? {
            // Fails for a folder that isn't empty, as `remove_dir` did. The deletion completes
            // when the handle closes, right here.
            delete_by_handle(&leaf)?;
            drop(leaf);
        }
        if let Some(mut file) = create_leaf(&at)? {
            return file.write_all(bytes);
        }
    }
    Err(redirected())
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

    /// Keeps `path` open the way an antivirus scanner or the search indexer does: reading, and
    /// sharing read, write and delete.
    fn hold_open_like_a_scanner(path: &Path) -> File {
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(path)
            .unwrap()
    }

    #[test]
    fn replace_succeeds_while_another_handle_keeps_the_old_file_open() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::fs::write(dir.path().join("d/f"), "old").unwrap();
        let _scanner = hold_open_like_a_scanner(&dir.path().join("d/f"));
        replace(
            dir.path(),
            Path::new("d/f"),
            Content::File {
                bytes: b"new",
                executable: false,
            },
            &mut no_step,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("d/f")).unwrap(),
            "new"
        );
    }

    #[test]
    fn remove_and_prune_removes_a_folder_whose_file_another_handle_keeps_open() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::fs::write(dir.path().join("d/f"), "old").unwrap();
        let _scanner = hold_open_like_a_scanner(&dir.path().join("d/f"));
        remove_and_prune(dir.path(), Path::new("d/f"), &mut no_step).unwrap();
        assert!(
            std::fs::symlink_metadata(dir.path().join("d")).is_err(),
            "the emptied folder is still there"
        );
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn DeviceIoControl(
            device: *mut c_void,
            code: u32,
            in_buffer: *const c_void,
            in_size: u32,
            out_buffer: *mut c_void,
            out_size: u32,
            returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
    }

    /// Turns the existing empty folder `dir` into a junction to `target` in place
    /// (`FSCTL_SET_REPARSE_POINT`, as `mklink /J` does after creating the folder). Unlike
    /// deleting and recreating it, this needs no `DELETE` access, so it works on a folder the
    /// walk holds (#164).
    fn junction_in_place(dir: &Path, target: &Path) -> io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        const FILE_WRITE_DATA: u32 = 0x0002;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
        const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;

        // `canonicalize` gives `\\?\C:\…`; the substitute name is the NT form `\??\C:\…`.
        let canonical = std::fs::canonicalize(target)?;
        let canonical: Vec<u16> = canonical.as_os_str().encode_wide().collect();
        let print: Vec<u16> = canonical[4..].to_vec();
        let mut substitute: Vec<u16> = r"\??\".encode_utf16().collect();
        substitute.extend_from_slice(&print);

        let units = |v: &[u16]| u16::try_from(v.len() * 2).unwrap();
        // Mount-point layout: tag, data length, reserved, then substitute offset/length and
        // print offset/length (bytes, relative to the path buffer), then both names, each
        // NUL-terminated.
        let mut path_buffer = substitute.clone();
        path_buffer.push(0);
        path_buffer.extend_from_slice(&print);
        path_buffer.push(0);
        let data_length = 8 + units(&path_buffer);
        let mut buf = Vec::new();
        buf.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        buf.extend_from_slice(&data_length.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&units(&substitute).to_le_bytes());
        buf.extend_from_slice(&(units(&substitute) + 2).to_le_bytes());
        buf.extend_from_slice(&units(&print).to_le_bytes());
        for unit in path_buffer {
            buf.extend_from_slice(&unit.to_le_bytes());
        }

        let handle = OpenOptions::new()
            .access_mode(FILE_WRITE_DATA | FILE_WRITE_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(dir)?;
        let mut returned = 0u32;
        // SAFETY: the handle belongs to `handle`, which outlives the call; `buf` is a readable
        // `REPARSE_DATA_BUFFER` of exactly the size passed; no output buffer and no overlapped
        // I/O (null, 0), and `returned` is a writable u32.
        let ok = unsafe {
            DeviceIoControl(
                handle.as_raw_handle(),
                FSCTL_SET_REPARSE_POINT,
                buf.as_ptr().cast(),
                u32::try_from(buf.len()).unwrap(),
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// A step callback that turns the held, empty `root\d` into a junction to `outside` as soon
    /// as the walk has opened it, recording whether that worked.
    fn junction_d_when_opened<'a>(
        root: &'a Path,
        outside: &'a Path,
        converted: &'a mut Option<io::Result<()>>,
    ) -> impl FnMut(Step<'_>) + 'a {
        move |step| {
            if step == Step::Opened(OsStr::new("d")) && converted.is_none() {
                *converted = Some(junction_in_place(&root.join("d"), outside));
            }
        }
    }

    #[test]
    fn a_held_folder_turned_into_a_junction_is_not_written_through() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let mut converted = None;
        let written = write(
            dir.path(),
            Path::new("d/f"),
            b"x",
            &mut junction_d_when_opened(dir.path(), outside.path(), &mut converted),
        );
        converted
            .expect("the walk never opened d")
            .expect("turning the held folder into a junction");
        assert!(written.is_err(), "the write went through: {written:?}");
        assert!(!outside.path().join("f").exists(), "written outside");
    }

    #[test]
    fn a_held_folder_turned_into_a_junction_is_not_read_through() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("f"), "OUTSIDE").unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let mut converted = None;
        let read = read(
            dir.path(),
            Path::new("d/f"),
            &mut junction_d_when_opened(dir.path(), outside.path(), &mut converted),
        );
        converted
            .expect("the walk never opened d")
            .expect("turning the held folder into a junction");
        assert!(
            !matches!(read, Ok(Some(Leaf::File { ref bytes, .. })) if bytes == b"OUTSIDE"),
            "read through the junction"
        );
    }

    #[test]
    fn no_folder_is_created_in_a_junction_target_during_the_walk() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let mut converted = None;
        let mut created = 0;
        let written = {
            let mut convert = junction_d_when_opened(dir.path(), outside.path(), &mut converted);
            write(dir.path(), Path::new("d/e/f"), b"x", &mut |step| {
                if step == Step::Created(OsStr::new("e")) {
                    created += 1;
                }
                convert(step);
            })
        };
        converted
            .expect("the walk never opened d")
            .expect("turning the held folder into a junction");
        let left: Vec<_> = std::fs::read_dir(outside.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(
            left.is_empty(),
            "created in the junction's target: {left:?}"
        );
        // Pins how NTFS treats the relative create (so a change shows up here): with the held
        // parent now a reparse point, `NtCreateFile` relative to it is refused outright with
        // `ERROR_CANT_RESOLVE_FILENAME` (1921), creating nothing anywhere, and the walk fails
        // with that error.
        const ERROR_CANT_RESOLVE_FILENAME: i32 = 1921;
        let error = written.expect_err("the write went through");
        assert_eq!(
            (created, error.raw_os_error()),
            (0, Some(ERROR_CANT_RESOLVE_FILENAME)),
            "{error:?}"
        );
    }

    #[test]
    fn file_id_info_has_the_documented_layout() {
        assert_eq!(std::mem::size_of::<AttributeTagInfo>(), 8);
        assert_eq!(std::mem::size_of::<FileIdInfo>(), 24);
        assert_eq!(std::mem::align_of::<FileIdInfo>(), 8);
        assert_eq!(std::mem::size_of::<DispositionInfoEx>(), 4);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn nt_structures_have_the_documented_layout() {
        assert_eq!(std::mem::size_of::<UnicodeString>(), 16);
        assert_eq!(std::mem::size_of::<ObjectAttributes>(), 48);
        assert_eq!(std::mem::size_of::<IoStatusBlock>(), 16);
    }

    #[test]
    fn create_dir_in_creates_in_the_held_folder_and_refuses_an_existing_name() {
        let dir = TempDir::new().unwrap();
        let held = open_pinned(dir.path(), FILE_LIST_DIRECTORY).unwrap();
        create_dir_in(&held, OsStr::new("new")).unwrap();
        assert!(dir.path().join("new").is_dir());
        let again = create_dir_in(&held, OsStr::new("NEW")).unwrap_err();
        assert_eq!(again.raw_os_error(), Some(ERROR_ALREADY_EXISTS));
    }
}
