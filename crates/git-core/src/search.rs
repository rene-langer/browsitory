//! Commit search: a small query language parsed once (`parse_query`) and a paged, cancellable
//! revwalk over it (`search_commits`).

use git2::Repository;
use thiserror::Error;

use crate::graph::GraphCommit;

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

#[allow(unused_variables)]
pub fn search_commits(
    repo: &Repository,
    query: &SearchQuery,
    refs: Option<&[String]>,
    cursor: Option<&str>,
    limits: PageLimits,
    cancel: &dyn Fn() -> bool,
) -> Result<SearchPage, SearchError> {
    unimplemented!()
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
