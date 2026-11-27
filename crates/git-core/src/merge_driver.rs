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
//!
//! libgit2's checkout writes every conflicted file as a text merge, whatever its driver. After
//! an operation stops with conflicts, [`keep_ours`] writes our side instead for a file whose
//! driver doesn't merge text, as git's binary driver leaves it (#1204).

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

/// What a merge of `path` (relative, `/`-separated) uses: the `merge` attribute (`Text` when
/// set, `Unset` for `-merge`, else its name), or `merge.default`'s name without one. Attributes
/// are read as libgit2's merge reads them: the work tree's `.gitattributes`, then the index's.
/// `None`: libgit2's text merge (neither is given), or unreadable.
enum Driver {
    Text,
    Unset,
    Named(Vec<u8>),
}

fn driver(repo: &Repository, path: &std::path::Path) -> Option<Driver> {
    let value = repo
        .get_attr_bytes(path, "merge", AttrCheckFlags::FILE_THEN_INDEX)
        .ok()?;
    Some(match AttrValue::from_bytes(value) {
        AttrValue::True => Driver::Text,
        AttrValue::False => Driver::Unset,
        AttrValue::Bytes(name) => Driver::Named(name.to_vec()),
        AttrValue::String(name) => Driver::Named(name.as_bytes().to_vec()),
        AttrValue::Unspecified => Driver::Named(
            repo.config()
                .ok()?
                .snapshot()
                .ok()?
                .get_bytes("merge.default")
                .ok()?
                .to_vec(),
        ),
    })
}

/// The driver of `names` a merge of `path` would use ([`driver`]).
fn driver_for(repo: &Repository, names: &BTreeSet<Vec<u8>>, path: &str) -> Option<String> {
    match driver(repo, std::path::Path::new(path))? {
        Driver::Named(name) if names.contains(&name) => {
            Some(String::from_utf8_lossy(&name).into_owned())
        }
        _ => None,
    }
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

/// Whether git leaves our side of a conflicted `path` in the work tree: its [`driver`] is unset
/// (`-merge`), `binary`, or one of `names`, a driver Browsitory doesn't run. `None` (the config
/// can't be read) counts every name but libgit2's `text` and `union`, as [`apply`] leaves such a
/// file conflicted.
fn keeps_ours(
    repo: &Repository,
    names: Option<&BTreeSet<Vec<u8>>>,
    path: &std::path::Path,
) -> bool {
    match driver(repo, path) {
        Some(Driver::Unset) => true,
        Some(Driver::Named(name)) => match name.as_slice() {
            b"binary" => true,
            b"text" | b"union" => false,
            name => names.is_none_or(|names| names.contains(name)),
        },
        Some(Driver::Text) | None => false,
    }
}

/// After a merge, pick, rebase step or stash apply stopped with conflicts: each file both sides
/// changed whose driver keeps ours ([`keeps_ours`]) gets our side (stage 2) in the work tree,
/// written through the checkout filters, in place of libgit2's text merge (#1204). The index
/// keeps the three stages. Best effort: if that checkout fails, the file keeps the text merge
/// and is still conflicted.
pub(crate) fn keep_ours(repo: &Repository) {
    let _ = write_ours(repo);
}

fn write_ours(repo: &Repository) -> Result<(), git2::Error> {
    const FILE: u32 = 0o100644;
    const EXECUTABLE: u32 = 0o100755;
    let index = repo.index()?;
    if !index.has_conflicts() {
        return Ok(());
    }
    let names = configured_names(repo).ok();
    let mut paths = Vec::new();
    for conflict in index.conflicts()? {
        let conflict = conflict?;
        let (Some(our), Some(their)) = (conflict.our, conflict.their) else {
            continue;
        };
        let files = [our.mode, their.mode]
            .iter()
            .all(|mode| matches!(*mode, FILE | EXECUTABLE));
        let path = crate::worktree_guard::path_from_bytes(&our.path);
        if files && keeps_ours(repo, names.as_ref(), &path) {
            paths.push(path);
        }
    }
    if paths.is_empty() {
        return Ok(());
    }
    // The operation's own checkout has just written these files from the same index.
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force().use_ours(true).disable_pathspec_match(true);
    for path in &paths {
        checkout.path(path);
    }
    let filters = crate::hardlink::on_checkout(repo, &mut checkout);
    repo.checkout_index(None, Some(&mut checkout))?;
    filters
        .check()
        .map_err(|refusal| git2::Error::from_str(&refusal.to_string()))
}
