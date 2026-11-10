//! Commit search: a small query language parsed once (`parse_query`) into a boolean expression
//! over per-commit terms, and a paged, cancellable revwalk over it (`search_commits`).

use git2::{DiffLineType, DiffOptions, ErrorCode, ObjectType, Oid, Repository, Sort};
use std::cell::OnceCell;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::graph::{GraphCommit, GraphSelection, RefDecorations};
use crate::history::HistoryError;
use crate::line_history::{line_history_ids, LineRange};

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
    /// Where the walk starts, not a test of each commit; only ever a top-level AND term.
    Branch(String),
    /// The commit changed these lines of the file, followed back through earlier changes like
    /// line history (#447). The lines are the file's at the `branch:` term's tip, else HEAD.
    Lines { path: String, lines: LineRange },
    /// `merges:only` / `merges:none` (#450): the commit has more than one parent, or not.
    Merges(MergeFilter),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeFilter {
    /// `merges:only`: commits with two or more parents.
    OnlyMerges,
    /// `merges:none`: commits with at most one parent.
    NoMerges,
}

/// A parsed query (#450): terms joined by AND (adjacent terms, or `and`), OR and NOT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchExpr {
    Term(SearchTerm),
    Not(Box<SearchExpr>),
    /// Every part matches. At least two parts, none of them an `And` itself.
    And(Vec<SearchExpr>),
    /// Any part matches. At least two parts, none of them an `Or` itself.
    Or(Vec<SearchExpr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub expr: SearchExpr,
}

impl SearchQuery {
    /// The parts every match must satisfy: the top-level `And`'s parts, or the whole query.
    pub fn top_level(&self) -> &[SearchExpr] {
        match &self.expr {
            SearchExpr::And(parts) => parts,
            expr => std::slice::from_ref(expr),
        }
    }

    /// The `branch:` term's name, if any. `parse_query` only accepts one at the top level.
    pub fn branch(&self) -> Option<&str> {
        self.top_level().iter().find_map(|part| match part {
            SearchExpr::Term(SearchTerm::Branch(name)) => Some(name.as_str()),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageLimits {
    pub matches: usize,
    pub scan: usize,
    /// Wall-clock budget for one page. Checked after each scanned commit, so a page always
    /// scans at least one commit (a zero budget still makes progress). Keeps a page with
    /// expensive `path:`/`contents:` diffs from holding the single repo worker for long.
    pub time: Duration,
}

impl PageLimits {
    pub const DEFAULT: PageLimits = PageLimits {
        matches: 100,
        scan: 20_000,
        time: Duration::from_millis(250),
    };
}

/// While skipping to a cursor's offset, `cancel` is polled once every this many commits.
const SKIP_CANCEL_INTERVAL: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPage {
    pub commits: Vec<GraphCommit>,
    pub next_cursor: Option<String>,
    pub scanned: usize,
}

#[derive(Debug, Error)]
pub enum SearchError {
    /// The query doesn't parse, or names something that isn't there. Its text starts with
    /// `Invalid search: ` so the transports can tell it apart (`ErrorKind::InvalidSearchQuery`)
    /// and the search field can show it (#450).
    #[error("Invalid search: {message}")]
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

/// The keys a `key:value` term can use (#450). The frontend's search help is checked against
/// them through `frontend/src/ipc/generated/wire-constants.ts` (#497).
pub const KEYS: [&str; 11] = [
    "author",
    "committer",
    "message",
    "commit",
    "path",
    "contents",
    "after",
    "before",
    "branch",
    "line",
    "merges",
];

const OR_NEEDS_TERMS: &str = "\"or\" needs a term on both sides.";
const AND_NEEDS_TERMS: &str = "\"and\" needs a term on both sides.";
const NOT_NEEDS_TERM: &str = "\"not\" or \"-\" needs a term right after it, like -wip.";
const MISSING_CLOSE: &str = "Missing closing parenthesis.";
const UNMATCHED_CLOSE: &str = "Unmatched \")\". Put it in quotes to search for it literally.";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Open,
    Close,
    And,
    Or,
    Not,
    Term(SearchTerm),
}

/// Parses a search query (#450).
///
/// Tokens are separated by whitespace. `(` and `)` group; a bare `and`, `or` or `not` (any case)
/// is an operator; a `-` at the start of a token negates what follows it. Every other token is
/// a term: `key:value` (key is `[A-Za-z]+`, case-insensitive) or a bare value; a value is either
/// `"quoted"` (with `\"` and `\\` escapes) or runs to the next whitespace or to a `)` that
/// closes a group (a `(` inside a value keeps its `)`, so `fix(ui)` stays one word).
///
/// Precedence, loosest first: `or`, then AND (adjacent terms or `and`), then `not`/`-`.
/// `branch:` sets where the walk starts, so it may only appear once, ANDed at the top level.
pub fn parse_query(input: &str) -> Result<SearchQuery, SearchError> {
    let tokens = tokenize(input)?;
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
    };
    let expr = parser.or()?;
    if parser.pos < tokens.len() {
        // `or()` only stops early at a `)` without its `(`.
        return Err(invalid(UNMATCHED_CLOSE));
    }
    let query = SearchQuery { expr };

    let branches = count_branch_terms(&query.expr);
    if branches > 1 {
        return Err(invalid("Use at most one branch: term."));
    }
    if branches == 1 && query.branch().is_none() {
        return Err(invalid(
            "branch: picks where the search starts, so it can't be negated or used with or.",
        ));
    }
    Ok(query)
}

fn count_branch_terms(expr: &SearchExpr) -> usize {
    match expr {
        SearchExpr::Term(SearchTerm::Branch(_)) => 1,
        SearchExpr::Term(_) => 0,
        SearchExpr::Not(inner) => count_branch_terms(inner),
        SearchExpr::And(parts) | SearchExpr::Or(parts) => {
            parts.iter().map(count_branch_terms).sum()
        }
    }
}

/// `and`/`or`/`not` (any case) at `i`, when it is a whole token: followed by the end,
/// whitespace or a parenthesis.
fn operator_at(chars: &[char], i: usize) -> Option<(Token, usize)> {
    for (word, token) in [("and", Token::And), ("or", Token::Or), ("not", Token::Not)] {
        let len = word.len();
        if i + len > chars.len() {
            continue;
        }
        let candidate: String = chars[i..i + len].iter().collect();
        let ends = chars
            .get(i + len)
            .is_none_or(|c| c.is_whitespace() || matches!(c, '(' | ')'));
        if ends && candidate.eq_ignore_ascii_case(word) {
            return Some((token, len));
        }
    }
    None
}

fn tokenize(input: &str) -> Result<Vec<Token>, SearchError> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut tokens = Vec::new();
    // Every pass takes at least one char, so the input's length bounds the passes: an index
    // that stopped advancing (a bug) ends the loop rather than hanging or filling memory.
    for _ in 0..chars.len() {
        if i >= chars.len() {
            break;
        }
        match chars[i] {
            c if c.is_whitespace() => {
                i += 1;
                continue;
            }
            '(' => {
                tokens.push(Token::Open);
                i += 1;
                continue;
            }
            ')' => {
                tokens.push(Token::Close);
                i += 1;
                continue;
            }
            '-' => {
                if chars
                    .get(i + 1)
                    .is_none_or(|c| c.is_whitespace() || *c == ')')
                {
                    return Err(invalid(NOT_NEEDS_TERM));
                }
                tokens.push(Token::Not);
                i += 1;
                continue;
            }
            _ => {}
        }
        if let Some((token, len)) = operator_at(&chars, i) {
            tokens.push(token);
            i += len;
            continue;
        }

        // A key is a run of ASCII letters immediately followed by `:`.
        let end = i + chars[i..]
            .iter()
            .take_while(|c| c.is_ascii_alphabetic())
            .count();
        let key = if end > i && end < chars.len() && chars[end] == ':' {
            let key: String = chars[i..end].iter().collect::<String>().to_lowercase();
            if !KEYS.contains(&key.as_str()) {
                let raw: String = chars[i..end].iter().collect();
                return Err(invalid(format!(
                    "Unknown search key \"{raw}:\". Put the text in quotes to search for it literally."
                )));
            }
            i = end + 1;
            if i >= chars.len() || chars[i].is_whitespace() || chars[i] == ')' {
                return Err(invalid(format!("{key}: needs a value.")));
            }
            if chars[i] == '(' {
                return Err(invalid(format!(
                    "{key}: takes one value. For several, repeat it: ({key}:a or {key}:b)."
                )));
            }
            Some(key)
        } else {
            None
        };

        let value = if chars[i] == '"' {
            // An iterator, not index arithmetic: it always advances, so the loop that grows
            // `value` always ends.
            let mut rest = chars[i + 1..].iter();
            let mut value = String::new();
            let mut closed = false;
            while let Some(&c) = rest.next() {
                match c {
                    '"' => {
                        closed = true;
                        break;
                    }
                    '\\' if matches!(rest.as_slice().first(), Some('"' | '\\')) => {
                        value.extend(rest.next());
                    }
                    c => value.push(c),
                }
            }
            i = chars.len() - rest.as_slice().len();
            if !closed {
                return Err(invalid("Missing closing quote."));
            }
            if value.is_empty() {
                return Err(invalid("Empty quotes don't match anything."));
            }
            value
        } else {
            let start = i;
            // Parentheses opened inside the value; a `)` beyond them closes a group instead.
            let mut depth = 0usize;
            let len = chars[start..]
                .iter()
                .take_while(|&&c| match c {
                    c if c.is_whitespace() => false,
                    '(' => {
                        depth += 1;
                        true
                    }
                    ')' if depth == 0 => false,
                    ')' => {
                        depth -= 1;
                        true
                    }
                    _ => true,
                })
                .count();
            i = start + len;
            chars[start..i].iter().collect()
        };

        tokens.push(Token::Term(term(key.as_deref(), value)?));
    }
    Ok(tokens)
}

fn term(key: Option<&str>, value: String) -> Result<SearchTerm, SearchError> {
    Ok(match key {
        None => SearchTerm::Text(value),
        Some("author") => SearchTerm::Author(value),
        Some("committer") => SearchTerm::Committer(value),
        Some("message") => SearchTerm::Message(value),
        Some("commit") => {
            if value.is_empty() || value.len() > 40 || !value.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err(invalid("commit: expects a hexadecimal SHA prefix."));
            }
            SearchTerm::Commit(value.to_lowercase())
        }
        Some("path") => SearchTerm::Path(value),
        Some("contents") => SearchTerm::Contents(value),
        Some("after") => SearchTerm::After(
            parse_day(&value).ok_or_else(|| invalid("after: expects a date like 2026-01-31."))?,
        ),
        Some("before") => SearchTerm::Before(
            parse_day(&value).ok_or_else(|| invalid("before: expects a date like 2026-01-31."))?,
        ),
        Some("branch") => SearchTerm::Branch(value),
        Some("line") => parse_line_term(&value)?,
        Some("merges") => SearchTerm::Merges(match value.to_lowercase().as_str() {
            "only" => MergeFilter::OnlyMerges,
            "none" => MergeFilter::NoMerges,
            _ => return Err(invalid("merges: expects only or none.")),
        }),
        Some(other) => unreachable!("unhandled search key {other}"),
    })
}

/// Recursive descent over the tokens: `or := and ("or" and)*`,
/// `and := unary (["and"] unary)*`, `unary := ("not" | "-") unary | "(" or ")" | term`.
struct Parser<'t> {
    tokens: &'t [Token],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn previous(&self) -> Option<&Token> {
        self.pos.checked_sub(1).and_then(|p| self.tokens.get(p))
    }

    fn or(&mut self) -> Result<SearchExpr, SearchError> {
        let mut parts = vec![self.and()?];
        // Every pass takes at least one token; see `tokenize` for why the passes are bounded.
        for _ in 0..self.tokens.len() {
            if self.peek() != Some(&Token::Or) {
                break;
            }
            self.pos += 1;
            parts.push(self.and()?);
        }
        Ok(join(parts, false))
    }

    fn and(&mut self) -> Result<SearchExpr, SearchError> {
        let mut parts = Vec::new();
        // Every pass but the last takes at least one token; see `tokenize` for why the passes
        // are bounded.
        for _ in 0..=self.tokens.len() {
            match self.peek() {
                None | Some(Token::Close) | Some(Token::Or) => break,
                Some(Token::And) => {
                    self.pos += 1;
                    if parts.is_empty()
                        || matches!(
                            self.peek(),
                            None | Some(Token::Close | Token::Or | Token::And)
                        )
                    {
                        return Err(invalid(AND_NEEDS_TERMS));
                    }
                }
                Some(_) => parts.push(self.unary()?),
            }
        }
        if parts.is_empty() {
            return Err(invalid(match (self.previous(), self.peek()) {
                (_, Some(Token::Or)) | (Some(Token::Or), _) => OR_NEEDS_TERMS,
                (None, None) => "Type something to search for.",
                (Some(Token::Open), Some(Token::Close)) => {
                    "Empty parentheses don't match anything."
                }
                (Some(Token::Open), None) => MISSING_CLOSE,
                _ => UNMATCHED_CLOSE,
            }));
        }
        Ok(join(parts, true))
    }

    fn unary(&mut self) -> Result<SearchExpr, SearchError> {
        let token = self.peek().cloned();
        self.pos += 1;
        match token {
            Some(Token::Not) => {
                if matches!(
                    self.peek(),
                    None | Some(Token::Close | Token::Or | Token::And)
                ) {
                    return Err(invalid(NOT_NEEDS_TERM));
                }
                Ok(SearchExpr::Not(Box::new(self.unary()?)))
            }
            Some(Token::Open) => {
                let inner = self.or()?;
                if self.peek() != Some(&Token::Close) {
                    return Err(invalid(MISSING_CLOSE));
                }
                self.pos += 1;
                Ok(inner)
            }
            Some(Token::Term(term)) => Ok(SearchExpr::Term(term)),
            // `and()` only calls this on a token that starts a term.
            other => unreachable!("unary() at {other:?}"),
        }
    }
}

/// One part as itself; several as an `And` (`all`) or `Or`, flattening nested ones of the same
/// kind so `a (b c)` is `And[a, b, c]`.
fn join(parts: Vec<SearchExpr>, all: bool) -> SearchExpr {
    if parts.len() == 1 {
        return parts.into_iter().next().expect("one part");
    }
    let mut flat = Vec::with_capacity(parts.len());
    for part in parts {
        match part {
            SearchExpr::And(inner) if all => flat.extend(inner),
            SearchExpr::Or(inner) if !all => flat.extend(inner),
            other => flat.push(other),
        }
    }
    if all {
        SearchExpr::And(flat)
    } else {
        SearchExpr::Or(flat)
    }
}

/// `<path>:<from>-<to>` or `<path>:<line>` → a `Lines` term. The path is everything before the
/// last `:`, so it may hold colons itself.
fn parse_line_term(value: &str) -> Result<SearchTerm, SearchError> {
    const EXPECTS: &str = "line: expects a file and lines, like line:src/main.rs:10-20.";
    let (path, range) = value.rsplit_once(':').ok_or_else(|| invalid(EXPECTS))?;
    let number = |text: &str| -> Result<usize, SearchError> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid(EXPECTS));
        }
        text.parse().map_err(|_| invalid(EXPECTS))
    };
    let (start, end) = match range.split_once('-') {
        Some((start, end)) => (number(start)?, number(end)?),
        None => {
            let line = number(range)?;
            (line, line)
        }
    };
    if path.is_empty() {
        return Err(invalid(EXPECTS));
    }
    if start == 0 || end < start {
        return Err(invalid(
            "line: needs lines counted from 1, the first no higher than the last.",
        ));
    }
    Ok(SearchTerm::Lines {
        path: path.to_string(),
        lines: LineRange { start, end },
    })
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
/// `branch:` term, or else from the graph's `selection` (local and remote branches alike, see
/// [`GraphSelection`]; missing names are skipped). Without a `branch:` term, a selection of
/// exactly one branch walks first parents only, like `graph_log` for the same selection, so
/// search and graph list the same commits (#380, #385).
///
/// A page ends after `limits.matches` matches, `limits.scan` commits scanned in this call, or
/// once `limits.time` has elapsed (checked after each commit, so at least one is scanned); in
/// every case `next_cursor` resumes right after the last scanned commit while any remain.
/// `cursor` resumes a previous page; it is rejected as `StaleCursor` when the walk's start
/// points moved since it was issued. `cancel` is polled once before the walk, every
/// `SKIP_CANCEL_INTERVAL` commits while skipping to the cursor, and before each scanned
/// commit; when it returns `true` the search stops with `Cancelled`.
pub fn search_commits(
    repo: &Repository,
    query: &SearchQuery,
    selection: &GraphSelection,
    cursor: Option<&str>,
    limits: PageLimits,
    cancel: &dyn Fn() -> bool,
) -> Result<SearchPage, SearchError> {
    let starts = start_points(repo, query, selection)?;
    let first_parent = query.branch().is_none() && selection.walks_first_parent();
    let fingerprint = fingerprint(&starts, first_parent)?;
    let started = Instant::now();
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

    let node = prepare(repo, query, &starts, cancel)?;
    // `contents:` looks only at the files of the `path:` terms ANDed at the top level.
    let paths: Vec<&str> = query
        .top_level()
        .iter()
        .filter_map(|part| match part {
            SearchExpr::Term(SearchTerm::Path(p)) => Some(p.as_str()),
            _ => None,
        })
        .collect();

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    if first_parent {
        revwalk.simplify_first_parent()?;
    }
    for oid in &starts {
        revwalk.push(*oid)?;
    }
    let decorations = RefDecorations::load(repo)?;

    let mut walk = revwalk.peekable();
    for skipped in 0..offset {
        if skipped % SKIP_CANCEL_INTERVAL == 0 && cancel() {
            return Err(SearchError::Cancelled);
        }
        if walk.next().is_none() {
            break;
        }
    }
    let mut position = offset;
    let mut scanned_here = 0;
    let mut commits = Vec::new();
    while commits.len() < limits.matches
        && scanned_here < limits.scan
        && (scanned_here == 0 || started.elapsed() < limits.time)
    {
        if cancel() {
            return Err(SearchError::Cancelled);
        }
        let Some(oid) = walk.next() else {
            break;
        };
        let commit = repo.find_commit(oid?)?;
        position += 1;
        scanned_here += 1;
        if Subject::new(repo, &commit, &paths).eval(&node)? {
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
    selection: &GraphSelection,
) -> Result<Vec<Oid>, SearchError> {
    let Some(name) = query.branch() else {
        // The same start points as the graph for this selection.
        return Ok(selection.start_oids(repo)?);
    };
    let resolved = ["refs/heads/", "refs/remotes/", "refs/tags/"]
        .iter()
        .find_map(|prefix| repo.find_reference(&format!("{prefix}{name}")).ok());
    let commit = resolved
        .and_then(|r| r.peel_to_commit().ok())
        .ok_or_else(|| invalid(format!("No branch named \"{name}\".")))?;
    Ok(vec![commit.id()])
}

/// First 16 hex chars of the blob hash of the sorted, comma-joined start oids.
/// Identifies a walk: its start oids and whether it follows first parents only, so a cursor from
/// one walk never resumes the other (one selected branch and two branches at the same commit
/// share their start oids but not their walk). Start oids and first-parent mode determine the
/// walk, so a remote branch added to or dropped from the selection (#385) changes the
/// fingerprint exactly when it changes the walk.
fn fingerprint(starts: &[Oid], first_parent: bool) -> Result<String, git2::Error> {
    let mut joined = starts
        .iter()
        .map(|o| o.to_string())
        .collect::<Vec<_>>()
        .join(",");
    if first_parent {
        joined.push_str(";first-parent");
    }
    let hash = Oid::hash_object(ObjectType::Blob, joined.as_bytes())?;
    Ok(hash.to_string()[..16].to_string())
}

/// A search term ready for matching: lowercased once.
enum Prepared {
    /// The commits a `line:` term's line history lists, computed once per page.
    Lines(HashSet<Oid>),
    Commit(String),
    Merges(MergeFilter),
    After(i64),
    Before(i64),
    Author(String),
    Committer(String),
    Text {
        needle: String,
        sha: bool,
    },
    Message(String),
    Path(String),
    Contents(String),
}

impl Prepared {
    fn cost(&self) -> u8 {
        match self {
            Prepared::Lines(_) | Prepared::Commit(_) | Prepared::Merges(_) => 0,
            Prepared::After(_) | Prepared::Before(_) => 1,
            Prepared::Author(_) | Prepared::Committer(_) => 2,
            Prepared::Text { .. } | Prepared::Message(_) => 3,
            Prepared::Path(_) => 4,
            Prepared::Contents(_) => 5,
        }
    }
}

/// A query ready for matching: the parts of every `And`/`Or` ordered cheapest first, so the
/// expensive diff-based terms run only when the cheap ones haven't decided already.
enum Node {
    Term(Prepared),
    Not(Box<Node>),
    /// Every part matches; empty matches every commit (a query of only `branch:`).
    And(Vec<Node>),
    Or(Vec<Node>),
}

impl Node {
    /// The most expensive term inside.
    fn cost(&self) -> u8 {
        match self {
            Node::Term(term) => term.cost(),
            Node::Not(inner) => inner.cost(),
            Node::And(parts) | Node::Or(parts) => parts.iter().map(Node::cost).max().unwrap_or(0),
        }
    }
}

/// `starts` are the walk's start points: with a `branch:` term, the one that names the commit
/// `line:` terms read their lines from (HEAD otherwise).
fn prepare(
    repo: &Repository,
    query: &SearchQuery,
    starts: &[Oid],
    cancel: &dyn Fn() -> bool,
) -> Result<Node, SearchError> {
    let ctx = PrepareContext {
        repo,
        starts,
        has_branch: query.branch().is_some(),
        cancel,
    };
    Ok(ctx.node(&query.expr)?.unwrap_or(Node::And(Vec::new())))
}

struct PrepareContext<'a> {
    repo: &'a Repository,
    starts: &'a [Oid],
    has_branch: bool,
    cancel: &'a dyn Fn() -> bool,
}

impl PrepareContext<'_> {
    /// `None` for a `branch:` term, which tests nothing (`parse_query` keeps it at the top
    /// level, so it is only ever dropped from the top-level `And`).
    fn node(&self, expr: &SearchExpr) -> Result<Option<Node>, SearchError> {
        let sorted = |parts: &[SearchExpr]| -> Result<Vec<Node>, SearchError> {
            let mut nodes = Vec::with_capacity(parts.len());
            for part in parts {
                nodes.extend(self.node(part)?);
            }
            nodes.sort_by_key(Node::cost);
            Ok(nodes)
        };
        Ok(Some(match expr {
            SearchExpr::Term(term) => match self.term(term)? {
                Some(prepared) => Node::Term(prepared),
                None => return Ok(None),
            },
            SearchExpr::Not(inner) => {
                Node::Not(Box::new(self.node(inner)?.unwrap_or(Node::And(Vec::new()))))
            }
            SearchExpr::And(parts) => Node::And(sorted(parts)?),
            SearchExpr::Or(parts) => Node::Or(sorted(parts)?),
        }))
    }

    fn term(&self, term: &SearchTerm) -> Result<Option<Prepared>, SearchError> {
        Ok(Some(match term {
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
            SearchTerm::Merges(filter) => Prepared::Merges(*filter),
            SearchTerm::Lines { path, lines } => Prepared::Lines(line_term_commits(
                self.repo,
                self.has_branch,
                self.starts,
                path,
                *lines,
                self.cancel,
            )?),
            SearchTerm::Branch(_) => return Ok(None),
        }))
    }
}

/// The commits a `line:` term matches: line history of `path`'s `range` as it is at the
/// `branch:` term's tip (`starts` holds exactly that commit then), or else at HEAD. As a set
/// computed up front it is a plain per-commit test, so `not` and `or` apply to it like to any
/// other term.
fn line_term_commits(
    repo: &Repository,
    has_branch: bool,
    starts: &[Oid],
    path: &str,
    range: LineRange,
    cancel: &dyn Fn() -> bool,
) -> Result<HashSet<Oid>, SearchError> {
    let start = if has_branch {
        starts.first().copied()
    } else {
        match repo.head() {
            Ok(head) => Some(head.peel_to_commit()?.id()),
            Err(e) if matches!(e.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => None,
            Err(e) => return Err(e.into()),
        }
    };
    let Some(start) = start else {
        return Ok(HashSet::new());
    };
    match line_history_ids(repo, start, path, range, cancel) {
        Ok(Some(ids)) => Ok(ids),
        Ok(None) => Err(SearchError::Cancelled),
        Err(HistoryError::Git(e)) => Err(e.into()),
        Err(e) => Err(invalid(format!("line: {e}"))),
    }
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

/// One commit being matched, with what several terms share computed at most once.
struct Subject<'a, 'r> {
    repo: &'r Repository,
    commit: &'a git2::Commit<'r>,
    id: String,
    message: OnceCell<String>,
    day: i64,
    /// The top-level `path:` patterns; `contents:` looks only at their files.
    paths: &'a [&'a str],
    /// Diff against the first parent limited to `paths`; built at most once.
    scoped: Option<git2::Diff<'r>>,
}

impl<'a, 'r> Subject<'a, 'r> {
    fn new(repo: &'r Repository, commit: &'a git2::Commit<'r>, paths: &'a [&'a str]) -> Self {
        let when = commit.time();
        Self {
            repo,
            commit,
            id: commit.id().to_string(),
            message: OnceCell::new(),
            day: (when.seconds() + i64::from(when.offset_minutes()) * 60).div_euclid(86_400),
            paths,
            scoped: None,
        }
    }

    fn message(&self) -> &str {
        self.message
            .get_or_init(|| String::from_utf8_lossy(self.commit.message_bytes()).to_lowercase())
    }

    fn scoped_diff(&mut self) -> Result<&git2::Diff<'r>, git2::Error> {
        if self.scoped.is_none() {
            self.scoped = Some(first_parent_diff(self.repo, self.commit, self.paths)?);
        }
        Ok(self.scoped.as_ref().expect("diff just built"))
    }

    fn eval(&mut self, node: &Node) -> Result<bool, git2::Error> {
        Ok(match node {
            Node::Term(term) => self.term(term)?,
            Node::Not(inner) => !self.eval(inner)?,
            Node::And(parts) => {
                for part in parts {
                    if !self.eval(part)? {
                        return Ok(false);
                    }
                }
                true
            }
            Node::Or(parts) => {
                for part in parts {
                    if self.eval(part)? {
                        return Ok(true);
                    }
                }
                false
            }
        })
    }

    fn term(&mut self, term: &Prepared) -> Result<bool, git2::Error> {
        let commit = self.commit;
        Ok(match term {
            Prepared::Lines(ids) => ids.contains(&commit.id()),
            Prepared::Commit(prefix) => self.id.starts_with(prefix.as_str()),
            Prepared::Merges(MergeFilter::OnlyMerges) => commit.parent_count() > 1,
            Prepared::Merges(MergeFilter::NoMerges) => commit.parent_count() <= 1,
            Prepared::After(d) => self.day >= *d,
            Prepared::Before(d) => self.day <= *d,
            Prepared::Author(n) => signature_contains(&commit.author(), n),
            Prepared::Committer(n) => signature_contains(&commit.committer(), n),
            Prepared::Text { needle, sha } => {
                (*sha && self.id.starts_with(needle.as_str()))
                    || self.message().contains(needle.as_str())
            }
            Prepared::Message(t) => self.message().contains(t.as_str()),
            Prepared::Path(pattern) => {
                if self.paths == [pattern.as_str()] {
                    self.scoped_diff()?.deltas().len() > 0
                } else {
                    first_parent_diff(self.repo, commit, &[pattern.as_str()])?
                        .deltas()
                        .len()
                        > 0
                }
            }
            Prepared::Contents(needle) => diff_lines_contain(self.scoped_diff()?, needle)?,
        })
    }
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

    /// The query's terms when it is a plain AND of terms.
    fn terms(input: &str) -> Vec<SearchTerm> {
        parse_query(input)
            .unwrap()
            .top_level()
            .iter()
            .map(|part| match part {
                SearchExpr::Term(term) => term.clone(),
                other => panic!("expected only terms in {input:?}, got {other:?}"),
            })
            .collect()
    }

    fn expr(input: &str) -> SearchExpr {
        parse_query(input).unwrap().expr
    }

    fn text(value: &str) -> SearchExpr {
        SearchExpr::Term(Text(value.into()))
    }

    fn not(inner: SearchExpr) -> SearchExpr {
        SearchExpr::Not(Box::new(inner))
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
    fn a_backslash_before_any_other_char_stays_in_quotes() {
        assert_eq!(terms(r#""C:\dir\x""#), vec![Text(r"C:\dir\x".into())]);
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
        let expects = "line: expects a file and lines, like line:src/main.rs:10-20.";
        assert_eq!(error("line:src/main.rs"), expects);
        assert_eq!(error("line:src/main.rs:"), expects);
        assert_eq!(error("line::3-4"), expects);
        assert_eq!(error("line:a.rs:x-4"), expects);
        assert_eq!(error("line:a.rs:3-"), expects);
        assert_eq!(error("line:a.rs:-3"), expects);
        assert_eq!(
            error("line:a.rs:5-3"),
            "line: needs lines counted from 1, the first no higher than the last."
        );
        assert_eq!(
            error("line:a.rs:0-3"),
            "line: needs lines counted from 1, the first no higher than the last."
        );
    }

    #[test]
    fn line_terms_take_a_range_or_one_line_and_the_last_colon() {
        assert_eq!(
            terms(r#"line:src/a.rs:10-20 LINE:b.rs:7 line:"dir with space/c:d.rs:1-2""#),
            vec![
                Lines {
                    path: "src/a.rs".into(),
                    lines: LineRange { start: 10, end: 20 }
                },
                Lines {
                    path: "b.rs".into(),
                    lines: LineRange { start: 7, end: 7 }
                },
                Lines {
                    path: "dir with space/c:d.rs".into(),
                    lines: LineRange { start: 1, end: 2 }
                },
            ]
        );
    }

    #[test]
    fn adjacent_terms_and_explicit_and_both_mean_and() {
        assert_eq!(expr("a b"), expr("a AND b"));
        assert_eq!(expr("a and b"), SearchExpr::And(vec![text("a"), text("b")]));
    }

    #[test]
    fn or_binds_looser_than_and_and_not_binds_tightest() {
        assert_eq!(
            expr("a b or not c d"),
            SearchExpr::Or(vec![
                SearchExpr::And(vec![text("a"), text("b")]),
                SearchExpr::And(vec![not(text("c")), text("d")]),
            ])
        );
    }

    #[test]
    fn parentheses_group_and_nested_groups_flatten() {
        assert_eq!(
            expr("(a OR b) c"),
            SearchExpr::And(vec![SearchExpr::Or(vec![text("a"), text("b")]), text("c"),])
        );
        assert_eq!(
            expr("a (b (c d))"),
            SearchExpr::And(vec![text("a"), text("b"), text("c"), text("d")])
        );
        assert_eq!(
            expr("a or (b or c)"),
            SearchExpr::Or(vec![text("a"), text("b"), text("c")])
        );
        assert_eq!(expr("((a))"), text("a"));
    }

    #[test]
    fn a_leading_dash_negates_and_works_on_keys_and_groups() {
        assert_eq!(
            expr("-wip -author:bot -(a b)"),
            SearchExpr::And(vec![
                not(text("wip")),
                not(SearchExpr::Term(Author("bot".into()))),
                not(SearchExpr::And(vec![text("a"), text("b")])),
            ])
        );
        assert_eq!(expr("not not a"), not(not(text("a"))));
        assert_eq!(expr("NOT(a)"), not(text("a")));
    }

    #[test]
    fn operators_are_whole_words_and_quotes_make_them_text() {
        assert_eq!(
            terms(r#"order nothing "or" "not" a-b fix(ui) fix(ui):"#),
            vec![
                Text("order".into()),
                Text("nothing".into()),
                Text("or".into()),
                Text("not".into()),
                Text("a-b".into()),
                Text("fix(ui)".into()),
                Text("fix(ui):".into()),
            ]
        );
        assert_eq!(
            terms(r#""-wip" "(x)""#),
            vec![Text("-wip".into()), Text("(x)".into())]
        );
    }

    #[test]
    fn a_closing_parenthesis_ends_a_value() {
        assert_eq!(
            expr("(author:ada or path:src/a(b).rs)"),
            SearchExpr::Or(vec![
                SearchExpr::Term(Author("ada".into())),
                SearchExpr::Term(Path("src/a(b).rs".into())),
            ])
        );
    }

    #[test]
    fn merges_takes_only_or_none() {
        assert_eq!(
            terms("merges:only MERGES:None"),
            vec![
                Merges(MergeFilter::OnlyMerges),
                Merges(MergeFilter::NoMerges)
            ]
        );
        assert_eq!(error("merges:yes"), "merges: expects only or none.");
    }

    #[test]
    fn malformed_boolean_queries_have_specific_messages() {
        let or = "\"or\" needs a term on both sides.";
        let and = "\"and\" needs a term on both sides.";
        let not = "\"not\" or \"-\" needs a term right after it, like -wip.";
        let unmatched = "Unmatched \")\". Put it in quotes to search for it literally.";
        assert_eq!(error("or a"), or);
        assert_eq!(error("a or"), or);
        assert_eq!(error("a or or b"), or);
        assert_eq!(error("(or a)"), or);
        assert_eq!(error("(a or)"), or);
        assert_eq!(error("and a"), and);
        assert_eq!(error("a and"), and);
        assert_eq!(error("a and or b"), and);
        assert_eq!(error("not"), not);
        assert_eq!(error("a -"), not);
        assert_eq!(error("- a"), not);
        assert_eq!(error("(a -)"), not);
        assert_eq!(error("not or a"), not);
        assert_eq!(error("()"), "Empty parentheses don't match anything.");
        assert_eq!(error("(a"), "Missing closing parenthesis.");
        assert_eq!(error("(a (b)"), "Missing closing parenthesis.");
        assert_eq!(error("("), "Missing closing parenthesis.");
        assert_eq!(error("a)"), unmatched);
        assert_eq!(error(")"), unmatched);
        assert_eq!(error("(a))"), unmatched);
        assert_eq!(error("author:)"), "author: needs a value.");
        assert_eq!(
            error("author:(a or b)"),
            "author: takes one value. For several, repeat it: (author:a or author:b)."
        );
    }

    #[test]
    fn branch_must_be_a_single_top_level_and_term() {
        assert_eq!(
            parse_query("(a b) branch:main").unwrap().branch(),
            Some("main")
        );
        assert_eq!(parse_query("branch:main").unwrap().branch(), Some("main"));
        let message =
            "branch: picks where the search starts, so it can't be negated or used with or.";
        assert_eq!(error("-branch:main"), message);
        assert_eq!(error("a or branch:main"), message);
        assert_eq!(error("(a or b branch:main)"), message);
        assert_eq!(
            error("branch:a (branch:b)"),
            "Use at most one branch: term."
        );
    }

    #[test]
    fn invalid_query_text_names_the_search() {
        assert_eq!(
            parse_query("(").unwrap_err().to_string(),
            "Invalid search: Missing closing parenthesis."
        );
    }
}
