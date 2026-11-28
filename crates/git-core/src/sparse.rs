//! The sparse checkout of a work tree, read as git reads it (`dir.c`): `core.sparseCheckout`,
//! `core.sparseCheckoutCone` and the work tree's own `info/sparse-checkout` (a linked worktree
//! has its file under `.git/worktrees/<name>/`). libgit2 knows nothing of it (#1172).

use std::collections::HashSet;

use crate::wildmatch::{is_glob_special, wildmatch, Flags};

/// Whether the sparse checkout of `repo`'s work tree leaves the file `path` (`/`-separated) out.
/// A test seam (#1187): the tests compare it with the marks git sets for many patterns.
#[doc(hidden)]
pub fn leaves_out(repo: &git2::Repository, path: &str) -> Result<bool, git2::Error> {
    Ok(Sparse::read(repo)?.excludes(path.as_bytes()))
}

/// Which paths a work tree's sparse checkout holds.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Sparse {
    /// No sparse checkout, or one without a readable patterns file: git applies none.
    Off,
    /// Cone mode, with patterns of the cone format.
    Cone(Cone),
    /// Other patterns (gitignore-style).
    Patterns(Patterns),
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
        let ignore_case = on("core.ignorecase");
        let cone = on("core.sparseCheckoutCone")
            .then(|| Cone::parse(&bytes, ignore_case))
            .flatten();
        Ok(cone.map_or_else(
            || Self::Patterns(Patterns::parse(&bytes, ignore_case)),
            Self::Cone,
        ))
    }

    /// Whether `path` (a file, `/`-separated) lies outside the sparse checkout.
    pub(crate) fn excludes(&self, path: &[u8]) -> bool {
        match self {
            Self::Off => false,
            Self::Cone(cone) => !cone.contains(path),
            Self::Patterns(patterns) => !patterns.contains(path),
        }
    }
}

/// Non-cone patterns as git reads them (`add_patterns_from_buffer`, `parse_path_pattern`).
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Patterns {
    list: Vec<Pattern>,
    /// `core.ignorecase`: git compares ignoring ASCII case.
    ignore_case: bool,
}

/// One pattern line, without its `!` and trailing `/`.
#[derive(Debug, PartialEq, Eq)]
struct Pattern {
    text: Vec<u8>,
    /// `!`: a path it matches is left out.
    negative: bool,
    /// A trailing `/`: it matches folders only.
    folder_only: bool,
    /// No `/` but a trailing one: it matches a path's last name, at any depth.
    basename: bool,
    /// `*` followed by no other glob character: the name ends with the rest.
    ends_with: bool,
    /// The length of the part before the first glob character.
    literal: usize,
}

impl Patterns {
    fn parse(file: &[u8], ignore_case: bool) -> Self {
        let file = file.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(file);
        let list = file
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty() && line[0] != b'#')
            .map(|line| {
                let line = line.strip_suffix(b"\r").unwrap_or(line);
                Pattern::parse(trim_trailing_spaces(line))
            })
            .collect();
        Self { list, ignore_case }
    }

    /// git's `path_in_sparse_checkout` with non-cone patterns, for a file: the last pattern that
    /// matches it decides; when none does, its folder's verdict counts, up to the top, where a
    /// path no pattern decides is left out.
    fn contains(&self, path: &[u8]) -> bool {
        let mut end = path.len();
        let mut folder = false;
        while end > 0 {
            let slash = path[..end].iter().rposition(|byte| *byte == b'/');
            let name = slash.map_or(0, |slash| slash + 1);
            if let Some(negative) = self.last_match(&path[..end], name, folder) {
                return !negative;
            }
            folder = true;
            end = slash.unwrap_or(0);
        }
        false
    }

    /// git's `last_matching_pattern_from_list`: whether the last pattern that matches `path`
    /// (whose last name starts at `name`) is negative, `None` when none matches.
    fn last_match(&self, path: &[u8], name: usize, folder: bool) -> Option<bool> {
        self.list
            .iter()
            .rev()
            .filter(|pattern| folder || !pattern.folder_only)
            .find(|pattern| {
                if pattern.basename {
                    self.matches_name(pattern, &path[name..])
                } else {
                    self.matches_path(pattern, path)
                }
            })
            .map(|pattern| pattern.negative)
    }

    /// git's `match_basename`.
    fn matches_name(&self, pattern: &Pattern, name: &[u8]) -> bool {
        let text = &pattern.text;
        if pattern.literal == text.len() {
            self.same(text, name)
        } else if pattern.ends_with {
            let suffix = &text[1..];
            name.len() >= suffix.len() && self.same(suffix, &name[name.len() - suffix.len()..])
        } else {
            wildmatch(text, name, self.flags(false))
        }
    }

    /// git's `match_pathname`, from the top (the base of `info/sparse-checkout`).
    fn matches_path(&self, pattern: &Pattern, path: &[u8]) -> bool {
        let (text, literal) = match pattern.text.strip_prefix(b"/") {
            Some(text) => (text, pattern.literal - 1),
            None => (&pattern.text[..], pattern.literal),
        };
        if literal > path.len() || !self.same(&text[..literal], &path[..literal]) {
            return false;
        }
        let (text, path) = (&text[literal..], &path[literal..]);
        if literal > 0 && text.is_empty() && path.is_empty() {
            return true;
        }
        wildmatch(text, path, self.flags(true))
    }

    fn same(&self, a: &[u8], b: &[u8]) -> bool {
        if self.ignore_case {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    }

    fn flags(&self, pathname: bool) -> Flags {
        Flags {
            pathname,
            casefold: self.ignore_case,
        }
    }
}

impl Pattern {
    /// git's `parse_path_pattern`.
    fn parse(line: &[u8]) -> Self {
        let (negative, line) = match line.strip_prefix(b"!") {
            Some(rest) => (true, rest),
            None => (false, line),
        };
        let (folder_only, text) = match line.strip_suffix(b"/") {
            Some(text) => (true, text),
            None => (false, line),
        };
        let literal = line
            .iter()
            .position(|byte| is_glob_special(*byte))
            .unwrap_or(line.len())
            .min(text.len());
        let ends_with =
            line.first() == Some(&b'*') && !line[1..].iter().any(|byte| is_glob_special(*byte));
        Self {
            negative,
            folder_only,
            basename: !text.contains(&b'/'),
            ends_with,
            literal,
            text: text.to_vec(),
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
        let full = self.fold([&b"/"[..], path].concat());
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
    let mut last_space = None;
    let mut bytes = line.iter().enumerate();
    while let Some((at, byte)) = bytes.next() {
        match byte {
            b' ' => {
                last_space.get_or_insert(at);
            }
            b'\\' => {
                if bytes.next().is_none() {
                    return line;
                }
                last_space = None;
            }
            _ => last_space = None,
        }
    }
    &line[..last_space.unwrap_or(line.len())]
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
