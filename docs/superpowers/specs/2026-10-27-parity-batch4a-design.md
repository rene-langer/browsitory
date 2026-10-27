# Feature parity, batch 4a: commit search

## Goal

Find commits by text, author, committer, message, SHA, changed path, changed line contents, date
range and branch, from a search field above the commit graph. Both frontends (Tauri app, VSCode
extension) through `RepoClient`. Refines section 4a of
`docs/superpowers/specs/2026-10-27-parity-batch4-roadmap-design.md` (issue #90).

## Current state

- `git-core::graph::graph_log(repo, limit, selected_branches)` walks `refs/heads/*` (or the
  named local branches) with `Sort::TOPOLOGICAL | Sort::TIME` and builds `GraphCommit` rows
  through the crate-private `RefDecorations::load` / `graph_commit`. `commit.time()` (committer
  time) is the row's `timestamp`.
- `graph_log` fails with `reference 'refs/heads/<name>' not found` when the selection names a
  branch that no longer exists. Search does not copy this; it skips missing refs (see Decisions).
- File history (`useFileHistory` + `FileHistoryList`) already replaces the graph rows with a flat
  list. It saves the selected row on open, restores it on close, drops stale responses with a
  generation counter, and reloads on `refreshGeneration`.
- The worker thread runs one command at a time. `cancel_transfer` reaches a busy worker through
  a side channel (`CancelRegistry`, an `Arc<Mutex<HashSet<String>>>` shared by `Worker` and
  `WorkerHandle`), never through the command queue.
- Tauri commands are `async fn`. The sidecar runs every JSON-RPC request on its own thread
  (`vscode-sidecar/src/main.rs`), so a cancel request is not queued behind a running search
  there either.
- No search command exists anywhere.

## Decisions

- **Search runs in `git-core` with `git2` only.** No `git log` subprocess and no new dependency.
  `docs/LICENSE_COMPLIANCE.md` does not change.
- **One parsed query, all terms ANDed.** No `OR`, negation or regex (out of scope per the
  roadmap). Matching ignores case for every text key.
- **Bare words are separate terms.** `fix login` means "fix" AND "login". Quotes make a phrase:
  `"fix login"`.
- **Only `<letters>:` is read as a key, and keys are case-insensitive.** An unknown key is an
  error that tells the user to quote the text (`"feat:"`). Conventional-commit prefixes therefore
  need quotes. The alternative was to treat unknown keys as text, but then a typo such as
  `autor:x` would quietly match nothing.
- **A bare word also matches a SHA prefix** when it is 4–40 hex characters. `commit:` accepts
  1–40 hex characters.
- **Dates are whole days, inclusive at both ends, in the commit's own timezone.** `after:D` keeps
  commits whose committer date is on or after D. `before:D` keeps commits on or before D. "Date"
  means the calendar day of the committer timestamp plus its own UTC offset, which is the date
  `git log` prints. This needs no timezone library. git's `--before` is a point in time, not a
  whole day; this is a deliberate difference.
- **`path:` and `contents:` compare a commit with its first parent.** A root commit is compared
  with the empty tree. Merges are therefore judged by what they brought into the first-parent
  line, like `git log --first-parent -p`. `path:` uses libgit2's pathspec matching
  (`DiffOptions::pathspec`): `src` and `src/` match the directory, and `*` crosses `/`, so
  `*.md` matches `docs/guide.md`. Matching is case-sensitive, like git. Several `path:` terms
  must all match. `contents:` searches only in files that match the `path:` terms, when there
  are any. Binary files never match `contents:`.
- **`branch:` replaces the graph selection.** A query without `branch:` searches what the graph
  shows: the `refs` argument, with the same meaning as `getCommitGraph`'s `selectedBranches`
  (`null` = every local branch). `branch:NAME` resolves `refs/heads/NAME`, then
  `refs/remotes/NAME`, then `refs/tags/NAME`, and searches only commits reachable from it. An
  unknown name is an `InvalidQuery`. At most one `branch:` term is allowed. "Reachable from both
  A and B" is rarely what a user means, and union would break the AND rule.
- **Missing selected refs are skipped, not an error.** This means a stale persisted graph
  selection cannot break search.
- **`refs: string[] | null` instead of the roadmap's `string[]`.** It mirrors `getCommitGraph`,
  so the frontend passes `graphBranchSelection` unchanged.
- **Paging uses an offset cursor checked against the walk's start points.** The cursor is
  `"<offset>:<fingerprint>"`. The offset counts commits already scanned in walk order. The
  fingerprint is the first 16 hex characters of `Oid::hash_object(Blob, …)` over the sorted,
  comma-joined start oids. Resuming re-creates the revwalk and skips `offset` oids. That costs
  about one graph refresh, because libgit2's topological sort walks everything before it yields
  the first oid anyway. The other option was to keep the revwalk alive in the worker, but it
  borrows the `Repository` the worker loop also uses mutably, so the order is not worth the
  lifetime trouble. If the fingerprint differs (a branch moved between pages) or the cursor is
  malformed, the result is `StaleCursor` ("The history changed since this search started.
  Search again."). Duplicates or gaps are never possible.
- **Page limits.** `search_commits` returns once 100 matches are collected or 20,000 commits
  have been scanned in this call (`PageLimits::DEFAULT`). `scanned` is cumulative: the walk
  position, so it equals the cursor's offset. `next_cursor` is `None` once the walk is
  exhausted. The walk is peeked, so a page that ends exactly at the last commit already returns
  `None`. `PageLimits` is a parameter, not a constant, so tests can use small budgets. The
  roadmap's `limit: usize` became `limits: PageLimits`.
- **The frontend keeps paging until it has a full round.** One round requests pages until it
  has 100 new matches or the cursor is `null`, updating "Scanned N commits…" after each page.
  So a rare term still finds its first matches without user clicks, and the worker is free
  between pages, so other commands run between them. "Load more" starts the next round from
  `nextCursor`.
- **One in-flight search per repo, cancelled by a generation counter.** `Worker` and
  `WorkerHandle` share `search_generation: Arc<AtomicU64>`. `WorkerHandle::search_commits`
  increments it before it queues `Command::SearchCommits { generation, … }`. The running search
  polls `generation != current`, once before the walk starts and before each commit. So a new
  search (or "Load more") cancels any older one, even one still queued, and `cancel_search`
  just increments the counter. This is the same side-channel reasoning as `CancelRegistry`. A
  counter replaces the registry's set of operation ids because there is only ever one search to
  cancel.
- **Cancelled is an error, not an empty page.** The worker replies `Err("Search cancelled.")`.
  The frontend drops every response whose generation is stale, so it never shows that text.
- **Search does not record an undo entry** (read-only; `undo::plan_for` returns `None`).
- **`tauriRepoClient.searchCommits` uses plain `invoke`, not `loggedInvoke`.** Invalid queries
  and cancellations happen while the user types and would flood the log. The error is shown in
  the UI.
- **The field is always visible above the graph.** It is hidden only while file history is
  open. Ctrl/Cmd+F on the focused commit list (graph or results) and the palette command
  "Search commits" focus it. An empty field shows the normal graph. A non-empty field (after
  250 ms of no typing) replaces the graph with the results.
- **Selection follows the file-history pattern.** The selected row is saved when results first
  replace the graph. The first match is selected when a new query's first match arrives. Esc in
  the field or in the results list, or ×, clears the query, cancels the search, restores the
  graph and re-selects the saved row.
- **File history from a search result keeps the search.** File history takes over the pane.
  Closing it returns to the same results with the hit selected (file history restores its own
  saved row). The palette command "Search commits" closes an open file history first.
- **Results reload on `refreshGeneration` and on a graph branch selection change**, from the
  first page, without changing the selection.
- **Retry always restarts from the first page.** This covers `StaleCursor` and any failed
  "Load more".
- **Result rows have no context menu.** Cherry-pick, revert, etc. stay on the graph. Selecting a
  result shows its commit header, files and diffs as usual.

## Backend (`git-core/src/search.rs`, new)

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchTerm {
    Text(String),      // message contains, or SHA prefix when 4–40 hex chars
    Author(String),    // author name or email contains
    Committer(String), // committer name or email contains
    Message(String),   // full message contains
    Commit(String),    // SHA prefix, lowercase hex, 1–40 chars
    Path(String),      // pathspec pattern
    Contents(String),  // added or removed line contains
    After(i64),        // days since 1970-01-01, inclusive
    Before(i64),       // days since 1970-01-01, inclusive
    Branch(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery { pub terms: Vec<SearchTerm> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageLimits { pub matches: usize, pub scan: usize }
impl PageLimits { pub const DEFAULT: PageLimits = PageLimits { matches: 100, scan: 20_000 }; }

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

pub fn parse_query(input: &str) -> Result<SearchQuery, SearchError>;

pub fn search_commits(
    repo: &Repository,
    query: &SearchQuery,
    refs: Option<&[String]>,
    cursor: Option<&str>,
    limits: PageLimits,
    cancel: &dyn Fn() -> bool,
) -> Result<SearchPage, SearchError>;
```

### Query grammar

```
query  := ws* (term (ws+ term)*)? ws*
term   := key ':' value | value
key    := [A-Za-z]+            (only when directly followed by ':')
value  := '"' ( [^"\\] | '\\' ["\\] )* '"' | [^ \t\r\n]+
```

Error messages (the `InvalidQuery` text, shown verbatim in the UI):

| Input | Message |
|---|---|
| blank | `Type something to search for.` |
| `feat: x` | `Unknown search key "feat:". Put the text in quotes to search for it literally.` |
| `author:` / `author: x` | `author: needs a value.` |
| `"abc` | `Missing closing quote.` |
| `""` / `author:""` | `Empty quotes don't match anything.` |
| `after:2026-13-01` | `after: expects a date like 2026-01-31.` (same for `before:`) |
| `commit:xyz` | `commit: expects a hexadecimal SHA prefix.` |
| two `branch:` | `Use at most one branch: term.` |
| `branch:nope` | `No branch named "nope".` (raised by `search_commits`) |

### Matching order

Each commit is checked in this order, cheapest first, stopping at the first term that fails:
`Commit`, `After`/`Before`, `Author`/`Committer`, `Text`/`Message`, then `Path`, then
`Contents`. `Branch` only picks the start point. The first-parent diff is computed at most once
per commit and shared by `Path` and `Contents`.

## `repo-service`, Tauri, sidecar, `RepoClient`

- `worker/search.rs` (new): handler plus `WorkerHandle::search_commits(query: String, refs:
  Option<Vec<String>>, cursor: Option<String>) -> Result<SearchPage, String>` and
  `WorkerHandle::cancel_search(&self)`. Parsing happens in the handler, so an invalid query
  replies `Err(message)`.
- Tauri: `search_commits(repo_path, query, refs, cursor)` → `SearchPageDto { commits:
  Vec<GraphCommitDto>, next_cursor, scanned }` (camelCase), `cancel_search(repo_path)`. New
  module `commands/search.rs`.
- Sidecar: `search_commits`, `cancel_search`, with its own `SearchPageDto` (the sidecar keeps
  its own DTO copies by convention).
- `RepoClient`:

```ts
export interface SearchPage {
  commits: GraphCommit[];
  nextCursor: string | null;
  scanned: number;
}

searchCommits(repoPath: string, query: string, refs: string[] | null, cursor: string | null): Promise<SearchPage>;
cancelSearch(repoPath: string): Promise<void>;
```

## Frontend

- `state/useCommitSearch.ts`: the field text, the results state, the debounce (250 ms), rounds
  of 100 matches, the generation counter, the saved selection, `cancelSearch` on close and
  unmount, and a reload on refresh or a branch selection change.
- `components/CommitSearchBar.tsx`: `input type="search"` (aria-label "Search commits",
  placeholder "Search commits — author:, path:, contents:…"), a "?" button (aria-label "Search
  syntax", `aria-expanded`) that toggles the help panel, and a × button (aria-label "Close
  search", only while the field has text). Esc in the field closes the search. ↓ in the field
  moves focus to the results list.
- `lib/searchSyntax.ts`: the help rows, which are also the source for the user guide table.

| Syntax | Meaning |
|---|---|
| `word` | Message contains it, or SHA starts with it |
| `"two words"` | Exact phrase |
| `author:NAME` | Author name or email |
| `committer:NAME` | Committer name or email |
| `message:TEXT` | Anywhere in the full message |
| `commit:SHA` | SHA starts with it |
| `path:PATTERN` | Changes a matching path (`src/`, `*.md`) |
| `contents:TEXT` | An added or removed line contains it |
| `after:YYYY-MM-DD` | Committed on or after that day |
| `before:YYYY-MM-DD` | Committed on or before that day |
| `branch:NAME` | Only commits reachable from NAME |

  Footer: "Terms combine with AND. Case is ignored except in path:."
- `components/CommitSearchResults.tsx`: flat rows exactly like `FileHistoryList` (badges, short
  id + summary, author, date), `role="listbox"` aria-label "Search results". Keyboard: ↑/↓ and
  j/k move the selection, Esc closes, Ctrl/Cmd+F focuses the field. The status line reads:
  - loading, nothing scanned yet: `Searching…`
  - loading: `Scanned 20,000 commits…`
  - done, no matches: `No commits match this search.`
  - done, walk finished: `1 match` / `N matches`
  - done, more to scan: `N matches in the first 40,000 commits`, plus a **Load more** button

  Numbers use `toLocaleString("en-US")`. An error renders `InlineError` with Retry (restart)
  and Dismiss (close the search) above the rows.
- `App.tsx`: the bar above the graph (hidden while file history is open). Results replace
  `CommitGraph` while the search has results state. `diffPaneCommits` prefers file history,
  then search results, then the graph.
- `CommitGraph`: an `onRequestSearch` prop, called on Ctrl/Cmd+F while the list has focus.
- Palette: "Search commits" (keywords search, find, filter, author, grep, log).
- Shortcut sheet, under "Commit history": `Ctrl/Cmd+F` — "Search commits".

## Testing

- `git-core` inline parser tests: each key, key case, quotes and escapes, bare words split,
  every error message above, leap day accepted, `2026-02-30` rejected.
- `git-core` `tests/search.rs`, real temp repos: one test per key (text in summary and body,
  text as SHA prefix, author name/email, committer distinct from author, message body,
  commit prefix, path directory and glob, contents added line and removed line and not on
  unchanged context, contents limited by path, after/before inclusive in the commit's own
  timezone, branch reachable-only and remote-tracking name, unknown branch), AND of two terms,
  `refs` selection with a missing ref skipped, merge judged against its first parent, unborn
  repo, paging without duplicates, scan budget pages, stale cursor, cancel before the walk and
  during it.
- `repo-service`: a search round-trips through the worker; an invalid query replies its
  message; a stale-generation command replies `Search cancelled.`; a search after
  `cancel_search` still runs.
- `tauri-app`: `SearchPageDto` serialization (camelCase, null cursor).
- `vscode-sidecar`: `search_commits` + `cancel_search` round-trip.
- Frontend (mock `RepoClient`): both clients' wiring; hook debounce, rounds, load more, stale
  response dropped, error, retry, close restores selection and cancels, reload on refresh; bar
  and results components; App: Ctrl/Cmd+F from the graph, results replace the graph, × restores;
  the palette entry.
- E2E Tauri (`e2e/specs/commit-search.spec.ts`): a commit with a distinct author, search
  `author:searchy`, open the result and see its author in the header, close back to the graph.

## Docs

- `docs/USER_GUIDE.md`: a new "Searching commits" section after "File history", with the syntax
  table.
- `CHANGELOG.md` under Unreleased: "Commit search".
- `docs/ARCHITECTURE.md`: a short "Commit search" subsection (generation-counter cancel, offset
  cursor with fingerprint).
- No `docs/LICENSE_COMPLIANCE.md` change.

## Out of scope

Regex, `OR`/negation, searching stashes or the reflog, persisted search history, context menus
on result rows, highlighting matched text, a VSCode E2E flow for search.
