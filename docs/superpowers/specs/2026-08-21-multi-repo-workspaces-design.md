# Multi-Repo Workspaces Design

## Overview

Multi-repo tabs (`docs/superpowers/specs/2026-08-20-multi-repo-tabs-design.md`) let several
repos be open at once as independent tabs, but each has to be opened one at a time via the
folder dialog, and there is no relationship between tabs beyond "open in the same window" —
grouping was an explicit non-goal of that design.

This design adds **workspaces**: a saved, named set of repos discovered under one root
folder (e.g. a "projects" folder holding several service checkouts, or a second full clone
of the same set of repos on a different path). Opening a workspace opens all its member repos
as tabs in one action, visually clustered together in the tab strip.

## Goals

- Discover git repos under a chosen root folder (its immediate children) and let the user
  pick which ones to include.
- Save that selection as a named workspace, persisted across restarts.
- Open a saved workspace in one action, reopening all its member repos as tabs.
- Visually cluster a workspace's tabs together in the tab strip, with a group-level close-all.
- Edit an existing workspace's membership and name later; delete a workspace.
- Support multiple distinct workspaces whose member repos may share directory names but live
  under different roots (e.g. two full clones of the same multi-repo project) without
  collision.

## Non-goals

- Cross-workspace or cross-repo operations (unchanged from the multi-repo-tabs design).
- Recursive/multi-level scanning — only a root's immediate children are checked for a `.git`
  entry. Deeper layouts are out of scope for this pass.
- Auto re-sync of a workspace's membership when the root folder's contents change on disk —
  membership is a snapshot taken at save/edit time, not a live scan. Picking up new or removed
  repos requires explicitly re-editing the workspace.
- Tab drag-reorder — no such feature exists today for standalone tabs, and this design doesn't
  add one for workspace tabs either.
- Workspace-level settings/config beyond name and membership (e.g. no per-workspace theme).

## Current State

- `crates/config/src/lib.rs`: persists `recent_repos: Vec<PathBuf>` and, since multi-repo
  tabs, `open_repos: Vec<PathBuf>` + `active_repo: Option<PathBuf>`, all in one
  `<os-config-dir>/config.toml` via a `ConfigFile` struct and `_at`-suffixed testable
  functions (`list_recent_repos_at`, `add_recent_repo_at`, etc., wrapped by path-defaulting
  public functions).
- `crates/tauri-app/src/commands.rs`: `AppState.workers: Mutex<HashMap<String, Worker>>`
  keyed by canonicalized repo path. `open_repo`/`close_repo` add/remove entries.
  `list_open_repos`/`persist_open_repos` wrap the config crate's open-repos persistence.
  `pick_repo_folder` opens a native folder-picker dialog and returns the chosen path (or
  `None`) — used today to pick a single repo's root directly.
- `frontend/src/state/useOpenRepos.ts`: owns the `OpenRepo[]` tab list (`{ path,
  displayName }`) and `activePath`, restores from `listOpenRepos` on mount (re-opening each
  path for real, dropping any that fail), and persists on every open/close/switch.
- `frontend/src/components/RepoTabs.tsx`: renders one tab per `OpenRepo`, a close control per
  tab, and a trailing `+` opening `RepoPicker` in an overlay.
- `frontend/src/components/RepoPicker.tsx`: "Open Folder" button (calls `pickRepoFolder` then
  `onOpenRepo`) plus a flat list of recent repos, each clickable to open.

## Architecture

### Data model

A new `Workspace` record, stored alongside the existing config:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Workspace {
    id: String,             // uuid v4, stable identity independent of name or root
    name: String,           // user-editable; defaults to root's basename at save time
    root_path: PathBuf,
    member_paths: Vec<PathBuf>,  // snapshot chosen at save/edit time, not re-scanned
}
```

`ConfigFile` gains `#[serde(default)] workspaces: Vec<Workspace>`.

Identity is the `id`, not `name` or `root_path` — this is what makes the multi-clone case
(two workspaces whose roots contain identically-named subfolders) safe: nothing in storage or
in the open-repo registry keys on repo *name*, only on full paths, and workspaces themselves
are looked up by `id`.

The existing `open_repos` persistence (`Vec<PathBuf>` today) becomes a small struct per entry
so tab-group membership survives a restart, not just the initial open:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenRepoEntry {
    path: PathBuf,
    #[serde(default)]
    workspace_id: Option<String>,
}
```

`ConfigFile.open_repos` changes from `Vec<PathBuf>` to `Vec<OpenRepoEntry>`. `#[serde(default)]`
on the new field means old config files (bare path list under the old shape) fail to deserialize
under the new struct shape — since `open_repos` itself changes type, this needs a manual
migration: a custom `Deserialize` (or a `#[serde(untagged)]` enum accepting either the old
`Vec<PathBuf>` shape or the new one, normalizing to the new shape) so an existing user's
persisted tabs aren't silently dropped on first launch after upgrade.

### Backend

New `crates/config` functions, following the existing `_at`-suffixed testable-seam pattern:

- `scan_repos_in_root_at(root: &Path) -> Result<Vec<PathBuf>, ConfigError>` — reads `root`'s
  immediate directory entries, returns those containing a `.git` entry (dir or file, to allow
  worktrees/submodules), sorted for stable checklist ordering. (This one doesn't touch
  `config.toml`, but lives in `config` alongside the other repo-path logic rather than in
  `tauri-app`, keeping `commands.rs` a thin IPC layer as today.)
- `list_workspaces_at(path: &Path) -> Result<Vec<Workspace>, ConfigError>`
- `save_workspace_at(path: &Path, name: &str, root: &Path, members: &[PathBuf]) -> Result<String, ConfigError>`
  — generates a new `id` (uuid v4), dedupes `name` against existing workspace names by
  appending `" (2)"`, `" (3)"`, ... on collision, returns the new `id`.
- `update_workspace_at(path: &Path, id: &str, name: &str, members: &[PathBuf]) -> Result<(), ConfigError>`
  — root_path is immutable after creation (editing membership re-scans/re-picks from the same
  root; changing root is "delete and create a new workspace").
- `delete_workspace_at(path: &Path, id: &str) -> Result<(), ConfigError>`

Each gets a public path-defaulting wrapper (`scan_repos_in_root`, `list_workspaces`, etc.),
matching `list_recent_repos`/`list_recent_repos_at` today.

New `#[tauri::command]`s in `commands.rs` thinly wrapping each: `scan_repos_in_root`,
`list_workspaces`, `save_workspace`, `update_workspace`, `delete_workspace`. No change to
`AppState`, `open_repo`, `close_repo`, or `worker_handle` — workspace membership is a
frontend-side grouping concept layered on top of the existing per-path worker registry, not a
new backend repo-scoping mechanism.

`persist_open_repos` signature changes to accept the new per-entry shape (`Vec<{ path,
workspace_id }>` from the frontend) instead of `Vec<String>`; `list_open_repos` returns it
symmetrically.

### Frontend

`RepoClient` gains: `scanReposInRoot(root): Promise<string[]>`, `listWorkspaces():
Promise<Workspace[]>`, `saveWorkspace(name, root, members): Promise<string>`,
`updateWorkspace(id, name, members): Promise<void>`, `deleteWorkspace(id): Promise<void>`.
`persistOpenRepos`/`listOpenRepos` signatures updated for the `workspaceId`-carrying shape.

`RepoPicker` gains:
- An "Open Workspace Root" button next to "Open Folder": native folder pick →
  `scanReposInRoot` → a checklist overlay (all pre-checked) → a name field (prefilled from the
  root's basename) → confirm calls `saveWorkspace`, then opens every checked path as a tab
  (see below).
- A "Workspaces" list section (same `ListRow` styling as today's Recent Repos list) below the
  recent-repos list: each row shows the workspace name (root path as tooltip, matching the
  existing tab-tooltip convention) with an **Open All**, **Edit**, and **Delete** action. Edit
  reopens the checklist pre-checked against the workspace's current `member_paths` (plus any
  new repos `scanReposInRoot` finds under the same root that weren't previously members),
  letting the user add/remove and rename, then calls `updateWorkspace`.

`useOpenRepos`:
- `OpenRepo` gains `workspaceId?: string`.
- New `openWorkspace(workspace: Workspace): Promise<void>` — calls the existing single-path
  `openRepo` logic for each member sequentially (reusing its per-path open/dedupe/persist
  behavior), tagging each resulting `OpenRepo` with `workspace.id`. A member path that fails
  to open (moved/deleted since the workspace was saved) is skipped, same silent-drop rule the
  restore-on-relaunch path already uses — not a new error case to design for.
- Restore-on-mount reads the new `workspaceId`-carrying `listOpenRepos` shape and threads it
  straight into the restored `OpenRepo[]`, so grouping survives a relaunch without re-deriving
  it from `listWorkspaces`.

`RepoTabs`: groups the `openRepos` array by finding runs of consecutive entries sharing the
same non-null `workspaceId` (matches how they naturally end up contiguous — `openWorkspace`
opens all members back-to-back with nothing else opened in between) and wraps each run in a
small name-chip + divider, with a close-all control on the chip that calls `closeRepo` for
every path in that run. Entries with no `workspaceId`, and any workspace tab that's drifted
out of a contiguous run (e.g. closed and reopened individually later), render as plain
standalone tabs — grouping is a best-effort visual affordance, not a structural guarantee.

## Edge Cases

- **Root no longer exists / unreadable at scan time**: `scanReposInRoot` surfaces the IPC
  error to the picker overlay, same as `pickRepoFolder` failures today — no new error UI
  pattern needed.
- **A workspace member fails to open** (moved, deleted, permissions changed since save):
  skipped silently when opening the workspace, exactly like restore-on-relaunch's existing
  per-path drop rule.
- **Duplicate workspace names**: deduped automatically at save time (`"project"`, `"project
  (2)"`); editable afterward, not enforced unique beyond the auto-suffix.
- **Two workspaces with overlapping/identical member repo names under different roots** (the
  multi-clone case): no collision anywhere — the worker registry keys on full canonicalized
  path, workspaces key on `id`, and the tab strip's chip label is the workspace `name`, not
  derived from member names.
- **Closing the last tab of a workspace's run**: the group naturally has fewer tabs; if all
  are closed, the chip and divider simply have nothing left to wrap and disappear. The saved
  `Workspace` record itself is untouched by closing tabs (only "Delete" in the picker removes
  the saved workspace).
- **Old config.toml from before this change** (`open_repos: Vec<PathBuf>`): handled by the
  flexible deserialization noted under Data model, normalizing to `workspace_id: None` for
  every previously-open path.

## Testing

- **Rust**: `scan_repos_in_root_at` against a fixture directory with a mix of git and
  non-git children (including a bare `.git` file, for worktree/submodule-style children);
  `save_workspace_at`/`update_workspace_at`/`delete_workspace_at`/`list_workspaces_at`
  round-trip tests including the name-dedup-on-collision case; a deserialization test loading
  an old-shape `open_repos: Vec<PathBuf>` TOML fixture and asserting it normalizes correctly.
- **Frontend unit**: `useOpenRepos.test.ts` extended with `openWorkspace` (tagging, partial-
  failure skip) and restore-with-`workspaceId` cases; `RepoTabs.test.tsx` extended with
  grouped-run rendering (chip, divider, close-all) and the drifted-tab-renders-standalone
  case; `RepoPicker.test.tsx` extended with the workspace-creation flow and the Workspaces
  list's Open All/Edit/Delete actions.
- **E2E**: a new `workspaces.spec.ts` fixture sets up a root directory containing 2-3 fixture
  repos. Covers: scanning and checklist selection, saving, tab-strip chip grouping after Open
  All, close-all on the chip, editing membership (add/remove a repo, resave), deleting a
  workspace, and restart-restore of both the saved workspace list and an open workspace's tab
  grouping.

## Rollout

One implementation plan. Natural task order: config-crate data model + CRUD functions first
(testable in isolation via Rust tests, including the `open_repos` shape migration since that
touches existing persisted user data), then the scan function, then the new Tauri commands,
then `RepoClient`/`useOpenRepos` threading, then the `RepoPicker` creation/management UI, then
`RepoTabs` grouping, then the E2E spec last.
