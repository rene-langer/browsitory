# Browsitory Phase 1 Design

Status: Approved

## Context

Phase 0 (see `docs/superpowers/specs/2026-08-11-browsitory-architecture-design.md`) stood up
the workspace: `git-core::repo`/`status`, a Tauri shell, and a `RepoClient` IPC boundary with a
minimal status view. Nothing in the frontend calls `openRepo`, so the shipped app always shows
an empty/error status view — Phase 0's own scope explicitly stopped short of a usable app.

Phase 1, per `docs/ARCHITECTURE.md`'s roadmap: "full repo view — commit history, diff viewer,
stage/unstage, commit." This spec designs that slice as one coherent unit, following the same
`git-core` / `tauri-app` / `frontend` layering Phase 0 established. A fast, keyboard-driven
workflow with polished commit history, diff, and staging views is a stated product goal (see
`CLAUDE.local.md`); this design keeps that goal in view without naming any specific reference
product.

**Goals:**
- A user can pick a repo (folder picker + recent-repos list), see its commit history, see the
  diff for any commit or for uncommitted changes, stage/unstage whole files, and commit.
- One unified list (a synthetic "Uncommitted Changes" row plus real commits) driving one shared
  diff pane, rather than separate tabs — this is the layout Phase 2+ features (blame, graph)
  build on.
- Basic keyboard navigation (up/down through the list, Cmd/Ctrl+Enter to commit) ships now
  rather than being retrofitted later.

**Non-goals (explicitly deferred):**
- Hunk-level or line-level staging — whole-file only this phase.
- Word-level diff highlighting — line-level (added/removed/context) only this phase.
- Multi-branch commit graph rendering — Phase 2. This phase's history view is a simple linear
  list for the current branch's HEAD.
- Amend-last-commit, sign-off, or any commit option beyond message + commit.
- Pagination/virtualization for very large histories — the log call is capped at a fixed limit
  (see below); revisit if it proves insufficient.

## Architecture

### `git-core` additions

Four new modules alongside the existing `repo.rs`/`status.rs`, same style: free functions
taking `&git2::Repository` (or a path), `thiserror` error enums per module, tested against real
temp-dir repos.

**`log.rs`**
```rust
pub fn log(repo: &Repository, limit: usize) -> Result<Vec<CommitInfo>, LogError>;

pub struct CommitInfo {
    pub id: String,           // full hex OID
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,       // Unix seconds, UTC — frontend formats for display
}
```
Walks a `Revwalk` from HEAD in `Sort::TOPOLOGICAL | Sort::TIME` order, stops at `limit` commits.
No pagination in this phase — `limit` is a fixed cap (e.g. 300) chosen by the caller; a repo
with a longer history simply doesn't show older commits yet.

**`diff.rs`**
```rust
pub fn working_diff(repo: &Repository, path: &str, staged: bool) -> Result<Vec<DiffHunk>, DiffError>;
pub fn commit_diff(repo: &Repository, commit_id: &str, path: &str) -> Result<Vec<DiffHunk>, DiffError>;

pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

pub struct DiffLine {
    pub origin: DiffLineOrigin,  // Add | Remove | Context
    pub content: String,
}
```
`working_diff(staged: true)` diffs HEAD tree vs index for that path; `staged: false` diffs index
vs workdir. `commit_diff` diffs the commit's tree against its first parent's tree for that path
(a merge commit's diff is just "vs first parent" — no combined/conflict view; fine, since there's
no merge UI yet). Both build on `git2::Repository::diff_tree_to_index` /
`diff_index_to_workdir` / `diff_tree_to_tree`, converted to the flat `DiffHunk`/`DiffLine` shape
via each `git2::Diff`'s hunk/line callbacks.

**`stage.rs`**
```rust
pub fn stage_file(repo: &Repository, path: &str) -> Result<(), StageError>;
pub fn unstage_file(repo: &Repository, path: &str) -> Result<(), StageError>;
```
`stage_file`: `repo.index()?.add_path(path)` + `index.write()`. `unstage_file`:
`repo.reset_default(Some(&head_commit.into_object()), [path])` (or index removal for a file with
no HEAD history yet — an added-then-unstaged new file). Whole-file only.

**`commit.rs`**
```rust
pub fn commit(repo: &Repository, message: &str) -> Result<String, CommitError>; // returns new commit's OID as hex
```
Writes the current index as a tree (`repo.index()?.write_tree()`), uses `repo.signature()` for
author/committer (reads `user.name`/`user.email` from git config — errors clearly if unset),
parents = `[HEAD]` if HEAD resolves, or `[]` for a repo's first commit.

### `config` crate (stops being a stub)

```rust
pub fn list_recent_repos() -> Result<Vec<PathBuf>, ConfigError>;
pub fn add_recent_repo(path: &Path) -> Result<(), ConfigError>;
```
One TOML file in the OS config dir (via the `directories` crate, per the original stack table),
holding a simple ordered list of paths (most-recent-first, capped at e.g. 10, de-duplicated on
add). No preferences beyond the recent-repos list this phase.

### Worker / Tauri commands

`crates/tauri-app/src/worker.rs`'s `Command` enum grows one variant per new `git-core` function
— `GetLog { limit, reply }`, `GetWorkingDiff { path, staged, reply }`,
`GetCommitDiff { commit_id, path, reply }`, `StageFile { path, reply }`,
`UnstageFile { path, reply }`, `Commit { message, reply }` — following `GetStatus`'s existing
shape exactly (each Tauri command clones the `WorkerHandle`'s `Sender` and drops the state lock
before blocking on the reply, per Phase 0's fix).

`commands.rs` gains matching `#[tauri::command]`s, plus two that don't go through the worker
(no repo is open yet when they're called): `pick_repo_folder` (wraps
`@tauri-apps/plugin-dialog`'s folder-picker dialog, called from the Rust side via that plugin's
Tauri API) and `list_recent_repos` / `add_recent_repo` (call `crates/config` directly). DTOs
mirror the `git-core` types 1:1 (`CommitInfoDto`, `DiffHunkDto`, `DiffLineDto` with `origin` as a
`String`, following `StatusEntryDto`'s pattern), each with a wire-format-pinning test like
`commands.rs`'s existing `StatusKind` test.

### `RepoClient` IPC boundary

`frontend/src/ipc/RepoClient.ts` grows:
```ts
export type DiffLineOrigin = "Add" | "Remove" | "Context";
export interface DiffLine { origin: DiffLineOrigin; content: string; }
export interface DiffHunk {
  oldStart: number; oldLines: number; newStart: number; newLines: number;
  lines: DiffLine[];
}
export interface CommitInfo {
  id: string; shortId: string; summary: string;
  authorName: string; authorEmail: string; timestamp: number;
}

export interface RepoClient {
  pickRepoFolder(): Promise<string | null>;
  listRecentRepos(): Promise<string[]>;
  openRepo(path: string): Promise<void>;
  getStatus(): Promise<StatusEntry[]>;
  getLog(limit: number): Promise<CommitInfo[]>;
  getWorkingDiff(path: string, staged: boolean): Promise<DiffHunk[]>;
  getCommitDiff(commitId: string, path: string): Promise<DiffHunk[]>;
  stageFile(path: string): Promise<void>;
  unstageFile(path: string): Promise<void>;
  commit(message: string): Promise<void>;
}
```
`tauriRepoClient.ts` remains the only implementation, and the only file importing
`@tauri-apps/api`/`@tauri-apps/plugin-dialog` — the `no-restricted-imports` ESLint rule from
Phase 0 already covers this for any new file under `components/`/`state/`.

### Frontend components and state

`frontend/src/state/` gets its first real content — Phase 0 only had the constraint, not the
code. A small reducer-backed hook (`useAppState`, or similar) owns:
- `repoPath: string | null`
- `selectedRow: "uncommitted" | { commitId: string }`
- `status: StatusEntry[]`, `log: CommitInfo[]`

Every mutation (`stageFile`, `unstageFile`, `commit`) triggers a `status`+`log` refetch through
this hook — no component reaches into `RepoClient` and manages its own copy of shared state.

Components (`frontend/src/components/`):
- **`RepoPicker`** — shown when `repoPath` is null. Recent-repos list (from `listRecentRepos`)
  plus an "Open Folder" button (`pickRepoFolder` → `openRepo`).
- **`HistoryList`** — a synthetic "Uncommitted Changes" row on top (badge = `status.length`),
  then one row per `CommitInfo`. Up/down (or j/k) arrow keys move `selectedRow`; click also
  selects.
- **`DiffPane`** — renders based on `selectedRow`:
  - `"uncommitted"`: file list from `status`, each with stage/unstage buttons, each file's
    `working_diff` shown via `DiffView` when selected within the pane; `CommitBox` below (message
    textarea, Commit button, Cmd/Ctrl+Enter, disabled when nothing is staged).
  - `{ commitId }`: that commit's changed file list (derived from `commit_diff` against each
    changed path — or a simpler "list files changed in this commit" `git-core` helper if that's
    cleaner; implementer's call within `commit_diff`'s natural boundary) with each file's diff,
    read-only — no stage/unstage controls, no `CommitBox`.
- **`DiffView`** — shared, dumb rendering component: takes `DiffHunk[]`, renders hunk headers and
  per-line add/remove/context styling. Used by both branches of `DiffPane`.

## Error handling

Same pattern as Phase 0 throughout: `git-core` typed errors (`thiserror`) → `Worker`/Tauri
commands map to `Result<T, String>` → `RepoClient` promises reject, nothing swallowed. Every new
frontend data fetch (`getLog`, `getWorkingDiff`, `getCommitDiff`) follows `StatusView`'s Phase-0
fix: explicit error state, rendered, never silently falling through to an empty-looking view.

## Testing strategy

- `git-core`: one test file per new module (`tests/log.rs`, `tests/diff.rs`,
  `tests/stage_commit.rs`), real temp-dir repos, no mocks — covering at minimum what the prior
  egui-era MVP covered (log ordering, added/removed/context diff lines, stage-then-commit round
  trip), plus regression coverage for the two edge cases Phase 0 already found (rename detection,
  multi-entry status for a path that's both staged and further modified) so this phase's new code
  doesn't reintroduce them.
- `tauri-app`: `commands.rs` keeps the DTO wire-format pinning test pattern for each new type
  crossing the IPC boundary (e.g. `DiffLineOrigin` strings pinned against `RepoClient.ts`'s
  union), same rationale as Phase 0's `StatusKind` test.
- `frontend`: Vitest + Testing Library per component, `RepoClient` mocked — never
  `@tauri-apps/api`/`@tauri-apps/plugin-dialog` directly.
- **E2E starts this phase**, per the Phase 0 architecture spec's roadmap note — via
  `tauri-driver` + WebdriverIO, not Playwright (corrected from the original roadmap note:
  Playwright drives browser engines it manages itself and can't attach to a native
  Tauri/webkit2gtk window; `tauri-driver` is Tauri's own WebDriver bridge, the actually-supported
  path — `docs/ARCHITECTURE.md`'s testing-strategy section carries the same correction). One
  flow: open a repo → see its status → stage a file → commit → see the new commit in history.
  Drives the real `cargo tauri dev` build; this is the one flow that spans backend+frontend in a
  way unit tests can't catch (the actual IPC round-trip, the actual git operations).

## Open questions for the implementation plan

None blocking — the one implementer-judgment call noted above (deriving a commit's changed-file
list) is small enough to resolve during task-writing rather than here.
