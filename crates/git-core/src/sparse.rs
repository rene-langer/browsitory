//! The sparse checkout of a work tree, read as git reads it (`dir.c`): `core.sparseCheckout`,
//! `core.sparseCheckoutCone` and the work tree's own `info/sparse-checkout` (a linked worktree
//! has its file under `.git/worktrees/<name>/`). libgit2 knows nothing of it (#1172).

use std::collections::HashSet;
use std::path::Path;

/// Which paths a work tree's sparse checkout holds.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Sparse {
    /// No sparse checkout, or one without a readable patterns file: git applies none.
    Off,
    /// Cone mode, with patterns of the cone format.
    Cone(Cone),
    /// Other patterns (gitignore-style), which aren't matched here.
    Patterns,
}

/// Cone-mode patterns as git's hash sets hold them (`add_pattern_to_hashsets`): folders as
/// `/a/b`, without escapes.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Cone {
    /// `/*` without a later `!/*/`: every path is in.
    full: bool,
    /// Folders whose whole content is in (`/a/b/`).
    recursive: HashSet<Vec<u8>>,
    /// Folders whose own files are in (`/a/` followed by `!/a/*/`).
    parents: HashSet<Vec<u8>>,
    /// `core.ignorecase`: git compares the folders ignoring ASCII case.
    ignore_case: bool,
}

impl Sparse {
    pub(crate) fn read(repo: &git2::Repository) -> Result<Self, git2::Error> {
        let config = repo.config()?;
        let on = |key: &str| config.get_bool(key).unwrap_or(false);
        if !on("core.sparseCheckout") {
            return Ok(Self::Off);
        }
        let Ok(bytes) = std::fs::read(repo.path().join("info").join("sparse-checkout")) else {
            return Ok(Self::Off);
        };
        if !on("core.sparseCheckoutCone") {
            return Ok(Self::Patterns);
        }
        Ok(Cone::parse(&bytes, on("core.ignorecase")).map_or(Self::Patterns, Self::Cone))
    }

    /// Whether `path` (a file, `/`-separated) lies outside the sparse checkout. With non-cone
    /// patterns, whether its file is missing from `workdir`: a path that isn't there is left
    /// out, so nothing outside the patterns is written or listed as deleted.
    pub(crate) fn excludes(&self, workdir: Option<&Path>, path: &[u8]) -> bool {
        match self {
            Self::Off => false,
            Self::Cone(cone) => !cone.contains(path),
            Self::Patterns => workdir.is_some_and(|workdir| {
                workdir
                    .join(crate::worktree_guard::path_from_bytes(path))
                    .symlink_metadata()
                    .is_err()
            }),
        }
    }
}

impl Cone {
    /// The patterns of `file`, or `None` when one doesn't fit the cone format, as git then
    /// reads them as non-cone patterns ("disabling cone pattern matching").
    fn parse(file: &[u8], ignore_case: bool) -> Option<Self> {
        let mut cone = Self {
            ignore_case,
            ..Self::default()
        };
        let file = file.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(file);
        for line in file.split(|byte| *byte == b'\n') {
            if line.is_empty() || line[0] == b'#' {
                continue;
            }
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            cone.add(trim_trailing_spaces(line))?;
        }
        Some(cone)
    }

    /// git's `add_pattern_to_hashsets` for one line.
    fn add(&mut self, line: &[u8]) -> Option<()> {
        let (negative, rest) = match line.strip_prefix(b"!") {
            Some(rest) => (true, rest),
            None => (false, line),
        };
        let (folder, pattern) = match rest.strip_suffix(b"/") {
            Some(pattern) => (true, pattern),
            None => (false, rest),
        };
        if pattern == b"/*" && negative == folder {
            // `!/*/` leaves the top level's folders out; `/*` takes everything in.
            self.full = !negative;
            return Some(());
        }
        if pattern.len() < 2 || pattern[0] != b'/' {
            return None;
        }
        if !folder || !plain_folder(pattern) {
            return None;
        }
        let name = self.key(pattern);
        if pattern.ends_with(b"/*") && pattern.len() > 2 {
            // `!/a/*/` after `/a/`: only the files directly in `a` are in.
            if !negative || !self.recursive.remove(&name) {
                return None;
            }
            self.parents.insert(name);
            return Some(());
        }
        if negative || self.parents.contains(&name) {
            return None;
        }
        self.recursive.insert(name);
        Some(())
    }

    /// A pattern's folder as the sets hold it: escapes removed, a trailing `/*` cut
    /// (`dup_and_filter_pattern`), folded under `core.ignorecase`.
    fn key(&self, pattern: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(pattern.len());
        let mut bytes = pattern.iter();
        while let Some(&byte) = bytes.next() {
            if byte == b'\\' {
                match bytes.next() {
                    Some(&escaped) => out.push(escaped),
                    None => break,
                }
            } else {
                out.push(byte);
            }
        }
        if out.len() > 2 && out.ends_with(b"/*") {
            out.truncate(out.len() - 2);
        }
        self.fold(out)
    }

    fn fold(&self, mut name: Vec<u8>) -> Vec<u8> {
        if self.ignore_case {
            name.make_ascii_lowercase();
        }
        name
    }

    /// git's `path_matches_pattern_list` in cone mode, for a file: in when it is at the top
    /// level, its folder is a parent, or it or a folder above it is recursive.
    fn contains(&self, path: &[u8]) -> bool {
        if self.full {
            return true;
        }
        let mut full = Vec::with_capacity(path.len() + 1);
        full.push(b'/');
        full.extend_from_slice(path);
        let full = self.fold(full);
        if self.recursive.contains(&full) {
            return true;
        }
        let mut end = match full.iter().rposition(|byte| *byte == b'/') {
            Some(0) | None => return true,
            Some(slash) => slash,
        };
        if self.parents.contains(&full[..end]) {
            return true;
        }
        loop {
            if self.recursive.contains(&full[..end]) {
                return true;
            }
            match full[..end].iter().rposition(|byte| *byte == b'/') {
                Some(0) | None => return false,
                Some(slash) => end = slash,
            }
        }
    }
}

/// git's `trim_trailing_spaces`: trailing spaces go, unless escaped with a backslash.
fn trim_trailing_spaces(line: &[u8]) -> &[u8] {
    let mut end = line.len();
    let mut last_space = None;
    let mut at = 0;
    while at < line.len() {
        match line[at] {
            b' ' => {
                last_space.get_or_insert(at);
            }
            b'\\' => {
                at += 1;
                if at == line.len() {
                    return line;
                }
                last_space = None;
            }
            _ => last_space = None,
        }
        at += 1;
    }
    if let Some(space) = last_space {
        end = space;
    }
    &line[..end]
}

/// No glob character in `pattern` after its leading `/`, except an escaped one or a final
/// `/*` (git's check in `add_pattern_to_hashsets`).
fn plain_folder(pattern: &[u8]) -> bool {
    let glob = |byte: u8| matches!(byte, b'*' | b'?' | b'[' | b'\\');
    (1..pattern.len()).all(|at| {
        let (prev, cur) = (pattern[at - 1], pattern[at]);
        let next = pattern.get(at + 1).copied();
        !glob(cur)
            || prev == b'\\'
            || (cur == b'\\' && next.is_some_and(glob))
            || (prev == b'/' && cur == b'*' && next.is_none())
    })
}
