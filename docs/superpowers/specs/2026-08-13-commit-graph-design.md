# Multi-Branch Commit Graph Design

Status: Approved

## Context

Branch management, stash, and blame (see `docs/superpowers/specs/2026-08-12-branch-management-design.md`,
`docs/superpowers/specs/2026-08-12-stash-design.md`, and `docs/superpowers/specs/2026-08-13-blame-design.md`)
shipped the first three of Phase 2's six subsystems. This spec covers the fourth: a multi-branch
commit graph. Phase 1's design explicitly deferred this ("Multi-branch commit graph rendering —
Phase 2. This phase's history view is a simple linear list for the current branch's HEAD") — this
spec is what fulfills that deferral. Phase 2's remaining two (merge, rebase) are out of scope
here, each its own future spec; this graph is also deliberate sequencing ahead of them, since
reviewing a merge or an interactive rebase is far easier with branch topology already visible.

**Goals:**
- Replace `HistoryList`'s flat, current-branch-only list with a swimlane graph — colored lines per
  branch, merge commits shown with connections to all parents, branch-tip labels on the commits
  that are a branch's current tip.
- Show commits reachable from **every local branch**, not just the current one — this is the
  point of "multi-branch."
- Full swimlane visual quality (like dedicated Git-client), not a simplified flat-list-with-badges
  fallback — matches this project's stated dedicated Git client inspiration (see `CLAUDE.local.md`).
- Everything `HistoryList` already does today carries over unchanged: the synthetic "Uncommitted
  Changes" row, stash rows (with their inline Apply/Drop buttons), keyboard navigation, and the
  right-click "Branch from here" context menu on commit rows. Only the *commit* rows themselves
  gain graph rendering; the graph structure otherwise doesn't touch selection, diffing, or blame —
  clicking a commit still selects it via the existing `SelectedRow` shape, unchanged.

**Non-goals (explicitly deferred):**
- Graph editing (drag-to-reorder, cherry-pick-by-drag, etc.) — that belongs to the future
  interactive-rebase subsystem, not this one.
- Remote-tracking branches in the graph — local branches only, matching branch management's own
  scope decision (`docs/superpowers/specs/2026-08-12-branch-management-design.md`'s non-goals).
- Pagination beyond a fixed commit cap — same non-goal Phase 1 already accepted for the flat log
  (`docs/superpowers/specs/2026-08-12-browsitory-phase1-design.md`); this spec keeps that
  ceiling, just seeded from every branch tip instead of one.

**Removed as part of this work** (dead code once the graph replaces its only caller):
`git-core::log` (the `log()` function and `CommitInfo` struct), the `get_log`/`GetLog`/`getLog`
operation at every layer (worker, Tauri command, `RepoClient`), and `HistoryList.tsx`/
`HistoryList.test.tsx` (renamed and rewritten, not just deleted — see Frontend section). Nothing
else in the codebase depends on any of these (verified: `CommitInfo`/`getLog` usage is confined to
`git-core`'s own log module/tests, the worker/commands/main wiring, `RepoClient`/`tauriRepoClient`,
`useAppState`, and `HistoryList` itself plus test files for all of the above).

## Architecture

### `git-core` addition: `graph.rs`

Same shape as other modules: a `thiserror` `GraphError` enum, tested against real temp-dir repos
(`crates/git-core/tests/graph.rs`).

```rust
pub struct GraphCommit {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub parent_ids: Vec<String>,  // all parents — a merge commit carries 2+
    pub branch_refs: Vec<String>, // local branch names whose tip is this commit; usually empty
}

pub fn graph_log(repo: &Repository, limit: usize) -> Result<Vec<GraphCommit>, GraphError>;
```

- First builds a `Oid → Vec<branch name>` map by enumerating local branches the same way
  `branch::list_branches` already does (`Repository::branches(Some(BranchType::Local))`), keyed by
  each branch's target `Oid` instead of by current-ness.
- Walks `Revwalk::push_glob("refs/heads/*")` (every local branch tip in one call — this is the
  "multi-branch" part) with `Sort::TOPOLOGICAL | Sort::TIME`, the same sort `log()` already uses,
  capped at `limit` commits — same fixed-ceiling behavior as today, just seeded from every branch
  tip instead of only `HEAD`.
- Per commit: the same `CommitInfo`-shaped fields via the same defensive accessor pattern (`id`,
  `short_id` as the fixed-7-char prefix, `summary`/`author_name`/`author_email` via
  `.ok().unwrap_or_default()`, `timestamp` via `commit.time().seconds()`), plus `parent_ids` from
  `Commit::parent_ids()` (mapped to hex strings) and `branch_refs` looked up from the tip map
  (empty `Vec` for any commit that isn't a branch tip).

### `tauri-app`: `Worker`/`Command` and Tauri command — replacement, not addition

`Command::GetLog` → `Command::GetCommitGraph { limit: usize, reply: Sender<Result<Vec<GraphCommit>, String>> }`.
`WorkerHandle::get_log` → `WorkerHandle::get_commit_graph`. The Tauri command `get_log` →
`get_commit_graph`, with a `GraphCommitDto` (camelCase: `id`, `shortId`, `summary`, `authorName`,
`authorEmail`, `timestamp`, `parentIds`, `branchRefs`) replacing `CommitInfoDto`. Registered in
place of `get_log` in `main.rs`, not alongside it.

### `RepoClient` / frontend IPC — replacement, not addition

```ts
export interface GraphCommit {
  id: string;
  shortId: string;
  summary: string;
  authorName: string;
  authorEmail: string;
  timestamp: number;
  parentIds: string[];
  branchRefs: string[];
}

getCommitGraph(limit: number): Promise<GraphCommit[]>;
```

`CommitInfo` and `getLog` are removed from `RepoClient.ts`/`tauriRepoClient.ts`, not kept
alongside the new type/method.

### Frontend state and components

`useAppState.ts`'s `log: CommitInfo[]` state field becomes `commits: GraphCommit[]`, fetched via
`client.getCommitGraph(...)` in the same `Promise.all` `refresh()` already uses for
status/branches/stashes.

A new pure utility, `frontend/src/lib/commitGraphLayout.ts`, exports
`assignLanes(commits: GraphCommit[]): CommitLayout[]` where
`CommitLayout = { commitId: string; lane: number; parentConnections: { parentId: string; lane: number }[] }`.
This is the real generic lane-assignment algorithm (per the approved approach — not a
mostly-linear-only heuristic): it processes commits in the topologically-sorted order `graph_log`
already returns, tracks which lanes are "waiting" for which commit id next, assigns each commit to
an existing waiting lane if one matches or opens a new lane otherwise, and — for merge commits —
continues/opens one lane per parent. This function has no DOM/React dependency, so it's unit-tested
in complete isolation from rendering.

`HistoryList.tsx`/`HistoryList.test.tsx` are renamed to `CommitGraph.tsx`/`CommitGraph.test.tsx`
— the component now does something categorically different (graph vs. flat list), so its name
should say so. Same props shape as today except `log: CommitInfo[]` becomes
`commits: GraphCommit[]`. Every existing responsibility (Uncommitted Changes row, stash rows,
keyboard nav, right-click "Branch from here") is preserved unchanged; commit rows additionally
render a lane column (colored connector lines from `assignLanes`' output, via inline SVG) and any
branch-ref badges for commits that are a branch tip. The Uncommitted-Changes row and stash rows
render exactly as they do today, outside the lane graph — they have no place in commit topology.

**Hard backward-compatibility constraint, not optional polish:** all four existing E2E specs
(`e2e/specs/first-flow.spec.ts`, `branch-management.spec.ts`, `stash-management.spec.ts`,
`blame-viewer.spec.ts`) locate commit rows via a plain WebdriverIO substring match against an
`<li>`'s text content — `$("li*=e2e: first commit")`, `$("li*=e2e: blame fixture second
commit")`, etc. — and `blame-viewer.spec.ts` additionally asserts `aria-selected="true"`/`"false"`
directly on that `<li>`. Each commit row's outermost element must therefore stay an `<li>` whose
full text content still includes `{shortId} {summary}` as a plain substring (the new lane SVG and
branch-ref badges can be sibling/child elements alongside that text, e.g. an SVG contributes no
text content at all, and badge text should not be inserted *between* `shortId` and `summary`), and
`aria-selected` must stay set on that same `<li>`. This isn't a testing nicety — it's the thing
that keeps three already-shipped, already-reviewed features' E2E coverage from silently breaking
the moment this component's internals change. The implementation plan must call this out
explicitly in whichever task rewrites the commit-row markup, and Task testing must include running
the full existing E2E suite (not just the new graph flow) before considering that task done.

### Error handling

No new plumbing: graph errors flow through the same `Result<T, String>` → rejected promise →
`state.error` → inline banner path every other feature already uses.

### Testing

- `crates/git-core/tests/graph.rs`: two branches diverging from a common ancestor (both tips
  appear in the result with correct `branch_refs`), a merge commit (`parent_ids` has 2 entries),
  `branch_refs` empty for a non-tip commit, and the `limit` cap actually caps.
- `crates/tauri-app/src/worker.rs`: one thin wiring round-trip test, matching the economical scope
  every other feature's worker tests already use.
- `frontend/src/lib/commitGraphLayout.test.ts` (new): pure unit tests — a linear history assigns
  everything to lane 0, a fork opens a second lane, a merge commit's lanes correctly rejoin. No
  DOM/React involved.
- `frontend/src/components/CommitGraph.test.tsx` (renamed from `HistoryList.test.tsx`): every
  pre-existing behavior re-verified under the new props/name (Uncommitted Changes row, stash rows,
  keyboard nav, branch context menu), plus new tests for branch-ref badge rendering.
- One new E2E flow (`e2e/specs/`): create two branches with divergent commits, open the graph view,
  confirm commits from both branches are visible with correct branch-ref labels.
