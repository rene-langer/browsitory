# Remote Branch Labels in Commit Graph

## Goal

`CommitGraph` already labels each commit with the local branches that point
at it (`GraphCommit.branchRefs`, rendered as pill badges in
`CommitGraph.tsx`). Remote-tracking branches (`origin/main`,
`origin/feature-x`, …) have no equivalent — they're only visible in
`BranchTree`'s separate remote folders, disconnected from the graph view.
Sublime Merge shows both local and remote tips inline on the commit tree;
this design brings Browsitory's graph view to parity.

Scope decision: label existing commits only. A commit is labeled with a
remote branch's name only if that commit is already in the graph (i.e.
reachable from some local branch via the existing revwalk). Commits reachable
only from a remote ref (never fetched into a local branch) are out of scope —
no revwalk change, no risk of pulling in large unfetched history.

## Shared architecture

No change to the existing boundary:

`React components/state -> RepoClient -> {Tauri command, vscode-sidecar
command} -> repo-service worker -> git-core`

This feature touches every layer of that chain (unlike the sidebar/tree
design, which was frontend-only), because it adds one new commit field and
one new piece of persisted per-repo UI state, both of which cross the IPC
boundary and exist in both host binaries (`tauri-app`, `vscode-sidecar`).

## Part 1: Backend — remote tips on `GraphCommit`

`crates/git-core/src/graph.rs`:

- `GraphCommit` gains `remote_branch_refs: Vec<String>`.
- `graph_log` builds a second `tips_by_oid: HashMap<Oid, Vec<String>>` from
  `repo.branches(Some(BranchType::Remote))`, same shape as the existing local
  one. Skip symbolic refs (`origin/HEAD`) — `git2` represents these as a
  `Branch` whose `get().kind()` is `Some(ReferenceType::Symbolic)` rather
  than `Direct`; only direct refs get a target `Oid` to key into the map by
  anyway, so this is a natural filter, not a special case.
- `branch.name()` already returns the qualified form (`origin/main`), so no
  extra formatting needed.
- Revwalk (`push_glob("refs/heads/*")` / per-name `push_ref`) is unchanged —
  remote refs never seed the walk.
- Populate `remote_branch_refs` from this new map the same way
  `branch_refs` is populated from the local one, per commit.

## Part 2: Persisted remote badge visibility

Mirrors the existing local `graphBranchSelection` persistence exactly, as a
parallel, independent setting (not merged into the local one — local
selection also drives the revwalk query; remote selection only ever filters
badge rendering client-side, so keeping them separate avoids overloading one
field with two meanings).

- `crates/config/src/lib.rs`: new `GraphRemoteBranchSelection` struct
  (same shape as `GraphBranchSelection`), new field
  `graph_remote_branch_selections: Vec<GraphRemoteBranchSelection>` on the
  config struct with `#[serde(default)]` (old config files without the key
  deserialize to an empty `Vec`). New
  `get/set_graph_remote_branch_selection(_at)` functions, same pattern as
  the local pair.
- `crates/tauri-app/src/commands/status.rs`: new `get_graph_remote_branch_selection`
  / `set_graph_remote_branch_selection` Tauri commands wrapping the config
  functions; registered in `main.rs`'s command list.
- `crates/vscode-sidecar`: same two commands added to its dispatch table
  (mirrors however it currently exposes `get_graph_branch_selection` —
  follow that file's existing pattern).
- `frontend/src/ipc/RepoClient.ts`: interface gains
  `getGraphRemoteBranchSelection(repoPath): Promise<string[] | null>` and
  `setGraphRemoteBranchSelection(repoPath, selected: string[]): Promise<void>`.
- `tauriRepoClient.ts` / `vscodeRepoClient.ts`: thin wrappers calling the two
  new commands, same as the local pair.

Selected values are qualified remote branch names (`"origin/main"`) —
same string form as `GraphCommit.remoteBranchRefs` entries, so no lookup
needed to compare them.

## Part 3: Frontend state and rendering

- `frontend/src/ipc/RepoClient.ts`: `GraphCommit.remoteBranchRefs: string[]`.
- `useAppState.ts`: new `graphRemoteBranchSelection: string[] | null` loaded
  alongside `graphBranchSelection` on repo open (parallel `client.get...`
  call), and a setter that updates state and calls
  `client.setGraphRemoteBranchSelection`. `null` means "show all", matching
  the local field's convention. This filtering never touches
  `getCommitGraph`'s params — it's applied client-side only, in
  `CommitGraph`.
- `BranchTree.tsx`: `renderRemoteNodes` gets a swatch/toggle button on each
  branch row, visually matching the local row's (`toggleGraphBranch` /
  `branchSwatchColor`), wired to a new `toggleGraphRemoteBranch(qualifiedName)`
  that reads/writes `graphRemoteBranchSelection` the same way
  `toggleGraphBranch` does for the local field.
- `CommitGraph.tsx`: second badge loop after the existing `branchRefs.map`:
  `commit.remoteBranchRefs.filter(shown).map(...)`, where `shown` mirrors the
  existing local computation
  (`graphRemoteBranchSelection ?? <all remote names present in props>`).
  New CSS class `.remoteBranchBadge` in `CommitGraph.module.css`: same as
  `.branchBadge` but `border-style: dashed` — distinct at a glance, still
  muted, no new color needed.

## Testing

- `git-core/tests/graph.rs`: remote tips populate `remote_branch_refs`;
  `origin/HEAD` excluded; a commit with both a local and remote tip gets
  both fields populated independently.
- `config`: unit tests for `get/set_graph_remote_branch_selection(_at)`,
  including loading an old config file missing the new key.
- `repo-service`: DTO mapping test covers the new field passthrough.
- Frontend: `CommitGraph.test.tsx` (remote badges render, filtered by
  selection), `BranchTree.test.tsx` (remote row toggle), `useAppState.test.ts`
  (load + persist round-trip), `RepoClient` wiring tests for both
  `tauriRepoClient.test.ts` and `vscodeRepoClient.test.ts`.
- e2e: extend whichever existing spec already asserts on graph/branch badge
  content (both `e2e/` and `extension/e2e/`) rather than adding a new spec
  file, if one already covers this area.
