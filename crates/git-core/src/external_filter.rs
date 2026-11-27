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
//!
//! The app's status reads mark such files ([`annotate`]) and `head_info` names the drivers the
//! repository's attributes use ([`named_drivers`]), so the app can say why it refuses them
//! (#1087).

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;
use std::rc::Rc;

use git2::{AttrCheckFlags, AttrValue, Repository};
use sha2::{Digest, Sha256};

use crate::status::{StatusEntry, StatusKind};

/// A path git converts on its way in and out of the repository with something Browsitory
/// can't run, so it refuses the change: an external filter driver (#1063) or a
/// `working-tree-encoding` (#1156).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExternalFilter {
    #[error(
        "'{path}' uses the '{driver}' filter (for example Git LFS or git-crypt), which \
         Browsitory can't run, so it refused this change. Use git on the command line for it \
         instead."
    )]
    Driver { path: String, driver: String },
    #[error(
        "'{path}' uses working-tree-encoding={encoding}, which Browsitory can't convert, so it \
         refused this change. Use git on the command line for it instead."
    )]
    Encoding { path: String, encoding: String },
}

/// The configured driver names, as `filter=<name>` spells them (the config subsection,
/// case-sensitive), whether the attribute files name a `working-tree-encoding` to convert to,
/// and a handle of the repository's own to look attributes up with: a checkout's callbacks
/// can't borrow the caller's (`stash_apply` takes it mutably).
#[derive(Default)]
pub(crate) struct Drivers {
    names: BTreeSet<Vec<u8>>,
    /// The drivers whose clean command is Git LFS's (see [`is_git_lfs`]).
    git_lfs: BTreeSet<Vec<u8>>,
    /// The attribute files name an encoding other than UTF-8 (#1156); without one no path's
    /// encoding is looked up.
    encodings: bool,
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
        let mut git_lfs = BTreeSet::new();
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
            // git runs `process` instead of `clean` when both are set.
            let clean = ["process", "clean"].iter().find_map(|var| {
                config
                    .get_bytes(&format!("filter.{text}.{var}"))
                    .ok()
                    .filter(|value| !value.is_empty())
                    .map(<[u8]>::to_vec)
            });
            if clean.as_deref().is_some_and(is_git_lfs) {
                git_lfs.insert(name.clone());
            }
            if has_command || required {
                drivers.insert(name);
            }
        }
        // A bare repository has no work tree to filter into or from.
        let encodings = repo.workdir().is_some() && !named_encodings(repo).is_empty();
        let lookup = match repo.workdir() {
            // The git folder when the work tree has no `.git` of its own (`core.worktree`).
            Some(workdir) if !drivers.is_empty() || encodings => {
                Some(Repository::open(workdir).or_else(|_| Repository::open(repo.path()))?)
            }
            _ => None,
        };
        Ok(Self {
            names: drivers,
            git_lfs,
            encodings,
            lookup,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.names.is_empty() && !self.encodings
    }

    /// The refusal for `path` (relative, `/`-separated) when its `filter` attribute names one of
    /// the drivers, or else its `working-tree-encoding` is one git would convert to. The work
    /// tree's `.gitattributes` come first, then the index's, as `git add` reads them.
    pub(crate) fn refusal(&self, path: &Path) -> Result<Option<ExternalFilter>, git2::Error> {
        let Some(repo) = &self.lookup else {
            return Ok(None);
        };
        let shown = || path.to_string_lossy().replace('\\', "/");
        if !self.names.is_empty() {
            let value = repo.get_attr_bytes(path, "filter", AttrCheckFlags::FILE_THEN_INDEX)?;
            if let Some(name) = attr_text(value).filter(|name| self.names.contains(*name)) {
                return Ok(Some(ExternalFilter::Driver {
                    path: shown(),
                    driver: String::from_utf8_lossy(name).into_owned(),
                }));
            }
        }
        if self.encodings {
            let value = repo.get_attr_bytes(
                path,
                "working-tree-encoding",
                AttrCheckFlags::FILE_THEN_INDEX,
            )?;
            if let Some(encoding) = attr_text(value).filter(|value| converts_to(value)) {
                return Ok(Some(ExternalFilter::Encoding {
                    path: shown(),
                    encoding: String::from_utf8_lossy(encoding).into_owned(),
                }));
            }
        }
        Ok(None)
    }
}

/// An attribute's value when it is set to one (not just set, unset or unspecified).
fn attr_text(value: Option<&[u8]>) -> Option<&[u8]> {
    match AttrValue::from_bytes(value) {
        AttrValue::String(text) => Some(text.as_bytes()),
        AttrValue::Bytes(text) => Some(text),
        _ => None,
    }
}

/// Whether git converts a file to the `working-tree-encoding` `value`: any but an empty one
/// and UTF-8 (git's `is_encoding_utf8`: `UTF-8` or `UTF8`, any case). A bare
/// `working-tree-encoding` without a value makes git fail; it isn't counted.
fn converts_to(value: &[u8]) -> bool {
    !value.is_empty()
        && !value.eq_ignore_ascii_case(b"UTF-8")
        && !value.eq_ignore_ascii_case(b"UTF8")
}

/// Whether a driver's clean (or `process`) command runs Git LFS: its first word is a
/// `git-lfs` program, as `git lfs install` writes it (`git-lfs filter-process`).
fn is_git_lfs(command: &[u8]) -> bool {
    let program = command
        .split(|byte| byte.is_ascii_whitespace())
        .find(|word| !word.is_empty())
        .unwrap_or_default();
    let name = program
        .rsplit(|&byte| byte == b'/' || byte == b'\\')
        .next()
        .unwrap_or_default();
    name == b"git-lfs" || name == b"git-lfs.exe"
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

pub(crate) fn refuse_with<E, S>(
    drivers: &Drivers,
    paths: impl IntoIterator<Item = S>,
) -> Result<(), E>
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

/// Marks each unstaged entry whose work-tree file a configured driver filters with the driver's
/// name (#1087): Browsitory compares and would stage its raw bytes, where git runs the driver.
/// A deletion reads no file and a symbolic link is never filtered, so neither is marked; staged
/// entries compare two stored blobs. A modified Git LFS file whose content matches its stored
/// pointer is dropped: the driver's clean output would be that pointer, so git sees no change
/// ([`lfs_unchanged`]). Any error on the way leaves an entry as it is, never dropped. An entry
/// whose file git converts to a `working-tree-encoding` is marked with the encoding the same way
/// (#1156).
pub(crate) fn annotate(repo: &Repository, entries: &mut Vec<StatusEntry>) {
    let Ok(drivers) = Drivers::load(repo) else {
        return;
    };
    let Some(workdir) = repo.workdir().filter(|_| !drivers.is_empty()) else {
        return;
    };
    let index = repo.index().ok();
    entries.retain_mut(|entry| {
        // A deletion or an untracked folder has no regular file at its path: `is_file` below.
        if entry.staged {
            return true;
        }
        let Ok(Some(refusal)) = drivers.refusal(Path::new(&entry.path)) else {
            return true;
        };
        let on_disk = workdir.join(&entry.path);
        let is_file = on_disk
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_file());
        if !is_file {
            return true;
        }
        match refusal {
            ExternalFilter::Driver { driver, .. } => {
                let unchanged = entry.kind == StatusKind::Modified
                    && drivers.git_lfs.contains(driver.as_bytes())
                    && index
                        .as_ref()
                        .is_some_and(|index| lfs_unchanged(repo, index, &entry.path, &on_disk));
                if unchanged {
                    return false;
                }
                entry.external_filter = Some(driver);
            }
            ExternalFilter::Encoding { encoding, .. } => {
                entry.working_tree_encoding = Some(encoding);
            }
        }
        true
    });
}

/// The longest stored blob read as a possible LFS pointer; real pointers are about 130 bytes.
const MAX_POINTER_LEN: usize = 1024;

/// Whether the file at `on_disk` is what the stage-0 index entry of `path` stores as a canonical
/// Git LFS pointer: `size` bytes whose SHA-256 is `oid`. git-lfs's clean output for that file is
/// then exactly the stored blob (git runs the driver before its CRLF and `ident` conversions,
/// which leave a pointer as it is).
fn lfs_unchanged(repo: &Repository, index: &git2::Index, path: &str, on_disk: &Path) -> bool {
    let Some(entry) = index.get_path(Path::new(path), 0) else {
        return false;
    };
    let small = repo
        .odb()
        .and_then(|odb| odb.read_header(entry.id))
        .is_ok_and(|(len, _)| len <= MAX_POINTER_LEN);
    if !small {
        return false;
    }
    let Ok(blob) = repo.find_blob(entry.id) else {
        return false;
    };
    let Some((oid, size)) = canonical_pointer(blob.content()) else {
        return false;
    };
    let Ok(mut file) = std::fs::File::open(on_disk) else {
        return false;
    };
    if !file.metadata().is_ok_and(|meta| meta.len() == size) {
        return false;
    }
    // One byte past `size`, so a file that grew since its length was read doesn't match.
    let mut hasher = HashWriter(Sha256::new());
    let Ok(read) = std::io::copy(&mut (&mut file).take(size + 1), &mut hasher) else {
        return false;
    };
    let digest: String = hasher
        .0
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    read == size && digest == oid
}

/// Streams a file into the hash (`sha2` 0.11 has no `io::Write` of its own).
struct HashWriter(Sha256);

impl std::io::Write for HashWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The first line of every Git LFS pointer (a spec URL, never contacted).
const POINTER_VERSION: &str = "version https://git-lfs.github.com/spec/v1\n";

/// The `oid` and `size` of a pointer exactly as git-lfs writes one, without extension lines:
/// `version …\noid sha256:<64 lowercase hex>\nsize <decimal>\n`.
fn canonical_pointer(content: &[u8]) -> Option<(String, u64)> {
    let text = std::str::from_utf8(content).ok()?;
    let rest = text
        .strip_prefix(POINTER_VERSION)?
        .strip_prefix("oid sha256:")?;
    let (oid, rest) = rest.split_once('\n')?;
    let size = rest.strip_prefix("size ")?.strip_suffix('\n')?;
    // An `oid` that isn't 64 lowercase hex digits never equals the digest it is compared with.
    let size: u64 = size.parse().ok()?;
    // `parse` takes `+1` and `01`; git-lfs writes neither.
    (format!("{POINTER_VERSION}oid sha256:{oid}\nsize {size}\n") == text)
        .then(|| (oid.to_string(), size))
}

/// The configured drivers that the repository's attribute files name in a `filter=<name>`
/// (#1087), sorted, each once (see [`attribute_files`]). Without a configured driver no
/// attribute file is read for it.
pub(crate) fn named_drivers(repo: &Repository) -> Vec<String> {
    let Ok(drivers) = Drivers::load(repo) else {
        return Vec::new();
    };
    if drivers.names.is_empty() {
        return Vec::new();
    }
    let mut named = BTreeSet::new();
    for text in &attribute_files(repo) {
        for name in attribute_values(text, b"filter=") {
            if drivers.names.contains(name) {
                named.insert(String::from_utf8_lossy(name).into_owned());
            }
        }
    }
    named.into_iter().collect()
}

/// The `working-tree-encoding` values git converts to (not UTF-8) that the repository's
/// attribute files name (#1156), sorted, each once (see [`attribute_files`]).
pub(crate) fn named_encodings(repo: &Repository) -> Vec<String> {
    let mut named = BTreeSet::new();
    for text in &attribute_files(repo) {
        for value in attribute_values(text, b"working-tree-encoding=") {
            if converts_to(value) {
                named.insert(String::from_utf8_lossy(value).into_owned());
            }
        }
    }
    named.into_iter().collect()
}

/// The contents of the repository's attribute files: every `.gitattributes` the index tracks
/// (the work tree's file, else the stored one, as `FILE_THEN_INDEX` reads them), the work tree's
/// top-level one even if untracked, `info/attributes` and `core.attributesFile`. None in a
/// bare repository. Unreadable files are skipped.
fn attribute_files(repo: &Repository) -> Vec<Vec<u8>> {
    let Some(workdir) = repo.workdir() else {
        return Vec::new();
    };
    let mut files: Vec<Vec<u8>> = Vec::new();
    let mut tracked = BTreeSet::new();
    if let Ok(index) = repo.index() {
        for entry in index.iter() {
            let path = entry.path;
            let attributes = path == b".gitattributes" || path.ends_with(b"/.gitattributes");
            if !attributes || !tracked.insert(path.clone()) {
                continue;
            }
            let Ok(relative) = std::str::from_utf8(&path) else {
                continue;
            };
            let on_disk = workdir.join(relative);
            match on_disk.symlink_metadata() {
                Ok(meta) if meta.file_type().is_file() => {
                    files.extend(std::fs::read(&on_disk).ok());
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let stored = repo.find_blob(entry.id).ok();
                    files.extend(stored.map(|blob| blob.content().to_vec()));
                }
                _ => {}
            }
        }
    }
    let mut others = Vec::new();
    if !tracked.contains(&b".gitattributes"[..]) {
        others.push(workdir.join(".gitattributes"));
    }
    others.push(repo.commondir().join("info").join("attributes"));
    if let Ok(path) = repo
        .config()
        .and_then(|config| config.get_path("core.attributesFile"))
    {
        others.push(path);
    }
    for path in others {
        if path.symlink_metadata().is_ok_and(|meta| !meta.is_dir()) {
            files.extend(std::fs::read(&path).ok());
        }
    }
    files
}

/// The `<value>` of every `<prefix><value>` (`filter=`, `working-tree-encoding=`) in an
/// attributes file: the words after each line's pattern (or `[attr]` macro name); `#` lines are
/// comments.
fn attribute_values<'t>(text: &'t [u8], prefix: &[u8]) -> Vec<&'t [u8]> {
    let mut names = Vec::new();
    for line in text.split(|&byte| byte == b'\n') {
        let line = line.trim_ascii();
        if line.is_empty() || line.starts_with(b"#") {
            continue;
        }
        let rest = match line.strip_prefix(b"\"") {
            // A quoted pattern ends at the first quote that no backslash escapes.
            Some(quoted) => {
                let mut escaped = false;
                let end = quoted.iter().position(|&byte| {
                    let end = byte == b'"' && !escaped;
                    escaped = byte == b'\\' && !escaped;
                    end
                });
                match end {
                    Some(end) => &quoted[end + 1..],
                    None => continue,
                }
            }
            None => {
                let end = line
                    .iter()
                    .position(u8::is_ascii_whitespace)
                    .unwrap_or(line.len());
                &line[end..]
            }
        };
        names.extend(
            rest.split(u8::is_ascii_whitespace)
                .filter_map(|word| word.strip_prefix(prefix))
                .filter(|name| !name.is_empty()),
        );
    }
    names
}
