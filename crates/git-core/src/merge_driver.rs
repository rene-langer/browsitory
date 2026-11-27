//! Custom merge drivers (#1165).
//!
//! A path's `merge` attribute (or `merge.default` for a path without one) can name a driver
//! configured as `merge.<name>.driver`, a command git runs to merge the file: `merge=ours` with
//! `merge.ours.driver = true`, lock file and project file mergers. libgit2 knows only its
//! built-in drivers (`text`, `binary`, `union`) and would merge every other file as text.
//! Browsitory doesn't run driver commands (owner decision): such a file, when both sides changed
//! it, is left as a conflict for the user to resolve, and every other file merges as before.
//!
//! [`register`] puts a driver for every name libgit2 doesn't know (its `*` wildcard) into
//! libgit2's process-wide registry, so every merge libgit2 makes for the app sees it: merge,
//! squash, cherry-pick, revert, rebase steps, stash apply and pop, pull, and the merges in memory
//! the app's checks compute. libgit2 calls a driver only for a file both sides changed
//! differently, as git does. The driver leaves a configured driver's file conflicted
//! (`GIT_EMERGECONFLICT`) and hands any other name to libgit2's text merge (`GIT_PASSTHROUGH`),
//! as git merges a file whose driver isn't configured as text.
//!
//! The app's status reads name the driver of a conflicted file ([`annotate`]), so the app can
//! say why it is conflicted.

use std::collections::BTreeSet;
use std::ffi::{c_char, c_int, c_uint, c_void, CStr};
use std::sync::Once;

use git2::{AttrCheckFlags, AttrValue, Binding, Repository};

use crate::status::{StatusEntry, StatusKind};

/// libgit2's `git_merge_driver` (`git2/sys/merge.h`), which git2 doesn't wrap.
#[repr(C)]
struct RawDriver {
    version: c_uint,
    initialize: Option<unsafe extern "C" fn(*mut RawDriver) -> c_int>,
    shutdown: Option<unsafe extern "C" fn(*mut RawDriver)>,
    apply: Option<
        unsafe extern "C" fn(
            *mut RawDriver,
            *mut *const c_char,
            *mut u32,
            *mut c_void,
            *const c_char,
            *const c_void,
        ) -> c_int,
    >,
}

extern "C" {
    fn git_merge_driver_register(name: *const c_char, driver: *mut RawDriver) -> c_int;
    fn git_merge_driver_source_repo(source: *const c_void) -> *mut c_void;
}

const GIT_MERGE_DRIVER_VERSION: c_uint = 1;
const GIT_EMERGECONFLICT: c_int = -24;
const GIT_PASSTHROUGH: c_int = -30;

/// Registers the wildcard driver once per process. libgit2 must be initialised (any repository
/// opened, or `repo::configure_libgit2` called) before.
pub(crate) fn register() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let driver = Box::leak(Box::new(RawDriver {
            version: GIT_MERGE_DRIVER_VERSION,
            initialize: None,
            shutdown: None,
            apply: Some(apply),
        }));
        // SAFETY: the name is a NUL-terminated literal and the driver lives for the rest of the
        // process, as libgit2 requires; libgit2 copies the name.
        unsafe {
            git_merge_driver_register(c"*".as_ptr(), driver);
        }
    });
}

/// libgit2's call for a file both sides changed whose driver name it doesn't know.
unsafe extern "C" fn apply(
    _driver: *mut RawDriver,
    _path_out: *mut *const c_char,
    _mode_out: *mut u32,
    _merged_out: *mut c_void,
    name: *const c_char,
    source: *const c_void,
) -> c_int {
    // A panic must not unwind into libgit2; a config that can't be read must not merge as text.
    // Either way the file is left conflicted.
    let configured = std::panic::catch_unwind(|| {
        // SAFETY: libgit2 passes the driver name as a NUL-terminated string and a live source
        // whose repository outlives this call; the repository is borrowed, never freed here.
        let name = unsafe { CStr::from_ptr(name) }.to_bytes();
        let raw = unsafe { git_merge_driver_source_repo(source) };
        let repo = std::mem::ManuallyDrop::new(unsafe { Repository::from_raw(raw.cast()) });
        configured_names(&repo).map_or(true, |names| names.contains(name))
    });
    if configured.unwrap_or(true) {
        GIT_EMERGECONFLICT
    } else {
        GIT_PASSTHROUGH
    }
}

/// The names of the drivers `merge.<name>.driver` sets, at any config level, to anything (git
/// refuses a driver without a command too).
fn configured_names(repo: &Repository) -> Result<BTreeSet<Vec<u8>>, git2::Error> {
    let config = repo.config()?.snapshot()?;
    let mut names = BTreeSet::new();
    let mut entries = config.entries(Some(r"^merge\..+\.driver$"))?;
    while let Some(entry) = entries.next() {
        let entry = entry?;
        if let Some(name) = entry
            .name_bytes()
            .strip_prefix(b"merge.")
            .and_then(|rest| rest.strip_suffix(b".driver"))
        {
            names.insert(name.to_vec());
        }
    }
    Ok(names)
}

/// The driver of `names` a merge of `path` (relative, `/`-separated) would use: the `merge`
/// attribute's name, or `merge.default`'s when the attribute isn't given. Attributes are read as
/// libgit2's merge reads them: the work tree's `.gitattributes`, then the index's.
fn driver_for(repo: &Repository, names: &BTreeSet<Vec<u8>>, path: &str) -> Option<String> {
    let value = repo
        .get_attr_bytes(
            std::path::Path::new(path),
            "merge",
            AttrCheckFlags::FILE_THEN_INDEX,
        )
        .ok()?;
    let name = match AttrValue::from_bytes(value) {
        AttrValue::Bytes(name) => name.to_vec(),
        AttrValue::String(name) => name.as_bytes().to_vec(),
        AttrValue::Unspecified => repo
            .config()
            .ok()?
            .snapshot()
            .ok()?
            .get_bytes("merge.default")
            .ok()?
            .to_vec(),
        AttrValue::True | AttrValue::False => return None,
    };
    names
        .contains(&name)
        .then(|| String::from_utf8_lossy(&name).into_owned())
}

/// Names the configured driver of every conflicted entry that has one
/// (`StatusEntry::merge_driver`).
pub(crate) fn annotate(repo: &Repository, entries: &mut [StatusEntry]) {
    let Ok(names) = configured_names(repo) else {
        return;
    };
    for entry in entries
        .iter_mut()
        .filter(|entry| entry.kind == StatusKind::Conflicted)
    {
        entry.merge_driver = driver_for(repo, &names, &entry.path);
    }
}
