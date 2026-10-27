//! Commit search: a small query language parsed once (`parse_query`) and a paged, cancellable
//! revwalk over it (`search_commits`).

use git2::{DiffLineType, DiffOptions, ErrorCode, ObjectType, Oid, Repository, Sort};
use thiserror::Error;

use crate::graph::{GraphCommit, RefDecorations};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchTerm {
    /// Message contains it, or SHA prefix when 4–40 hex chars.
    Text(String),
    /// Author name or email contains it.
    Author(String),
    /// Committer name or email contains it.
    Committer(String),
    /// Full message contains it.
    Message(String),
    /// SHA prefix, lowercase hex, 1–40 chars.
    Commit(String),
    /// libgit2 pathspec pattern.
    Path(String),
    /// An added or removed line contains it.
    Contents(String),
    /// Days since 1970-01-01, inclusive.
    After(i64),
    /// Days since 1970-01-01, inclusive.
    Before(i64),
    Branch(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub terms: Vec<SearchTerm>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageLimits {
    pub matches: usize,
    pub scan: usize,
}

impl PageLimits {
    pub const DEFAULT: PageLimits = PageLimits {
        matches: 100,
        scan: 20_000,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPage {
    pub commits: Vec<GraphCommit>,
    pub next_cursor: Option<String>,
    pub scanned: usize,
}

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("{message}")]
    InvalidQuery { message: String },
    #[error("The history changed since this search started. Search again.")]
    StaleCursor,
    #[error("Search cancelled.")]
    Cancelled,
    #[error("failed to search commits: {0}")]
    Git(#[from] git2::Error),
}

fn invalid(message: impl Into<String>) -> SearchError {
    SearchError::InvalidQuery {
        message: message.into(),
    }
}

const KEYS: [&str; 9] = [
    "author",
    "committer",
    "message",
    "commit",
    "path",
    "contents",
    "after",
    "before",
    "branch",
];

/// Parses a search query. Grammar: whitespace-separated terms, each either `key:value` (key is
/// `[A-Za-z]+`, case-insensitive) or a bare value; a value is either `"quoted"` (with `\"` and
/// `\\` escapes) or runs to the next whitespace. All terms are ANDed.
pub fn parse_query(input: &str) -> Result<SearchQuery, SearchError> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut terms = Vec::new();
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }

        // A key is a run of ASCII letters immediately followed by `:`.
        let mut end = i;
        while end < chars.len() && chars[end].is_ascii_alphabetic() {
            end += 1;
        }
        let key = if end > i && end < chars.len() && chars[end] == ':' {
            let key: String = chars[i..end].iter().collect::<String>().to_lowercase();
            if !KEYS.contains(&key.as_str()) {
                let raw: String = chars[i..end].iter().collect();
                return Err(invalid(format!(
                    "Unknown search key \"{raw}:\". Put the text in quotes to search for it literally."
                )));
            }
            i = end + 1;
            if i >= chars.len() || chars[i].is_whitespace() {
                return Err(invalid(format!("{key}: needs a value.")));
            }
            Some(key)
        } else {
            None
        };

        let value = if chars[i] == '"' {
            i += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < chars.len() {
                match chars[i] {
                    '"' => {
                        closed = true;
                        i += 1;
                        break;
                    }
                    '\\' if i + 1 < chars.len() && matches!(chars[i + 1], '"' | '\\') => {
                        value.push(chars[i + 1]);
                        i += 2;
                    }
                    c => {
                        value.push(c);
                        i += 1;
                    }
                }
            }
            if !closed {
                return Err(invalid("Missing closing quote."));
            }
            if value.is_empty() {
                return Err(invalid("Empty quotes don't match anything."));
            }
            value
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            chars[start..i].iter().collect()
        };

        terms.push(match key.as_deref() {
            None => SearchTerm::Text(value),
            Some("author") => SearchTerm::Author(value),
            Some("committer") => SearchTerm::Committer(value),
            Some("message") => SearchTerm::Message(value),
            Some("commit") => {
                if value.is_empty()
                    || value.len() > 40
                    || !value.chars().all(|c| c.is_ascii_hexdigit())
                {
                    return Err(invalid("commit: expects a hexadecimal SHA prefix."));
                }
                SearchTerm::Commit(value.to_lowercase())
            }
            Some("path") => SearchTerm::Path(value),
            Some("contents") => SearchTerm::Contents(value),
            Some("after") => SearchTerm::After(
                parse_day(&value)
                    .ok_or_else(|| invalid("after: expects a date like 2026-01-31."))?,
            ),
            Some("before") => SearchTerm::Before(
                parse_day(&value)
                    .ok_or_else(|| invalid("before: expects a date like 2026-01-31."))?,
            ),
            Some("branch") => SearchTerm::Branch(value),
            Some(other) => unreachable!("unhandled search key {other}"),
        });
    }

    if terms.is_empty() {
        return Err(invalid("Type something to search for."));
    }
    if terms
        .iter()
        .filter(|t| matches!(t, SearchTerm::Branch(_)))
        .count()
        > 1
    {
        return Err(invalid("Use at most one branch: term."));
    }
    Ok(SearchQuery { terms })
}

/// `YYYY-MM-DD` → days since 1970-01-01, or `None` when malformed or not a real calendar day.
fn parse_day(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = &value[range];
        if part.bytes().all(|b| b.is_ascii_digit()) {
            part.parse().ok()
        } else {
            None
        }
    };
    let year = digits(0..4)?;
    let month = digits(5..7)?;
    let day = digits(8..10)?;
    if !(1..=12).contains(&month) {
        return None;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days_in_month = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=days_in_month).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// One page of commits matching `query`, walking `TOPOLOGICAL | TIME` from the query's
/// `branch:` term, or else from `refs` (`None` = every local branch; missing names are skipped).
///
/// A page ends after `limits.matches` matches or `limits.scan` commits scanned in this call.
/// `cursor` resumes a previous page; it is rejected as `StaleCursor` when the walk's start
/// points moved since it was issued. `cancel` is polled once before the walk and before each
/// commit; when it returns `true` the search stops with `Cancelled`.
pub fn search_commits(
    repo: &Repository,
    query: &SearchQuery,
    refs: Option<&[String]>,
    cursor: Option<&str>,
    limits: PageLimits,
    cancel: &dyn Fn() -> bool,
) -> Result<SearchPage, SearchError> {
    let starts = start_points(repo, query, refs)?;
    let fingerprint = fingerprint(&starts)?;
    let offset = match cursor {
        None => 0,
        Some(cursor) => {
            let (offset, print) = cursor.rsplit_once(':').ok_or(SearchError::StaleCursor)?;
            let offset: usize = offset.parse().map_err(|_| SearchError::StaleCursor)?;
            if print != fingerprint {
                return Err(SearchError::StaleCursor);
            }
            offset
        }
    };

    if cancel() {
        return Err(SearchError::Cancelled);
    }
    if starts.is_empty() {
        return Ok(SearchPage {
            commits: vec![],
            next_cursor: None,
            scanned: 0,
        });
    }

    let terms = prepare(&query.terms);
    let paths: Vec<&str> = terms
        .iter()
        .filter_map(|t| match t {
            Prepared::Path(p) => Some(p.as_str()),
            _ => None,
        })
        .collect();

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    for oid in &starts {
        revwalk.push(*oid)?;
    }
    let decorations = RefDecorations::load(repo)?;

    let mut walk = revwalk.skip(offset).peekable();
    let mut position = offset;
    let mut scanned_here = 0;
    let mut commits = Vec::new();
    while commits.len() < limits.matches && scanned_here < limits.scan {
        if cancel() {
            return Err(SearchError::Cancelled);
        }
        let Some(oid) = walk.next() else {
            break;
        };
        let commit = repo.find_commit(oid?)?;
        position += 1;
        scanned_here += 1;
        if matches(repo, &commit, &terms, &paths)? {
            commits.push(decorations.graph_commit(&commit));
        }
    }

    let next_cursor = walk
        .peek()
        .is_some()
        .then(|| format!("{position}:{fingerprint}"));
    Ok(SearchPage {
        commits,
        next_cursor,
        scanned: position,
    })
}

fn start_points(
    repo: &Repository,
    query: &SearchQuery,
    refs: Option<&[String]>,
) -> Result<Vec<Oid>, SearchError> {
    let branch = query.terms.iter().find_map(|t| match t {
        SearchTerm::Branch(name) => Some(name),
        _ => None,
    });
    let mut oids = Vec::new();
    if let Some(name) = branch {
        let resolved = ["refs/heads/", "refs/remotes/", "refs/tags/"]
            .iter()
            .find_map(|prefix| repo.find_reference(&format!("{prefix}{name}")).ok());
        let commit = resolved
            .and_then(|r| r.peel_to_commit().ok())
            .ok_or_else(|| invalid(format!("No branch named \"{name}\".")))?;
        oids.push(commit.id());
    } else {
        match refs {
            None => {
                for reference in repo.references_glob("refs/heads/*")? {
                    if let Ok(commit) = reference?.peel_to_commit() {
                        oids.push(commit.id());
                    }
                }
            }
            Some(names) => {
                for name in names {
                    // A persisted graph selection can name a branch deleted since: skip it.
                    if let Ok(commit) = repo
                        .find_reference(&format!("refs/heads/{name}"))
                        .and_then(|r| r.peel_to_commit())
                    {
                        oids.push(commit.id());
                    }
                }
            }
        }
    }
    oids.sort();
    oids.dedup();
    Ok(oids)
}

/// First 16 hex chars of the blob hash of the sorted, comma-joined start oids.
fn fingerprint(starts: &[Oid]) -> Result<String, git2::Error> {
    let joined = starts
        .iter()
        .map(|o| o.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let hash = Oid::hash_object(ObjectType::Blob, joined.as_bytes())?;
    Ok(hash.to_string()[..16].to_string())
}

/// A search term ready for matching: lowercased once, ordered by cost.
enum Prepared {
    Commit(String),
    After(i64),
    Before(i64),
    Author(String),
    Committer(String),
    Text { needle: String, sha: bool },
    Message(String),
    Path(String),
    Contents(String),
}

impl Prepared {
    fn cost(&self) -> u8 {
        match self {
            Prepared::Commit(_) => 0,
            Prepared::After(_) | Prepared::Before(_) => 1,
            Prepared::Author(_) | Prepared::Committer(_) => 2,
            Prepared::Text { .. } | Prepared::Message(_) => 3,
            Prepared::Path(_) => 4,
            Prepared::Contents(_) => 5,
        }
    }
}

fn prepare(terms: &[SearchTerm]) -> Vec<Prepared> {
    let mut prepared: Vec<Prepared> = terms
        .iter()
        .filter_map(|term| {
            Some(match term {
                SearchTerm::Text(t) => Prepared::Text {
                    needle: t.to_lowercase(),
                    sha: (4..=40).contains(&t.len()) && t.chars().all(|c| c.is_ascii_hexdigit()),
                },
                SearchTerm::Author(t) => Prepared::Author(t.to_lowercase()),
                SearchTerm::Committer(t) => Prepared::Committer(t.to_lowercase()),
                SearchTerm::Message(t) => Prepared::Message(t.to_lowercase()),
                SearchTerm::Commit(t) => Prepared::Commit(t.to_lowercase()),
                SearchTerm::Path(t) => Prepared::Path(t.clone()),
                SearchTerm::Contents(t) => Prepared::Contents(t.to_lowercase()),
                SearchTerm::After(d) => Prepared::After(*d),
                SearchTerm::Before(d) => Prepared::Before(*d),
                SearchTerm::Branch(_) => return None,
            })
        })
        .collect();
    prepared.sort_by_key(Prepared::cost);
    prepared
}

fn signature_contains(signature: &git2::Signature, needle: &str) -> bool {
    signature
        .name()
        .ok()
        .unwrap_or_default()
        .to_lowercase()
        .contains(needle)
        || signature
            .email()
            .ok()
            .unwrap_or_default()
            .to_lowercase()
            .contains(needle)
}

/// Diff of `commit` against its first parent (the empty tree for a root commit), limited to
/// `paths` when non-empty.
fn first_parent_diff<'r>(
    repo: &'r Repository,
    commit: &git2::Commit,
    paths: &[&str],
) -> Result<git2::Diff<'r>, git2::Error> {
    let tree = commit.tree()?;
    let parent_tree = match commit.parent(0) {
        Ok(parent) => Some(parent.tree()?),
        Err(_) => None,
    };
    let mut opts = DiffOptions::new();
    for path in paths {
        opts.pathspec(path);
    }
    repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
}

fn matches(
    repo: &Repository,
    commit: &git2::Commit,
    terms: &[Prepared],
    paths: &[&str],
) -> Result<bool, git2::Error> {
    let id = commit.id().to_string();
    let lowered = std::cell::OnceCell::new();
    let message =
        || lowered.get_or_init(|| String::from_utf8_lossy(commit.message_bytes()).to_lowercase());
    let when = commit.time();
    let day = (when.seconds() + i64::from(when.offset_minutes()) * 60).div_euclid(86_400);
    // Diff against the first parent limited to every `path:` term; built at most once.
    let mut combined: Option<git2::Diff> = None;

    for term in terms {
        let ok = match term {
            Prepared::Commit(prefix) => id.starts_with(prefix.as_str()),
            Prepared::After(d) => day >= *d,
            Prepared::Before(d) => day <= *d,
            Prepared::Author(n) => signature_contains(&commit.author(), n),
            Prepared::Committer(n) => signature_contains(&commit.committer(), n),
            Prepared::Text { needle, sha } => {
                (*sha && id.starts_with(needle.as_str())) || message().contains(needle.as_str())
            }
            Prepared::Message(t) => message().contains(t.as_str()),
            Prepared::Path(pattern) => {
                if paths.len() == 1 {
                    if combined.is_none() {
                        combined = Some(first_parent_diff(repo, commit, paths)?);
                    }
                    combined.as_ref().is_some_and(|d| d.deltas().len() > 0)
                } else {
                    first_parent_diff(repo, commit, &[pattern.as_str()])?
                        .deltas()
                        .len()
                        > 0
                }
            }
            Prepared::Contents(needle) => {
                if combined.is_none() {
                    combined = Some(first_parent_diff(repo, commit, paths)?);
                }
                let diff = combined.as_ref().expect("diff just built");
                diff_lines_contain(diff, needle)?
            }
        };
        if !ok {
            return Ok(false);
        }
    }
    Ok(true)
}

/// True when an added or removed line of `diff` contains `needle` (already lowercased).
/// Binary deltas yield no lines, so they never match.
fn diff_lines_contain(diff: &git2::Diff, needle: &str) -> Result<bool, git2::Error> {
    let mut found = false;
    let result = diff.foreach(
        &mut |_, _| true,
        None,
        None,
        Some(&mut |_, _, line| {
            if matches!(
                line.origin_value(),
                DiffLineType::Addition | DiffLineType::Deletion
            ) && String::from_utf8_lossy(line.content())
                .to_lowercase()
                .contains(needle)
            {
                found = true;
                return false; // stop the walk
            }
            true
        }),
    );
    match result {
        Ok(()) => Ok(found),
        Err(_) if found => Ok(true),
        Err(e) if e.code() == ErrorCode::User => Ok(true),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SearchTerm::*;

    fn terms(input: &str) -> Vec<SearchTerm> {
        parse_query(input).unwrap().terms
    }

    fn error(input: &str) -> String {
        match parse_query(input) {
            Err(SearchError::InvalidQuery { message }) => message,
            other => panic!("expected InvalidQuery for {input:?}, got {other:?}"),
        }
    }

    #[test]
    fn bare_words_are_separate_text_terms() {
        assert_eq!(
            terms("  fix   Login "),
            vec![Text("fix".into()), Text("Login".into())]
        );
    }

    #[test]
    fn quotes_make_a_phrase_with_escapes() {
        assert_eq!(
            terms(r#""fix login" "say \"hi\" \\o/""#),
            vec![Text("fix login".into()), Text(r#"say "hi" \o/"#.into()),]
        );
    }

    #[test]
    fn quoted_text_with_a_colon_is_literal() {
        assert_eq!(terms(r#""fix: crash""#), vec![Text("fix: crash".into())]);
    }

    #[test]
    fn every_key_parses_and_keys_ignore_case() {
        assert_eq!(
            terms(
                r#"Author:ada committer:bot message:"two words" COMMIT:AbC1 path:src/ contents:needle branch:main"#
            ),
            vec![
                Author("ada".into()),
                Committer("bot".into()),
                Message("two words".into()),
                Commit("abc1".into()),
                Path("src/".into()),
                Contents("needle".into()),
                Branch("main".into()),
            ]
        );
    }

    #[test]
    fn dates_parse_to_days_since_the_epoch() {
        assert_eq!(
            terms("after:2026-01-31 before:2024-02-29 after:1969-12-31"),
            vec![After(20_484), Before(19_782), After(-1),]
        );
    }

    #[test]
    fn unknown_key_suggests_quotes() {
        assert_eq!(
            error("feat: crash"),
            r#"Unknown search key "feat:". Put the text in quotes to search for it literally."#
        );
    }

    #[test]
    fn invalid_queries_have_specific_messages() {
        assert_eq!(error("   "), "Type something to search for.");
        assert_eq!(error("author:"), "author: needs a value.");
        assert_eq!(error("author: ada"), "author: needs a value.");
        assert_eq!(error(r#""abc"#), "Missing closing quote.");
        assert_eq!(error(r#""""#), "Empty quotes don't match anything.");
        assert_eq!(error(r#"author:"""#), "Empty quotes don't match anything.");
        assert_eq!(
            error("after:2026-13-01"),
            "after: expects a date like 2026-01-31."
        );
        assert_eq!(
            error("before:2026-02-30"),
            "before: expects a date like 2026-01-31."
        );
        assert_eq!(
            error("after:2026-1-5"),
            "after: expects a date like 2026-01-31."
        );
        assert_eq!(
            error("before:2023-02-29"),
            "before: expects a date like 2026-01-31."
        );
        assert_eq!(
            error("commit:xyz"),
            "commit: expects a hexadecimal SHA prefix."
        );
        assert_eq!(error("branch:a branch:b"), "Use at most one branch: term.");
    }
}
