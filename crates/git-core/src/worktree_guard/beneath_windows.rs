//! Race-free raw work-tree I/O on Windows (#149, #150).
//!
//! Win32 has no `openat`, but `NtCreateFile` resolves a name relative to an open folder
//! (`RootDirectory`). The work tree root is opened by path; every folder below it and every
//! leaf is then opened (or created) by its single name relative to the folder above it, which
//! is held (see `nt_open`), *as itself* (`FILE_OPEN_REPARSE_POINT`, so a link is never followed
//! and its target, a network share included, never contacted). Folders are held open without
//! `FILE_SHARE_DELETE` until the act is done. A folder held that way can't be renamed, deleted
//! or replaced (all need `DELETE` access, which the share mode denies). A process that already
//! holds `DELETE` access makes the open fail with a sharing violation: the walk fails closed.
//!
//! A held folder that is *empty* can still be turned into a junction or symbolic link in place
//! (`FSCTL_SET_REPARSE_POINT` needs no `DELETE`). On a local NTFS/ReFS volume a relative open
//! below it then fails with `ERROR_CANT_RESOLVE_FILENAME` (reported as "a folder of the working
//! tree changed"), so nothing is opened, created or contacted in the link's target (#163,
//! #193). On an SMB share the redirector sends the relative name to the server, which resolves
//! it by path; there every component and leaf is also checked on its own handle to sit directly
//! in the held parent (`GetFinalPathNameByHandleW` of both): a mismatch fails the act, and a
//! file created by the failed act is deleted through its handle. A link's target is read from
//! its own handle (`FSCTL_GET_REPARSE_POINT`), never by path. Leaves are deleted through their
//! handle (`FileDispositionInfoEx` with POSIX semantics, read-only files included, see
//! `delete_by_handle`), never by path. A write never opens the existing file for writing data:
//! it checks that the file may be written through a handle whose file ID matches the one
//! classified (a read-only one is refused and left as it is, #209), writes a new temporary file
//! created in the held parent, and renames that over the name through the temporary file's
//! handle, the new name resolved against the held parent (`rename_in_parent`, #239).
//!
//! Each component's attributes and reparse tag come from its own handle
//! (`GetFileInformationByHandleEx`): a name surrogate (symbolic link, junction, WSL link, …) is
//! refused or removed itself; another reparse point (a OneDrive placeholder) is a folder to
//! walk through or a file reopened without the flag (so the file system filter sees the read or
//! write), which must have the same file ID. That second open is by the file ID
//! (`FILE_OPEN_BY_FILE_ID`), so a link swapped in under the name meanwhile isn't followed; only
//! where the file system has no opens by ID (FAT/exFAT, which have no links, and SMB shares) is
//! it by name.
//!
//! Leaves are opened for reading with no more access than attributes, so a file whose ACL
//! denies writing still reads; only acts that may delete ask for `DELETE` (and
//! `FILE_WRITE_ATTRIBUTES` where the ACL allows it).
//!
//! A missing folder is created the same way, relative to the held parent (`create_dir_in`),
//! then opened and checked like any other component. On an SMB share the relative opens and
//! the checks after them narrow the race but don't close it.

use std::ffi::{c_void, OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

use super::reparse::{self, leaf_action, LeafAction, FILE_ATTRIBUTE_DIRECTORY};
use super::{Content, Leaf, OnLink, OnMissing, Step, StepFn, Walk};

const FILE_LIST_DIRECTORY: u32 = 0x0001;
const FILE_READ_ATTRIBUTES: u32 = 0x0080;
const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
const SYNCHRONIZE: u32 = 0x0010_0000;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_READ: u32 = 0x1;
const FILE_SHARE_WRITE: u32 = 0x2;
const FILE_SHARE_DELETE: u32 = 0x4;
const DELETE: u32 = 0x0001_0000;
#[cfg(test)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_PATH_NOT_FOUND: i32 = 3;
const ERROR_INVALID_NAME: i32 = 123;
const ERROR_ALREADY_EXISTS: i32 = 183;
const ERROR_DIRECTORY: i32 = 267;

/// `FILE_INFO_BY_HANDLE_CLASS` values.
const FILE_BASIC_INFO_CLASS: i32 = 0;
const FILE_ATTRIBUTE_TAG_INFO_CLASS: i32 = 9;
const FILE_ID_INFO_CLASS: i32 = 18;
const FILE_DISPOSITION_INFO_CLASS: i32 = 4;
const FILE_DISPOSITION_INFO_EX_CLASS: i32 = 21;

/// `FILE_DISPOSITION_INFO_EX` flags.
const FILE_DISPOSITION_FLAG_DELETE: u32 = 0x1;
const FILE_DISPOSITION_FLAG_POSIX_SEMANTICS: u32 = 0x2;
/// Windows 10 1809 and later; earlier builds refuse it with `ERROR_INVALID_PARAMETER`.
const FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE: u32 = 0x10;

const FILE_ATTRIBUTE_READONLY: u32 = 0x1;

/// What a file system without POSIX deletes answers (see `delete_by_handle`).
const ERROR_INVALID_FUNCTION: i32 = 1;
const ERROR_NOT_SUPPORTED: i32 = 50;
const ERROR_INVALID_PARAMETER: i32 = 87;
const ERROR_ACCESS_DENIED: i32 = 5;

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

/// `FILE_BASIC_INFO`: four `LARGE_INTEGER` times (0 = leave unchanged) and the attributes.
#[repr(C)]
#[derive(Default)]
struct BasicInfo {
    creation_time: i64,
    last_access_time: i64,
    last_write_time: i64,
    change_time: i64,
    file_attributes: u32,
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
///
/// A read-only file is deleted too, as std's `remove_file` and git do (#194): with
/// `IGNORE_READONLY_ATTRIBUTE` (Windows 10 1809+). 1607–1803 answer `ERROR_INVALID_PARAMETER`
/// to that flag, so there POSIX semantics alone, which refuse a read-only file with
/// `ERROR_ACCESS_DENIED`. On access denied, as where there are no POSIX deletes, the classic
/// disposition follows, which clears the attribute and tries again (see `delete_classic`).
fn delete_by_handle(file: &File) -> io::Result<()> {
    let posix = FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS;
    let mut ignoring_read_only = DispositionInfoEx {
        flags: posix | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    };
    let result = match set_information(
        file,
        FILE_DISPOSITION_INFO_EX_CLASS,
        &mut ignoring_read_only,
    ) {
        Err(error) if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER) => set_information(
            file,
            FILE_DISPOSITION_INFO_EX_CLASS,
            &mut DispositionInfoEx { flags: posix },
        ),
        other => other,
    };
    match result {
        // No POSIX deletes here, or (1607–1803) a read-only file.
        Err(error)
            if matches!(
                error.raw_os_error(),
                Some(
                    ERROR_INVALID_FUNCTION
                        | ERROR_NOT_SUPPORTED
                        | ERROR_INVALID_PARAMETER
                        | ERROR_ACCESS_DENIED
                )
            ) =>
        {
            delete_classic(file)
        }
        other => other,
    }
}

/// The classic disposition (`FileDispositionInfo`). It has no way to ignore the read-only
/// attribute, so a read-only file has the attribute cleared through the same handle (which needs
/// `FILE_WRITE_ATTRIBUTES`) and is tried again; the attribute is put back if that fails too.
fn delete_classic(file: &File) -> io::Result<()> {
    delete_classic_with(file, |file| {
        set_information(
            file,
            FILE_DISPOSITION_INFO_CLASS,
            &mut DispositionInfo { delete_file: 1 },
        )
    })
}

/// `delete_classic` with `delete` as the disposition call (a test seam).
fn delete_classic_with(file: &File, delete: impl Fn(&File) -> io::Result<()>) -> io::Result<()> {
    let error = match delete(file) {
        Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) => error,
        other => return other,
    };
    let attributes = attribute_tag(file)?.file_attributes;
    if attributes & FILE_ATTRIBUTE_READONLY == 0 {
        return Err(error);
    }
    set_attributes(file, attributes & !FILE_ATTRIBUTE_READONLY)?;
    delete(file).inspect_err(|_| {
        let _ = set_attributes(file, attributes);
    })
}

/// The attributes `FileBasicInfo` can set: read-only, hidden, system, archive, temporary,
/// offline, not content indexed, and OneDrive's pinned/unpinned (kept so a write doesn't clear
/// them). The rest (directory, reparse point, compressed, …) are the file system's.
const SETTABLE_ATTRIBUTES: u32 =
    0x1 | 0x2 | 0x4 | 0x20 | 0x100 | 0x1000 | 0x2000 | 0x8_0000 | 0x10_0000;

/// Sets `file`'s settable attributes (`FILE_ATTRIBUTE_NORMAL` for none), leaving its times alone.
fn set_attributes(file: &File, attributes: u32) -> io::Result<()> {
    let attributes = attributes & SETTABLE_ATTRIBUTES;
    let mut info = BasicInfo {
        file_attributes: if attributes == 0 {
            FILE_ATTRIBUTE_NORMAL
        } else {
            attributes
        },
        ..BasicInfo::default()
    };
    set_information(file, FILE_BASIC_INFO_CLASS, &mut info)
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
/// `NtCreateFile` create dispositions: open an existing name (`OPEN_EXISTING`), create a new
/// one and fail if the name exists (`CREATE_NEW`).
const FILE_OPEN: u32 = 1;
const FILE_CREATE: u32 = 2;
/// `NtCreateFile` create options.
const FILE_DIRECTORY_FILE: u32 = 0x0000_0001;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;
const FILE_NON_DIRECTORY_FILE: u32 = 0x0000_0040;
const FILE_OPEN_BY_FILE_ID: u32 = 0x0000_2000;
const FILE_OPEN_FOR_BACKUP_INTENT: u32 = 0x0000_4000;
const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

const FSCTL_GET_REPARSE_POINT: u32 = 0x0009_00A8;
const MAXIMUM_REPARSE_DATA_BUFFER_SIZE: usize = 16 * 1024;

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
    fn NtSetInformationFile(
        handle: *mut c_void,
        io_status: *mut IoStatusBlock,
        info: *mut c_void,
        length: u32,
        class: i32,
    ) -> i32;
}

/// `FILE_INFORMATION_CLASS` values (ntdll's, not kernel32's `FILE_INFO_BY_HANDLE_CLASS`).
const FILE_RENAME_INFORMATION_CLASS: i32 = 10;
const FILE_RENAME_INFORMATION_EX_CLASS: i32 = 65;
/// `FILE_RENAME_INFORMATION_EX` flags.
const FILE_RENAME_REPLACE_IF_EXISTS: u32 = 0x1;
const FILE_RENAME_POSIX_SEMANTICS: u32 = 0x2;

const ERROR_SHARING_VIOLATION: i32 = 32;

/// What `NtCreateFile` relative to a held folder answers once that folder has become a reparse
/// point (a junction or symbolic link set on it in place): the name is not resolved at all.
const ERROR_CANT_RESOLVE_FILENAME: i32 = 1921;

/// Opens or creates `name`, a single component, directly in the held folder `parent`, resolved
/// against `parent`'s handle rather than a path (#163, #193): on a local NTFS/ReFS volume a
/// parent turned into a junction or symbolic link meanwhile is never followed, the open fails
/// with `ERROR_CANT_RESOLVE_FILENAME`, reported as [`redirected`]. (An SMB share resolves the name
/// by path on the server, see the module docs.)
///
/// The arguments translate `CreateFileW`'s as it does itself: `SYNCHRONIZE |
/// FILE_READ_ATTRIBUTES` are always added to `access` and `FILE_SYNCHRONOUS_IO_NONALERT` to
/// `options` (not overlapped), names are case-insensitive; `options` carries
/// `FILE_OPEN_FOR_BACKUP_INTENT` for `FILE_FLAG_BACKUP_SEMANTICS` (`FILE_NON_DIRECTORY_FILE`
/// without it) and `FILE_OPEN_REPARSE_POINT` for `FILE_FLAG_OPEN_REPARSE_POINT`; `disposition` is
/// `FILE_OPEN` for `OPEN_EXISTING` and `FILE_CREATE` for `CREATE_NEW`. Other failures keep the
/// Win32 code `CreateFileW` would report.
fn nt_open(
    parent: &File,
    name: &OsStr,
    access: u32,
    share: u32,
    disposition: u32,
    options: u32,
    attributes: u32,
) -> io::Result<File> {
    use std::os::windows::ffi::OsStrExt;
    let mut wide: Vec<u16> = name.encode_wide().collect();
    nt_create(
        parent,
        &mut wide,
        access,
        share,
        disposition,
        options,
        attributes,
    )
}

/// Opens the file whose `FILE_ID_INFO` file ID is `id`, on the volume of the held folder
/// `parent` (`FILE_OPEN_BY_FILE_ID`): by identity, not by any name, so nothing swapped in under
/// the name meanwhile (a symbolic link included) is opened or followed. The 128-bit form is tried
/// first, then the 64-bit one NTFS uses where the upper half is zero.
fn open_by_id(parent: &File, id: &[u8; 16], access: u32, options: u32) -> io::Result<File> {
    let units = |bytes: &[u8]| -> Vec<u16> {
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&unit| u16::from_ne_bytes(unit))
            .collect()
    };
    let open = |name: &mut [u16]| {
        nt_create(
            parent,
            name,
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
            options | FILE_OPEN_BY_FILE_ID,
            0,
        )
    };
    match open(&mut units(id)) {
        Err(error)
            if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER) && id[8..] == [0; 8] =>
        {
            open(&mut units(&id[..8]))
        }
        other => other,
    }
}

/// `NtCreateFile` of `name` (a single component, or with `FILE_OPEN_BY_FILE_ID` a file ID's
/// bytes) relative to `parent`; see [`nt_open`].
fn nt_create(
    parent: &File,
    wide: &mut [u16],
    access: u32,
    share: u32,
    disposition: u32,
    options: u32,
    attributes: u32,
) -> io::Result<File> {
    use std::os::windows::io::FromRawHandle;

    let length = u16::try_from(wide.len() * 2)
        .ok()
        .filter(|&length| length > 0)
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let mut object_name = UnicodeString {
        length,
        maximum_length: length,
        buffer: wide.as_mut_ptr(),
    };
    let mut object = ObjectAttributes {
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
    // new handle, `object` names `wide` (borrowed for the call) relative to `parent`'s handle
    // (owned by `parent`, which outlives the call), `io_status` is writable. No allocation size
    // and no extended attributes (null, 0).
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            access | SYNCHRONIZE | FILE_READ_ATTRIBUTES,
            &mut object,
            &mut io_status,
            std::ptr::null_mut(),
            attributes,
            share,
            disposition,
            options | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null_mut(),
            0,
        )
    };
    if status < 0 {
        // SAFETY: a pure conversion of a status code.
        let code = unsafe { RtlNtStatusToDosError(status) } as i32;
        if code == ERROR_CANT_RESOLVE_FILENAME {
            return Err(redirected());
        }
        return Err(io::Error::from_raw_os_error(code));
    }
    // SAFETY: on success `handle` is a new, open, synchronous file handle that nothing else
    // owns; the `File` closes it exactly once.
    Ok(unsafe { File::from_raw_handle(handle) })
}

/// Creates the folder `name` directly in the held folder `parent` (see [`nt_open`]). The handle
/// is closed at once. An existing name fails with `ERROR_ALREADY_EXISTS`, as `CreateDirectoryW`
/// does.
fn create_dir_in(parent: &File, name: &OsStr) -> io::Result<()> {
    nt_open(
        parent,
        name,
        0,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        FILE_CREATE,
        FILE_DIRECTORY_FILE | FILE_OPEN_FOR_BACKUP_INTENT | FILE_OPEN_REPARSE_POINT,
        FILE_ATTRIBUTE_NORMAL,
    )
    .map(drop)
}

/// The target of the link `file` (opened as itself) refers to, as `std::fs::read_link` gives it
/// with `\` turned into `/`. Read from the handle (`FSCTL_GET_REPARSE_POINT`), not by path
/// (#193). An error for a reparse point that is neither a symbolic link nor a junction.
fn link_target(file: &File) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; MAXIMUM_REPARSE_DATA_BUFFER_SIZE];
    let mut returned = 0u32;
    // SAFETY: the handle belongs to `file`, which outlives the call; no input buffer (null, 0),
    // `buf` has exactly the writable size passed, `returned` is a writable u32, and no
    // overlapped I/O (the handle is synchronous).
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_GET_REPARSE_POINT,
            std::ptr::null(),
            0,
            buf.as_mut_ptr().cast(),
            MAXIMUM_REPARSE_DATA_BUFFER_SIZE as u32,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    buf.truncate(returned as usize);
    let target = reparse::link_target(&buf)
        .ok_or_else(|| io::Error::other("not a symbolic link or junction"))?;
    Ok(String::from_utf16_lossy(&target)
        .replace('\\', "/")
        .into_bytes())
}

/// True when `child` (opened relative to `parent`) is directly in the held folder `parent`. A
/// second check for an SMB share, where the server resolves a relative name by path (see the
/// module docs).
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

/// Opens `path` by path as itself (a link is never followed), held against rename and delete.
#[cfg(test)]
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
    /// `dirs[0]` is the work tree root; `dirs[i + 1]` is `names[i]` in `dirs[i]`.
    dirs: Vec<File>,
    /// Every component of the path, the leaf last (`names.len() == dirs.len()` once walked; the
    /// leaf is `names[dirs.len() - 1]`, in the last held folder).
    names: Vec<OsString>,
}

impl Beneath {
    /// The last held folder: the leaf's parent.
    pub(super) fn parent_file(&self) -> &File {
        &self.dirs[self.dirs.len() - 1]
    }

    fn leaf_name(&self) -> &OsStr {
        &self.names[self.dirs.len() - 1]
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

/// What a leaf handle is opened for (its access beyond `FILE_READ_ATTRIBUTES | SYNCHRONIZE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Purpose {
    /// Classifying, reading through a second handle, reading a link's target: no more access, so
    /// a file whose ACL denies writing still reads.
    Read,
    /// Possibly deleting it: `DELETE`, and `FILE_WRITE_ATTRIBUTES` for a read-only file's classic
    /// delete (dropped when the ACL denies it; that one fallback then can't clear the attribute).
    Delete,
}

/// Opens `name` in `at`'s last held folder as itself, checks it is in that folder, and
/// classifies it. `None` when missing. Shares delete: the handle is what any later act uses, so
/// a rename of the name meanwhile changes nothing.
fn open_leaf_at(at: &Beneath, name: &OsStr, purpose: Purpose) -> io::Result<Option<(File, Found)>> {
    let parent = at.parent_file();
    let open = |access| {
        nt_open(
            parent,
            name,
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
            FILE_OPEN_FOR_BACKUP_INTENT | FILE_OPEN_REPARSE_POINT,
            0,
        )
    };
    let opened = match purpose {
        Purpose::Read => open(0),
        Purpose::Delete => match open(DELETE | FILE_WRITE_ATTRIBUTES) {
            Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) => open(DELETE),
            other => other,
        },
    };
    let file = match opened {
        Ok(file) => file,
        Err(error) if missing(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    if !in_parent(&file, parent)? {
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

fn open_leaf(at: &Beneath, purpose: Purpose) -> io::Result<Option<(File, Found)>> {
    open_leaf_at(at, at.leaf_name(), purpose)
}

/// Opens the file `first` refers to a second time, normally (through the file system filter for
/// a OneDrive file) or as itself, by its file ID (see [`open_by_id`]), so a link swapped in under
/// its name meanwhile is never followed. Where the file system has no opens by ID (FAT/exFAT,
/// which have no links either, and SMB shares) it is opened by its name in the held parent
/// instead. Either way the second handle must have `first`'s file ID and still be directly in
/// the held parent; `None` when it isn't or the file is gone.
fn reopen_same(at: &Beneath, first: &File, reopen: bool, access: u32) -> io::Result<Option<File>> {
    let mut options = FILE_NON_DIRECTORY_FILE;
    if !reopen {
        options |= FILE_OPEN_REPARSE_POINT;
    }
    let id = file_id(first)?;
    let opened = match open_by_id(at.parent_file(), &id.file_id, access, options) {
        Err(error)
            if matches!(
                error.raw_os_error(),
                Some(
                    ERROR_INVALID_FUNCTION
                        | ERROR_NOT_SUPPORTED
                        | ERROR_INVALID_PARAMETER
                        | ERROR_INVALID_NAME
                )
            ) =>
        {
            nt_open(
                at.parent_file(),
                at.leaf_name(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                FILE_OPEN,
                options,
                0,
            )
        }
        other => other,
    };
    let second = match opened {
        Ok(file) => file,
        Err(error) if missing(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    // Opened by identity, it may have been renamed out of the held parent meanwhile (delete is
    // shared): then not this leaf any more, and the caller tries again.
    if file_id(&second)? != id || !in_parent(&second, at.parent_file())? {
        return Ok(None);
    }
    Ok(Some(second))
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
    // The work tree as the user chose it, links on the way to it followed.
    let root_dir = OpenOptions::new()
        .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(workdir)?;
    let mut at = Beneath {
        dirs: vec![root_dir],
        names,
    };
    for k in 0..at.names.len() - 1 {
        let name = at.names[k].clone();
        let mut next = None;
        for _ in 0..ATTEMPTS {
            let dir = match nt_open(
                at.parent_file(),
                &name,
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                FILE_OPEN,
                FILE_OPEN_FOR_BACKUP_INTENT | FILE_OPEN_REPARSE_POINT,
                0,
            ) {
                Ok(dir) => dir,
                Err(error) if missing(&error) => match on_missing {
                    OnMissing::Stop => return Ok(Walk::Missing),
                    OnMissing::Create => {
                        match create_dir_in(at.parent_file(), &name) {
                            Ok(()) => step(Step::Created(&name)),
                            Err(error) if error.raw_os_error() == Some(ERROR_ALREADY_EXISTS) => {}
                            Err(error) => return Err(error),
                        }
                        continue;
                    }
                },
                Err(error) => return Err(error),
            };
            if !in_parent(&dir, at.parent_file())? {
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
                            if let Some((link, Found::Link)) =
                                open_leaf_at(&at, &name, Purpose::Delete)?
                            {
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
    let (first, found) = match open_leaf(at, Purpose::Read) {
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

/// Creates `at`'s leaf as a new empty file in the held parent, checked to be there (deleted
/// again through its handle when it isn't). `None` when something already stands there.
fn create_leaf(at: &Beneath) -> io::Result<Option<File>> {
    let parent = at.parent_file();
    let file = match nt_open(
        parent,
        at.leaf_name(),
        GENERIC_READ | GENERIC_WRITE | DELETE,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        FILE_CREATE,
        FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT,
        FILE_ATTRIBUTE_NORMAL,
    ) {
        Ok(file) => file,
        Err(error)
            if error.raw_os_error() == Some(ERROR_ALREADY_EXISTS)
                || error.kind() == io::ErrorKind::AlreadyExists =>
        {
            return Ok(None)
        }
        Err(error) => return Err(error),
    };
    if !in_parent(&file, parent)? {
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
    write_with(
        workdir,
        rel,
        step,
        &mut |file| file.write_all(bytes),
        &mut || {},
    )
}

/// `write` with `fill` writing the content into the opened file and `on_wait` called before
/// each wait for a file in use (test seams).
fn write_with(
    workdir: &Path,
    rel: &Path,
    step: StepFn<'_>,
    fill: &mut dyn FnMut(&mut File) -> io::Result<()>,
    on_wait: &mut dyn FnMut(),
) -> io::Result<bool> {
    let at = match walk(workdir, rel, OnLink::Refuse, OnMissing::Create, step)? {
        Walk::Found(at) => at,
        Walk::Link | Walk::NotDir | Walk::Missing => return Ok(false),
    };
    let in_use = || file_in_use(rel);
    let (attributes, target) = classify_for_writing(&at, &in_use, on_wait)?;
    let mut temp = create_temp(&at, &mut next_temp_number)?;
    let written = (|| {
        if attributes != 0 {
            keep_attributes(&temp, attributes)?;
        }
        fill(&mut temp)?;
        temp.sync_all()?;
        let mut tries = 0;
        loop {
            tries += 1;
            match rename_in_parent(&temp, at.parent_file(), &target)
                .map_err(|error| rename_error(&at, &target, error, &in_use))
            {
                // A program that opened the file only for a moment (a backup or sync tool)
                // gets a short while to close it.
                Err(error) if super::is_in_use(&error) && tries < IN_USE_TRIES => {
                    on_wait();
                    std::thread::sleep(IN_USE_WAIT);
                }
                other => break other,
            }
        }
    })();
    if let Err(error) = written {
        // Not renamed: still the temporary file. Gone when `temp` closes, right here.
        let _ = delete_by_handle(&temp);
        return Err(error);
    }
    // Renamed: from here `temp` is the user's file and is never deleted on an error.
    settle_after_rename(&temp, final_path(&temp), final_path(&at.dirs[0]))
}

/// After a successful rename: an SMB server resolves the new name by path, so the file may have
/// landed somewhere else. Only when its path positively lies outside the work tree (`root`) is
/// it deleted again (through its handle) and the write reported as redirected. By then it has
/// replaced whatever had that name outside, and the delete removes that name too: the outside
/// file is lost, as with any write the server redirected. When either path can't be read (some file system filters,
/// shares) or the file is inside the work tree (in the right folder, or already moved by a
/// sync tool), it is kept and the write succeeds: deleting it could lose the user's content.
fn settle_after_rename(
    file: &File,
    path: io::Result<PathBuf>,
    root: io::Result<PathBuf>,
) -> io::Result<bool> {
    let (Ok(path), Ok(root)) = (path, root) else {
        return Ok(true);
    };
    if is_beneath(&path, &root) {
        return Ok(true);
    }
    let _ = delete_by_handle(file);
    Err(redirected())
}

/// Whether `path` lies below `root`, both `GetFinalPathNameByHandleW` paths, ignoring case.
fn is_beneath(path: &Path, root: &Path) -> bool {
    let path = path.to_string_lossy().to_lowercase();
    let root = root.to_string_lossy().to_lowercase();
    let root = root.trim_end_matches('\\');
    path.strip_prefix(root)
        .is_some_and(|rest| rest.starts_with('\\') && rest.len() > 1)
}

/// How often `write` tries a file another program holds open before reporting it in use, and
/// how long it waits in between.
const IN_USE_TRIES: usize = 1;
const IN_USE_WAIT: std::time::Duration = std::time::Duration::from_millis(100);

/// The next number for a temporary file's name, unique in this process.
fn next_temp_number() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn file_in_use(rel: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::ResourceBusy,
        super::FileInUse {
            name: rel.to_string_lossy().into_owned(),
        },
    )
}

/// What `write` replaces at `at`'s leaf: the settable attributes to keep and the name to give
/// the new file (the existing file's own spelling, so the case on disk stays). Nothing for a
/// missing leaf. A link is deleted itself (through its own handle, never followed); a folder is
/// an error. An existing file must be one the user may write: opened for reading and writing
/// (by its file ID, see `reopen_same`), the handle closed at once. A read-only file is refused
/// as such (#209) and a file another program holds without sharing writing as in use (after
/// `IN_USE_TRIES`). The rename that follows also needs to delete the old file, so a file whose
/// ACL lets the user write but not delete it fails there with the system's access denied.
fn classify_for_writing(
    at: &Beneath,
    in_use: &dyn Fn() -> io::Error,
    on_wait: &mut dyn FnMut(),
) -> io::Result<(u32, OsString)> {
    let mut busy = 0;
    for _ in 0..ATTEMPTS + IN_USE_TRIES {
        // Classified with no access beyond attributes; the probe below asks for writing.
        match open_leaf(at, Purpose::Read)? {
            None => return Ok((0, at.leaf_name().to_os_string())),
            Some((_, Found::Link)) => {
                // Reopened to delete it, checked to still be the link in the held parent.
                if let Some((link, Found::Link)) = open_leaf(at, Purpose::Delete)? {
                    delete_by_handle(&link)?;
                }
            }
            Some((_, Found::Dir)) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "not a regular file",
                ))
            }
            Some((first, Found::File { reopen })) => {
                match reopen_same(at, &first, reopen, GENERIC_READ | GENERIC_WRITE) {
                    Ok(Some(_writable)) => {
                        let attributes = attribute_tag(&first)?.file_attributes;
                        let name = on_disk_name(at.leaf_name(), &final_path(&first)?);
                        return Ok((attributes & SETTABLE_ATTRIBUTES, name));
                    }
                    Ok(None) => {}
                    Err(error) if error.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => {
                        busy += 1;
                        if busy >= IN_USE_TRIES {
                            return Err(in_use());
                        }
                        on_wait();
                        std::thread::sleep(IN_USE_WAIT);
                    }
                    Err(error) => return Err(read_only_or(&first, error)),
                }
            }
        }
    }
    Err(redirected())
}

/// The name the new file gets for `leaf`, whose file's final path is `path`: that path's last
/// component (the spelling on disk) when it equals `leaf` ignoring case, else `leaf` itself (a
/// short 8.3 name, or another name of a hardlinked file).
fn on_disk_name(leaf: &OsStr, path: &Path) -> OsString {
    let fold = |name: &OsStr| super::fold_name(name.to_string_lossy().as_bytes());
    match path.file_name() {
        Some(name) if fold(name) == fold(leaf) => name.to_os_string(),
        _ => leaf.to_os_string(),
    }
}

/// The temporary file's name for `leaf`: a leading dot as on Unix (which hides it there, not
/// here), named after it and this process so a left-over one (after a crash) says where it came
/// from. The part taken from `leaf` is kept short so the name fits where `leaf` itself fits.
fn temp_name(leaf: &OsStr, n: usize) -> OsString {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let units: Vec<u16> = leaf.encode_wide().collect();
    let mut cut = units.len().min(TEMP_PREFIX_UNITS);
    // Not between the two halves of a surrogate pair.
    if cut > 0 && cut < units.len() && (0xD800..0xDC00).contains(&units[cut - 1]) {
        cut -= 1;
    }
    let mut name: Vec<u16> = ".".encode_utf16().collect();
    name.extend_from_slice(&units[..cut]);
    name.extend(format!(".browsitory-{}-{n}.tmp", std::process::id()).encode_utf16());
    OsString::from_wide(&name)
}

/// At most this many UTF-16 units of the leaf's name go into its temporary file's name.
const TEMP_PREFIX_UNITS: usize = 64;

/// Creates a new, empty temporary file next to `at`'s leaf, in the held parent and checked to
/// be there (deleted again through its handle when it isn't). Its handle can write, set
/// attributes and rename or delete it. `next` numbers the names (`next_temp_number`; a test
/// seam); a name that exists is skipped.
fn create_temp(at: &Beneath, next: &mut dyn FnMut() -> usize) -> io::Result<File> {
    let parent = at.parent_file();
    for _ in 0..ATTEMPTS {
        let name = temp_name(at.leaf_name(), next());
        let file = match nt_open(
            parent,
            &name,
            GENERIC_WRITE | DELETE,
            FILE_SHARE_READ,
            FILE_CREATE,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT,
            FILE_ATTRIBUTE_NORMAL,
        ) {
            Ok(file) => file,
            Err(error)
                if error.raw_os_error() == Some(ERROR_ALREADY_EXISTS)
                    || error.kind() == io::ErrorKind::AlreadyExists =>
            {
                continue
            }
            Err(error) => return Err(error),
        };
        if !in_parent(&file, parent)? {
            delete_by_handle(&file)?;
            return Err(redirected());
        }
        return Ok(file);
    }
    Err(io::Error::from_raw_os_error(ERROR_ALREADY_EXISTS))
}

/// Gives the new file the replaced file's settable attributes. OneDrive's pinned/unpinned
/// attributes only mean something in a synced folder: where they are refused, the others
/// (read-only, hidden, system, archive, not content indexed) are set alone.
fn keep_attributes(file: &File, attributes: u32) -> io::Result<()> {
    const BASIC: u32 = 0x1 | 0x2 | 0x4 | 0x20 | 0x2000;
    set_attributes(file, attributes).or_else(|_| set_attributes(file, attributes & BASIC))
}

/// `write`'s error for a rename that failed: in use when another program holds the target
/// without sharing delete (a sharing violation, or access denied while an open of it for
/// deleting is refused as a sharing violation); read-only when it became read-only meanwhile;
/// else `error` itself.
fn rename_error(
    at: &Beneath,
    target: &OsStr,
    error: io::Error,
    in_use: &dyn Fn() -> io::Error,
) -> io::Error {
    match error.raw_os_error() {
        Some(ERROR_SHARING_VIOLATION) => in_use(),
        Some(ERROR_ACCESS_DENIED) => {
            if let Ok(Some((first, Found::File { .. }))) = open_leaf_at(at, target, Purpose::Read) {
                if has_read_only_attribute(&first).unwrap_or(false) {
                    return super::read_only_error();
                }
            }
            let held = nt_open(
                at.parent_file(),
                target,
                DELETE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                FILE_OPEN,
                FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT,
                0,
            );
            match held {
                Err(probe) if probe.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => in_use(),
                _ => error,
            }
        }
        _ => error,
    }
}

/// Renames the open file `file` to `name` in the held folder `parent`, replacing whatever file
/// or link has that name (never following it): `NtSetInformationFile` on `file`'s handle with
/// `RootDirectory` = `parent`, so the name is resolved against the held folder, never a path.
/// POSIX semantics (`FileRenameInformationEx`) replace a file another program keeps open with
/// delete sharing (a scanner, the indexer); where that class or flag isn't supported (older
/// Windows, FAT, some shares) the classic `FileRenameInformation` with `ReplaceIfExists`.
fn rename_in_parent(file: &File, parent: &File, name: &OsStr) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = name.encode_wide().collect();
    rename_in_parent_with(|class, flags| set_rename_info(file, parent, class, flags, &wide))
}

/// `rename_in_parent` with `set(class, flags)` as the rename call (a test seam).
fn rename_in_parent_with(mut set: impl FnMut(i32, u32) -> io::Result<()>) -> io::Result<()> {
    match set(
        FILE_RENAME_INFORMATION_EX_CLASS,
        FILE_RENAME_REPLACE_IF_EXISTS | FILE_RENAME_POSIX_SEMANTICS,
    ) {
        Err(error)
            if matches!(
                error.raw_os_error(),
                Some(ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED | ERROR_INVALID_PARAMETER)
            ) =>
        {
            // `ReplaceIfExists` = TRUE, the first byte of the same field.
            set(FILE_RENAME_INFORMATION_CLASS, 1)
        }
        other => other,
    }
}

/// `FILE_RENAME_INFORMATION(_EX)`'s fixed part: a `BOOLEAN ReplaceIfExists` / `ULONG Flags`
/// union, the `RootDirectory` handle and the name's length in bytes; the name (`WCHAR`s, no
/// terminating NUL) follows at `RENAME_NAME_OFFSET`.
#[repr(C)]
struct RenameInfo {
    flags: u32,
    root_directory: *mut c_void,
    file_name_length: u32,
}

const RENAME_NAME_OFFSET: usize = std::mem::offset_of!(RenameInfo, file_name_length) + 4;

/// A `FILE_RENAME_INFORMATION(_EX)` for `name` in `root`, in a buffer aligned for it. Its size
/// in bytes is the buffer's, at least the structure's.
fn rename_info(flags: u32, root: *mut c_void, name: &[u16]) -> io::Result<Vec<u64>> {
    let name_bytes = name.len() * 2;
    let file_name_length = u32::try_from(name_bytes)
        .ok()
        .filter(|&length| length > 0)
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let size = (RENAME_NAME_OFFSET + name_bytes).max(std::mem::size_of::<RenameInfo>());
    let mut buf = vec![0u64; size.div_ceil(8)];
    let base = buf.as_mut_ptr().cast::<u8>();
    // SAFETY: `buf` holds at least `size` bytes, aligned for `RenameInfo` (8); the fixed part is
    // written first, then the name after `RENAME_NAME_OFFSET`, which may overwrite the fixed
    // part's trailing padding only, and ends within `size`.
    unsafe {
        base.cast::<RenameInfo>().write(RenameInfo {
            flags,
            root_directory: root,
            file_name_length,
        });
        std::ptr::copy_nonoverlapping(
            name.as_ptr().cast::<u8>(),
            base.add(RENAME_NAME_OFFSET),
            name_bytes,
        );
    }
    Ok(buf)
}

fn set_rename_info(
    file: &File,
    parent: &File,
    class: i32,
    flags: u32,
    name: &[u16],
) -> io::Result<()> {
    let mut info = rename_info(flags, parent.as_raw_handle(), name)?;
    let length =
        u32::try_from(info.len() * 8).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let mut io_status = IoStatusBlock {
        status: 0,
        information: 0,
    };
    // SAFETY: the handles belong to `file` and `parent`, which outlive the call; `info` is a
    // `FILE_RENAME_INFORMATION(_EX)` of `length` bytes built by `rename_info`, and `io_status`
    // is writable.
    let status = unsafe {
        NtSetInformationFile(
            file.as_raw_handle(),
            &mut io_status,
            info.as_mut_ptr().cast(),
            length,
            class,
        )
    };
    if status < 0 {
        // SAFETY: a pure conversion of a status code.
        let code = unsafe { RtlNtStatusToDosError(status) } as i32;
        if code == ERROR_CANT_RESOLVE_FILENAME {
            return Err(redirected());
        }
        return Err(io::Error::from_raw_os_error(code));
    }
    Ok(())
}

/// `write`'s error for a file it may not open for writing: `read_only_error` when the open was
/// denied and `first` (the classified handle) has the read-only attribute (#209), which is left
/// as it is; else `error` itself (an ACL that denies writing, for one).
fn read_only_or(first: &File, error: io::Error) -> io::Error {
    if error.raw_os_error() != Some(ERROR_ACCESS_DENIED) {
        return error;
    }
    match has_read_only_attribute(first) {
        Ok(true) => super::read_only_error(),
        _ => error,
    }
}

fn has_read_only_attribute(file: &File) -> io::Result<bool> {
    Ok(attribute_tag(file)?.file_attributes & FILE_ATTRIBUTE_READONLY != 0)
}

/// See `worktree_guard::is_read_only_beneath`. The leaf is classified through a handle with no
/// access beyond its attributes, opened as itself, so an online-only OneDrive file is not
/// downloaded and a link is not followed.
pub(super) fn is_read_only(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<bool> {
    let at = match walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step)? {
        Walk::Found(at) => at,
        Walk::Link | Walk::Missing | Walk::NotDir => return Ok(false),
    };
    match open_leaf(&at, Purpose::Read)? {
        Some((leaf, Found::File { .. })) => has_read_only_attribute(&leaf),
        _ => Ok(false),
    }
}

/// See `worktree_guard::remove_beneath`.
pub(super) fn remove(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<bool> {
    let at = match walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step)? {
        Walk::Found(at) => at,
        Walk::Link => return Ok(false),
        Walk::Missing | Walk::NotDir => return Ok(true),
    };
    match open_leaf(&at, Purpose::Delete)? {
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
    match open_leaf(&at, Purpose::Delete)? {
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
    let (leaf, found) = match open_leaf(&at, Purpose::Read) {
        Ok(Some(leaf)) => leaf,
        Ok(None) => return Ok(None),
        Err(error) if unreadable(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    match found {
        Found::Dir => Ok(None),
        // From the handle just checked to be in the held parent: never followed.
        Found::Link => Ok(link_target(&leaf).ok().map(Leaf::Link)),
        Found::File { .. } => {
            drop(leaf);
            let Some(mut file) = open_leaf_for_reading(&at)? else {
                return Ok(None);
            };
            let stat = super::FileStat::of(&file.metadata()?);
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            Ok(Some(Leaf::File {
                bytes,
                executable: false,
                stat,
            }))
        }
    }
}

/// See `worktree_guard::remove_and_prune_beneath`.
pub(super) fn remove_and_prune(workdir: &Path, rel: &Path, step: StepFn<'_>) -> io::Result<()> {
    let Walk::Found(mut at) = walk(workdir, rel, OnLink::Refuse, OnMissing::Stop, step)? else {
        return Ok(());
    };
    if let Some((leaf, _)) = open_leaf(&at, Purpose::Delete)? {
        delete_by_handle(&leaf)?;
    }
    // Upwards: each folder's own pinning handle is closed (a held folder can't be removed),
    // while its parents stay held; it is then opened again, checked to be the plain folder in
    // its held parent, and deleted through that handle, which fails unless it is empty. Never
    // the work tree root.
    for i in (1..at.dirs.len()).rev() {
        at.dirs.pop();
        at.names.truncate(i);
        let Ok(Some((dir, Found::Dir))) = open_leaf(&at, Purpose::Delete) else {
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
        if let Some((leaf, _)) = open_leaf(&at, Purpose::Delete)? {
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

    /// Turns the existing empty folder `dir` into a junction to `target` in place
    /// (`FSCTL_SET_REPARSE_POINT`, as `mklink /J` does after creating the folder). Unlike
    /// deleting and recreating it, this needs no `DELETE` access, so it works on a folder the
    /// walk holds (#164).
    fn junction_in_place(dir: &Path, target: &Path) -> io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        const FILE_WRITE_DATA: u32 = 0x0002;
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
        assert_eq!(
            read.map_err(|error| error.to_string()),
            Err(redirected().to_string())
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
        // With the held parent now a reparse point, the relative open of `e` is refused
        // outright (`ERROR_CANT_RESOLVE_FILENAME`, see `nt_open_relative_to_a_converted_folder_
        // is_refused`): nothing is created anywhere and the walk says the folder changed.
        let error = written.expect_err("the write went through");
        assert_eq!(
            (created, error.to_string()),
            (0, redirected().to_string()),
            "{error:?}"
        );
    }

    /// Pins how NTFS treats an open relative to a held folder that became a junction (so a
    /// change shows up here): refused with `ERROR_CANT_RESOLVE_FILENAME` (1921), never resolved
    /// in the junction's target, for an existing name and a new one alike.
    #[test]
    fn nt_open_relative_to_a_converted_folder_is_refused() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("f"), "OUTSIDE").unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let root = open_pinned(dir.path(), FILE_LIST_DIRECTORY).unwrap();
        let held = nt_open(
            &root,
            OsStr::new("d"),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            FILE_OPEN,
            FILE_OPEN_FOR_BACKUP_INTENT | FILE_OPEN_REPARSE_POINT,
            0,
        )
        .unwrap();
        junction_in_place(&dir.path().join("d"), outside.path()).unwrap();
        let existing = nt_open(
            &held,
            OsStr::new("f"),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT,
            0,
        );
        assert_eq!(
            existing.map(drop).map_err(|e| e.to_string()),
            Err(redirected().to_string())
        );
        let new = nt_open(
            &held,
            OsStr::new("g"),
            GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            FILE_CREATE,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT,
            FILE_ATTRIBUTE_NORMAL,
        );
        assert_eq!(
            new.map(drop).map_err(|e| e.to_string()),
            Err(redirected().to_string())
        );
        assert!(!outside.path().join("g").exists());
    }

    #[test]
    fn nt_open_opens_a_file_in_the_held_folder() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f"), "inside").unwrap();
        let root = open_pinned(dir.path(), FILE_LIST_DIRECTORY).unwrap();
        let open = |name: &str| {
            nt_open(
                &root,
                OsStr::new(name),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                FILE_OPEN,
                FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT,
                0,
            )
        };
        let mut text = String::new();
        open("F").unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "inside");
        let missing_file = open("nope").unwrap_err();
        assert_eq!(missing_file.raw_os_error(), Some(ERROR_FILE_NOT_FOUND));
    }

    /// Whether this process may create symbolic links (an elevated runner, or developer mode).
    fn symlink_privilege(dir: &Path) -> bool {
        std::os::windows::fs::symlink_file("x", dir.join("probe")).is_ok()
    }

    /// The link's target as `read` reports it, from the handle `open_leaf` returns.
    fn target_through_the_handle(dir: &Path, name: &str) -> String {
        let Walk::Found(at) = walk(
            dir,
            Path::new(name),
            OnLink::Refuse,
            OnMissing::Stop,
            &mut no_step,
        )
        .unwrap() else {
            panic!("not found")
        };
        let Some((leaf, Found::Link)) = open_leaf(&at, Purpose::Read).unwrap() else {
            panic!("{name} is not a link")
        };
        String::from_utf8(link_target(&leaf).unwrap()).unwrap()
    }

    fn read_link(path: &Path) -> String {
        std::fs::read_link(path)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    }

    #[test]
    fn link_target_matches_read_link() {
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        junction(&dir.path().join("j"), outside.path());
        assert_eq!(
            target_through_the_handle(dir.path(), "j"),
            read_link(&dir.path().join("j"))
        );
        assert_eq!(
            read(dir.path(), Path::new("j"), &mut no_step).unwrap(),
            Some(Leaf::Link(read_link(&dir.path().join("j")).into_bytes()))
        );
        if !symlink_privilege(dir.path()) {
            // CI runners may create symbolic links: there the check must not be skipped.
            assert!(
                std::env::var_os("CI").is_none(),
                "no privilege to create symbolic links on CI"
            );
            eprintln!("SKIP: no privilege to create symbolic links");
            return;
        }
        let absolute = outside.path().join("f");
        std::os::windows::fs::symlink_file(&absolute, dir.path().join("abs")).unwrap();
        std::os::windows::fs::symlink_file(r"..\other\f", dir.path().join("rel")).unwrap();
        std::os::windows::fs::symlink_dir(outside.path(), dir.path().join("dirlink")).unwrap();
        for name in ["abs", "rel", "dirlink"] {
            assert_eq!(
                target_through_the_handle(dir.path(), name),
                read_link(&dir.path().join(name)),
                "{name}"
            );
        }
        assert_eq!(target_through_the_handle(dir.path(), "rel"), "../other/f");
    }

    /// Sets the read-only attribute, as Explorer's "Read-only" box or `attrib +R` does.
    fn make_read_only(path: &Path) {
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(path, permissions).unwrap();
    }

    #[test]
    fn remove_deletes_a_read_only_file() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::fs::write(dir.path().join("d/f"), "old").unwrap();
        make_read_only(&dir.path().join("d/f"));
        assert!(remove(dir.path(), Path::new("d/f"), &mut no_step).unwrap());
        assert!(std::fs::symlink_metadata(dir.path().join("d/f")).is_err());
    }

    #[test]
    fn replace_replaces_a_read_only_file() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::fs::write(dir.path().join("d/f"), "old").unwrap();
        make_read_only(&dir.path().join("d/f"));
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

    /// Clears `0`'s read-only attribute when dropped, so a failed assertion doesn't leave a
    /// temp folder that can't be deleted.
    struct WritableAgain(PathBuf);

    impl Drop for WritableAgain {
        fn drop(&mut self) {
            if let Ok(meta) = std::fs::symlink_metadata(&self.0) {
                let mut permissions = meta.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                let _ = std::fs::set_permissions(&self.0, permissions);
            }
        }
    }

    #[test]
    fn writing_a_read_only_file_is_reported_as_read_only() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::fs::write(dir.path().join("d/f"), "old").unwrap();
        make_read_only(&dir.path().join("d/f"));
        let _writable_again = WritableAgain(dir.path().join("d/f"));
        let error = write(dir.path(), Path::new("d/f"), b"new", &mut no_step).unwrap_err();
        assert!(crate::worktree_guard::is_read_only(&error), "{error:?}");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("d/f")).unwrap(),
            "old"
        );
        assert!(std::fs::metadata(dir.path().join("d/f"))
            .unwrap()
            .permissions()
            .readonly());
        assert_eq!(names_in(&dir.path().join("d")), ["f"]);
    }

    #[test]
    fn is_read_only_reads_the_attribute_without_opening_the_data() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::fs::write(dir.path().join("d/f"), "old").unwrap();
        std::fs::write(dir.path().join("d/g"), "old").unwrap();
        make_read_only(&dir.path().join("d/f"));
        let _writable_again = WritableAgain(dir.path().join("d/f"));
        assert!(is_read_only(dir.path(), Path::new("d/f"), &mut no_step).unwrap());
        assert!(!is_read_only(dir.path(), Path::new("d/g"), &mut no_step).unwrap());
        assert!(!is_read_only(dir.path(), Path::new("d/missing"), &mut no_step).unwrap());
        assert!(!is_read_only(dir.path(), Path::new("d"), &mut no_step).unwrap());
    }

    #[test]
    fn writing_a_writable_file_still_works() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::fs::write(dir.path().join("d/f"), "old").unwrap();
        assert!(write(dir.path(), Path::new("d/f"), b"new", &mut no_step).unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("d/f")).unwrap(),
            "new"
        );
        assert!(write(dir.path(), Path::new("d/g"), b"created", &mut no_step).unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("d/g")).unwrap(),
            "created"
        );
    }

    /// The names in `dir`, sorted.
    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// A fill that writes "par" and then fails, as a full disk would.
    fn fail_halfway(file: &mut File) -> io::Result<()> {
        file.write_all(b"par")?;
        Err(io::Error::other("disk full"))
    }

    fn attributes_of(path: &Path) -> u32 {
        use std::os::windows::fs::MetadataExt;
        std::fs::symlink_metadata(path).unwrap().file_attributes()
    }

    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

    fn make_hidden(path: &Path) {
        let status = std::process::Command::new("attrib")
            .arg("+H")
            .arg(path)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "attrib +H failed");
        assert_ne!(attributes_of(path) & FILE_ATTRIBUTE_HIDDEN, 0);
    }

    #[test]
    fn a_failed_write_keeps_the_old_content() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let f = dir.path().join("d").join("f");
        std::fs::write(&f, "old content").unwrap();
        make_hidden(&f);
        let error = write_with(
            dir.path(),
            Path::new("d/f"),
            &mut no_step,
            &mut fail_halfway,
            &mut || {},
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "disk full");
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "old content");
        assert_ne!(attributes_of(&f) & FILE_ATTRIBUTE_HIDDEN, 0);
        assert_eq!(names_in(&dir.path().join("d")), ["f"]);
    }

    #[test]
    fn a_failed_write_of_a_new_file_leaves_nothing() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::fs::write(dir.path().join("d").join("f"), "old").unwrap();
        let error = write_with(
            dir.path(),
            Path::new("d/new"),
            &mut no_step,
            &mut fail_halfway,
            &mut || {},
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "disk full");
        assert_eq!(names_in(&dir.path().join("d")), ["f"]);
    }

    #[test]
    fn write_keeps_the_hidden_attribute() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let f = dir.path().join("d").join("f");
        std::fs::write(&f, "old").unwrap();
        make_hidden(&f);
        assert!(write(dir.path(), Path::new("d/f"), b"new", &mut no_step).unwrap());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "new");
        assert_ne!(attributes_of(&f) & FILE_ATTRIBUTE_HIDDEN, 0);
        assert_eq!(names_in(&dir.path().join("d")), ["f"]);
    }

    #[test]
    fn write_keeps_the_on_disk_name_case() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::fs::write(dir.path().join("d").join("F.txt"), "old").unwrap();
        assert!(write(dir.path(), Path::new("d/f.txt"), b"new", &mut no_step).unwrap());
        assert_eq!(names_in(&dir.path().join("d")), ["F.txt"]);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("d").join("F.txt")).unwrap(),
            "new"
        );
    }

    #[test]
    fn writing_a_file_held_without_share_delete_is_in_use() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let f = dir.path().join("d").join("f");
        std::fs::write(&f, "old").unwrap();
        // As an editor that keeps the file open, sharing read and write but not delete.
        let _editor = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&f)
            .unwrap();
        let error = write(dir.path(), Path::new("d/f"), b"new", &mut no_step).unwrap_err();
        assert!(crate::worktree_guard::is_in_use(&error), "{error:?}");
        assert_eq!(
            error.to_string(),
            "'d/f' is open in another program. Close it, then try again."
        );
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "old");
        assert_eq!(names_in(&dir.path().join("d")), ["f"]);
    }

    #[test]
    fn writing_a_file_held_without_share_write_is_in_use() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let f = dir.path().join("d").join("f");
        std::fs::write(&f, "old").unwrap();
        // Shares reading only: the probe for writing is refused before any temporary file.
        let _holder = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&f)
            .unwrap();
        let error = write(dir.path(), Path::new("d/f"), b"new", &mut no_step).unwrap_err();
        assert!(crate::worktree_guard::is_in_use(&error), "{error:?}");
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "old");
        assert_eq!(names_in(&dir.path().join("d")), ["f"]);
    }

    /// A written file, opened as `write` holds it after the rename (`DELETE` access).
    fn renamed_file(dir: &Path) -> (PathBuf, File) {
        let path = dir.join("f");
        std::fs::write(&path, "new").unwrap();
        let file = OpenOptions::new()
            .access_mode(GENERIC_WRITE | DELETE)
            .share_mode(FILE_SHARE_READ)
            .open(&path)
            .unwrap();
        (path, file)
    }

    #[test]
    fn a_failed_check_after_the_rename_keeps_the_file() {
        let dir = TempDir::new().unwrap();
        let (path, file) = renamed_file(dir.path());
        let root = final_path(&open_pinned(dir.path(), 0).unwrap());
        let unreadable = || Err(io::Error::from_raw_os_error(ERROR_NOT_SUPPORTED));
        assert!(settle_after_rename(&file, unreadable(), root).unwrap());
        assert!(settle_after_rename(&file, final_path(&file), unreadable()).unwrap());
        drop(file);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn a_file_moved_elsewhere_in_the_work_tree_after_the_rename_is_kept() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let (path, file) = renamed_file(&dir.path().join("sub"));
        let root = final_path(&open_pinned(dir.path(), 0).unwrap()).unwrap();
        // As a sync tool may have moved it: elsewhere, but still inside the work tree.
        let moved = root.join("other").join("f");
        assert!(settle_after_rename(&file, Ok(moved), Ok(root)).unwrap());
        drop(file);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn a_file_that_landed_outside_the_work_tree_is_deleted() {
        let work_tree = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let (path, file) = renamed_file(outside.path());
        let root = final_path(&open_pinned(work_tree.path(), 0).unwrap());
        let error = settle_after_rename(&file, final_path(&file), root).unwrap_err();
        assert_eq!(error.to_string(), redirected().to_string());
        drop(file);
        assert!(std::fs::symlink_metadata(&path).is_err(), "still there");
    }

    #[test]
    fn is_beneath_compares_whole_components_ignoring_case() {
        let root = Path::new(r"\\?\C:\Work");
        assert!(is_beneath(Path::new(r"\\?\C:\work\d\f"), root));
        assert!(is_beneath(
            Path::new(r"\\?\C:\Work\f"),
            Path::new(r"\\?\C:\Work\")
        ));
        assert!(!is_beneath(Path::new(r"\\?\C:\Work2\f"), root));
        assert!(!is_beneath(Path::new(r"\\?\C:\Work"), root));
        assert!(!is_beneath(Path::new(r"\\?\D:\Work\f"), root));
    }

    #[test]
    fn a_file_held_for_a_moment_is_replaced_after_a_retry() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let f = dir.path().join("d").join("f");
        std::fs::write(&f, "old").unwrap();
        // Shares reading and writing, not delete: the rename is refused while it is open.
        let mut holder = Some(
            OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .open(&f)
                .unwrap(),
        );
        let mut waits = 0;
        let written = write_with(
            dir.path(),
            Path::new("d/f"),
            &mut no_step,
            &mut |file| file.write_all(b"new"),
            // The other program closes the file during the first wait.
            &mut || {
                waits += 1;
                holder.take();
            },
        );
        assert!(written.unwrap());
        assert_eq!(waits, 1);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "new");
        assert_eq!(names_in(&dir.path().join("d")), ["f"]);
    }

    #[test]
    fn create_temp_skips_names_that_exist() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        for n in 0..2 {
            let name = temp_name(OsStr::new("f"), n);
            std::fs::write(dir.path().join("d").join(&name), "someone else's").unwrap();
        }
        let Walk::Found(at) = walk(
            dir.path(),
            Path::new("d/f"),
            OnLink::Refuse,
            OnMissing::Stop,
            &mut no_step,
        )
        .unwrap() else {
            panic!("not found")
        };
        let mut n = 0;
        let temp = create_temp(&at, &mut || {
            n += 1;
            n - 1
        })
        .unwrap();
        assert_eq!(
            final_path(&temp).unwrap().file_name().unwrap(),
            temp_name(OsStr::new("f"), 2)
        );
        delete_by_handle(&temp).unwrap();
        drop(temp);
        for n in 0..2 {
            let name = temp_name(OsStr::new("f"), n);
            assert_eq!(
                std::fs::read_to_string(dir.path().join("d").join(&name)).unwrap(),
                "someone else's"
            );
        }
        // Every name taken: an error, nothing created.
        let error = create_temp(&at, &mut || 0).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(ERROR_ALREADY_EXISTS));
        assert_eq!(names_in(&dir.path().join("d")).len(), 2);
    }

    #[test]
    fn on_disk_name_takes_the_final_spelling_only_for_the_same_name() {
        assert_eq!(
            on_disk_name(OsStr::new("f.txt"), Path::new(r"\\?\C:\d\F.txt")),
            "F.txt"
        );
        // A short 8.3 name, or another name of a hardlinked file: the leaf as given.
        assert_eq!(
            on_disk_name(
                OsStr::new("LONGFI~1.TXT"),
                Path::new(r"\\?\C:\d\longfilename.txt")
            ),
            "LONGFI~1.TXT"
        );
        assert_eq!(on_disk_name(OsStr::new("a"), Path::new(r"\\?\C:\d\b")), "a");
    }

    #[test]
    fn writing_a_hardlink_keeps_its_name_and_the_other_name_keeps_the_old_content() {
        // Both names in one folder: the final path may report either, and the new file must
        // still get the name written. (A hardlink whose other name is in another folder fails
        // the walker's in-the-held-parent check, as it did before #239.)
        let dir = TempDir::new().unwrap();
        let d = dir.path().join("d");
        std::fs::create_dir(&d).unwrap();
        std::fs::write(d.join("a-other"), "old").unwrap();
        std::fs::hard_link(d.join("a-other"), d.join("f")).unwrap();
        assert!(write(dir.path(), Path::new("d/f"), b"new", &mut no_step).unwrap());
        assert_eq!(names_in(&d), ["a-other", "f"]);
        assert_eq!(std::fs::read_to_string(d.join("f")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(d.join("a-other")).unwrap(), "old");
    }

    #[test]
    fn a_file_held_open_by_a_scanner_is_still_replaced() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let f = dir.path().join("d").join("f");
        std::fs::write(&f, "old").unwrap();
        let _scanner = hold_open_like_a_scanner(&f);
        assert!(write(dir.path(), Path::new("d/f"), b"new", &mut no_step).unwrap());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "new");
        assert_eq!(names_in(&dir.path().join("d")), ["f"]);
    }

    #[test]
    fn remove_and_prune_removes_a_read_only_file_and_its_folder() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        std::fs::write(dir.path().join("d/f"), "old").unwrap();
        make_read_only(&dir.path().join("d/f"));
        remove_and_prune(dir.path(), Path::new("d/f"), &mut no_step).unwrap();
        assert!(
            std::fs::symlink_metadata(dir.path().join("d")).is_err(),
            "the read-only file or its folder is still there"
        );
    }

    #[test]
    fn a_converted_folder_never_has_its_target_opened_for_reading() {
        const ERROR_SHARING_VIOLATION: i32 = 32;
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("f"), "OUTSIDE").unwrap();
        // Shares nothing: any open of the target fails with a sharing violation.
        let _exclusive = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(outside.path().join("f"))
            .unwrap();
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
            !matches!(&read, Err(error) if error.raw_os_error() == Some(ERROR_SHARING_VIOLATION)),
            "the junction's target was opened: {read:?}"
        );
        assert_eq!(
            read.map_err(|error| error.to_string()),
            Err(redirected().to_string())
        );
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn FindFirstChangeNotificationW(path: *const u16, subtree: i32, filter: u32)
            -> *mut c_void;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn FindCloseChangeNotification(handle: *mut c_void) -> i32;
    }

    #[test]
    fn a_converted_folder_never_gets_a_name_in_its_target_even_briefly() {
        use std::os::windows::ffi::OsStrExt;
        const FILE_NOTIFY_CHANGE_FILE_NAME: u32 = 0x1;
        const WAIT_OBJECT_0: u32 = 0;
        let dir = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let wide: Vec<u16> = outside
            .path()
            .as_os_str()
            .encode_wide()
            .chain([0])
            .collect();
        // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call.
        let watch =
            unsafe { FindFirstChangeNotificationW(wide.as_ptr(), 0, FILE_NOTIFY_CHANGE_FILE_NAME) };
        assert!(
            !watch.is_null() && watch as isize != -1,
            "{}",
            io::Error::last_os_error()
        );
        let mut converted = None;
        let written = write(
            dir.path(),
            Path::new("d/f"),
            b"x",
            &mut junction_d_when_opened(dir.path(), outside.path(), &mut converted),
        );
        // SAFETY: `watch` is the open change notification handle from above.
        let signalled = unsafe { WaitForSingleObject(watch, 500) } == WAIT_OBJECT_0;
        // SAFETY: closed exactly once, here.
        unsafe { FindCloseChangeNotification(watch) };
        converted
            .expect("the walk never opened d")
            .expect("turning the held folder into a junction");
        assert!(written.is_err(), "the write went through: {written:?}");
        assert!(!signalled, "a name was created in the junction's target");
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }

    #[test]
    fn file_id_info_has_the_documented_layout() {
        assert_eq!(std::mem::size_of::<AttributeTagInfo>(), 8);
        assert_eq!(std::mem::size_of::<FileIdInfo>(), 24);
        assert_eq!(std::mem::align_of::<FileIdInfo>(), 8);
        assert_eq!(std::mem::size_of::<DispositionInfoEx>(), 4);
        assert_eq!(std::mem::size_of::<BasicInfo>(), 40);
        assert_eq!(std::mem::offset_of!(BasicInfo, file_attributes), 32);
    }

    /// The fallback for file systems without POSIX deletes (FAT/exFAT, some shares), tested
    /// directly on NTFS: it clears the read-only attribute itself.
    #[test]
    fn delete_classic_deletes_a_read_only_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, "old").unwrap();
        make_read_only(&path);
        let file = OpenOptions::new()
            .access_mode(DELETE | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES | SYNCHRONIZE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
            .unwrap();
        delete_classic(&file).unwrap();
        drop(file);
        assert!(std::fs::symlink_metadata(&path).is_err());
    }

    /// When the delete still fails after the read-only attribute was cleared, the attribute is
    /// put back (the second disposition call is made to fail).
    #[test]
    fn delete_classic_restores_read_only_when_the_retry_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, "old").unwrap();
        make_read_only(&path);
        let file = OpenOptions::new()
            .access_mode(DELETE | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES | SYNCHRONIZE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
            .unwrap();
        let calls = std::cell::Cell::new(0);
        let error = delete_classic_with(&file, |file| {
            calls.set(calls.get() + 1);
            // The retry comes after the attribute was cleared.
            let read_only = attribute_tag(file).unwrap().file_attributes & FILE_ATTRIBUTE_READONLY;
            assert_eq!(read_only != 0, calls.get() == 1, "call {}", calls.get());
            Err(io::Error::from_raw_os_error(ERROR_ACCESS_DENIED))
        })
        .unwrap_err();
        assert_eq!(
            (calls.get(), error.raw_os_error()),
            (2, Some(ERROR_ACCESS_DENIED))
        );
        drop(file);
        assert!(std::fs::metadata(&path).unwrap().permissions().readonly());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
    }

    #[test]
    fn set_attributes_writes_only_settable_attributes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, "x").unwrap();
        let file = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES | SYNCHRONIZE)
            .open(&path)
            .unwrap();
        let attributes = attribute_tag(&file).unwrap().file_attributes;
        assert_eq!(attributes & FILE_ATTRIBUTE_DIRECTORY, 0);
        let wanted = attributes | FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_DIRECTORY;
        // Unmasked, the directory bit on a file is refused...
        let mut unmasked = BasicInfo {
            file_attributes: wanted,
            ..BasicInfo::default()
        };
        assert!(set_information(&file, FILE_BASIC_INFO_CLASS, &mut unmasked).is_err());
        // ...masked, it is dropped and the rest is written.
        set_attributes(&file, wanted).unwrap();
        let set = attribute_tag(&file).unwrap().file_attributes;
        assert_ne!(set & FILE_ATTRIBUTE_READONLY, 0);
        assert_eq!(set & FILE_ATTRIBUTE_DIRECTORY, 0);
        set_attributes(&file, attributes).unwrap();
    }

    /// Denies writing (data, appending, extended attributes, attributes) to everyone, as
    /// `icacls /deny *S-1-1-0:(WD,AD,WEA,WA)` does.
    fn deny_writing(path: &Path) {
        let status = std::process::Command::new("icacls")
            .arg(path)
            .args(["/deny", "*S-1-1-0:(WD,AD,WEA,WA)"])
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "icacls failed");
    }

    #[test]
    fn a_file_whose_acl_denies_writing_still_reads_and_is_removed() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        let path = dir.path().join("d").join("f");
        std::fs::write(&path, "inside").unwrap();
        deny_writing(&path);
        assert!(
            std::fs::OpenOptions::new().write(true).open(&path).is_err(),
            "the ACL didn't deny writing"
        );
        let read = read(dir.path(), Path::new("d/f"), &mut no_step).unwrap();
        assert!(
            matches!(&read, Some(Leaf::File { bytes, .. }) if bytes == b"inside"),
            "{read:?}"
        );
        // Deleting needs only `DELETE`: the open falls back to it without `FILE_WRITE_ATTRIBUTES`.
        assert!(remove(dir.path(), Path::new("d/f"), &mut no_step).unwrap());
        assert!(std::fs::symlink_metadata(&path).is_err());
    }

    /// The work tree through the admin share `\\localhost\C$\…` (SMB): the reopen of an
    /// existing file, by its ID or, where the share refuses that, by name, still works.
    #[test]
    fn a_work_tree_on_an_smb_share_is_written_and_read() {
        let dir = TempDir::new().unwrap();
        let local = std::fs::canonicalize(dir.path()).unwrap();
        // `\\?\C:\x` -> `\\localhost\C$\x`.
        let local = local.to_string_lossy();
        let Some(rest) = local.strip_prefix(r"\\?\") else {
            panic!("not a drive path: {local}")
        };
        let (drive, rest) = rest.split_at(1);
        let share = PathBuf::from(format!(r"\\localhost\{drive}${}", &rest[1..]));
        if std::fs::metadata(&share).is_err() {
            assert!(
                std::env::var_os("CI").is_none(),
                "the admin share isn't reachable on CI: {}",
                share.display()
            );
            eprintln!("SKIP: no admin share at {}", share.display());
            return;
        }
        assert!(write(&share, Path::new("d/f"), b"one", &mut no_step).unwrap());
        // Existing now: written through the reopened handle.
        assert!(write(&share, Path::new("d/f"), b"two", &mut no_step).unwrap());
        let read = read(&share, Path::new("d/f"), &mut no_step).unwrap();
        assert!(
            matches!(&read, Some(Leaf::File { bytes, .. }) if bytes == b"two"),
            "{read:?}"
        );
        assert_eq!(
            std::fs::read(dir.path().join("d").join("f")).unwrap(),
            b"two"
        );
    }

    /// A OneDrive-like file is reopened by its file ID: the result is the same file.
    #[test]
    fn open_by_id_opens_the_same_file() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f"), "inside").unwrap();
        let root = open_pinned(dir.path(), FILE_LIST_DIRECTORY).unwrap();
        let first = nt_open(
            &root,
            OsStr::new("f"),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT,
            0,
        )
        .unwrap();
        let id = file_id(&first).unwrap();
        // Renamed meanwhile: opened by identity, not by the name.
        std::fs::rename(dir.path().join("f"), dir.path().join("g")).unwrap();
        std::fs::write(dir.path().join("f"), "swapped").unwrap();
        let mut second =
            open_by_id(&root, &id.file_id, GENERIC_READ, FILE_NON_DIRECTORY_FILE).unwrap();
        assert!(file_id(&second).unwrap() == id);
        let mut text = String::new();
        second.read_to_string(&mut text).unwrap();
        assert_eq!(text, "inside");
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn nt_structures_have_the_documented_layout() {
        assert_eq!(std::mem::size_of::<UnicodeString>(), 16);
        assert_eq!(std::mem::size_of::<ObjectAttributes>(), 48);
        assert_eq!(std::mem::size_of::<IoStatusBlock>(), 16);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn rename_info_has_the_documented_layout() {
        // `FILE_RENAME_INFORMATION` on 64-bit: Flags at 0, RootDirectory at 8, FileNameLength
        // at 16, FileName at 20; `sizeof` 24.
        assert_eq!(std::mem::size_of::<RenameInfo>(), 24);
        assert_eq!(RENAME_NAME_OFFSET, 20);
        let root = 0x1234_5678_usize as *mut c_void;
        let name: Vec<u16> = "f.txt".encode_utf16().collect();
        let buf = rename_info(3, root, &name).unwrap();
        // SAFETY: reading back the bytes of a plain `Vec<u64>`.
        let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len() * 8) };
        assert_eq!(bytes.len(), 32);
        assert_eq!(&bytes[0..4], &3u32.to_ne_bytes());
        assert_eq!(&bytes[8..16], &0x1234_5678_u64.to_ne_bytes());
        assert_eq!(&bytes[16..20], &10u32.to_ne_bytes());
        let written: Vec<u16> = bytes[20..30]
            .chunks(2)
            .map(|unit| u16::from_ne_bytes([unit[0], unit[1]]))
            .collect();
        assert_eq!(written, name);
        // A one-letter name still passes the whole structure's size.
        assert_eq!(rename_info(1, root, &[0x66]).unwrap().len() * 8, 24);
        assert!(rename_info(1, root, &[]).is_err());
    }

    #[test]
    fn rename_falls_back_to_the_classic_class_where_posix_renames_are_refused() {
        for refusal in [
            ERROR_INVALID_PARAMETER,
            ERROR_NOT_SUPPORTED,
            ERROR_INVALID_FUNCTION,
        ] {
            let mut calls = Vec::new();
            rename_in_parent_with(|class, flags| {
                calls.push((class, flags));
                if class == FILE_RENAME_INFORMATION_EX_CLASS {
                    Err(io::Error::from_raw_os_error(refusal))
                } else {
                    Ok(())
                }
            })
            .unwrap();
            assert_eq!(
                calls,
                [
                    (FILE_RENAME_INFORMATION_EX_CLASS, 0x3),
                    (FILE_RENAME_INFORMATION_CLASS, 1)
                ]
            );
        }
        // Any other failure is the answer.
        let mut calls = 0;
        let error = rename_in_parent_with(|_, _| {
            calls += 1;
            Err(io::Error::from_raw_os_error(ERROR_SHARING_VIOLATION))
        })
        .unwrap_err();
        assert_eq!(
            (calls, error.raw_os_error()),
            (1, Some(ERROR_SHARING_VIOLATION))
        );
    }

    #[test]
    fn rename_in_parent_replaces_a_file_in_the_held_folder() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f"), "old").unwrap();
        let held = open_pinned(dir.path(), FILE_LIST_DIRECTORY).unwrap();
        let mut temp = nt_open(
            &held,
            OsStr::new("t"),
            GENERIC_WRITE | DELETE,
            FILE_SHARE_READ,
            FILE_CREATE,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT,
            FILE_ATTRIBUTE_NORMAL,
        )
        .unwrap();
        temp.write_all(b"new").unwrap();
        rename_in_parent(&temp, &held, OsStr::new("f")).unwrap();
        drop(temp);
        assert_eq!(names_in(dir.path()), ["f"]);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("f")).unwrap(),
            "new"
        );
    }

    #[test]
    fn temp_names_are_short_hidden_and_distinct() {
        let long: String = "x".repeat(200);
        let name = temp_name(OsStr::new(&long), 7).into_string().unwrap();
        assert!(name.starts_with(&format!(".{}.browsitory-", "x".repeat(64))));
        assert!(name.ends_with("-7.tmp"));
        assert_ne!(temp_name(OsStr::new("f"), 1), temp_name(OsStr::new("f"), 2));
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
