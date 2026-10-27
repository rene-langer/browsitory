# Feature parity, batch 4b: refs and remotes

## Goal

Show tags on commit graph rows. Delete a remote branch. Push any local branch to any remote
branch, optionally setting it as upstream ("publish"). Fetch every remote at once, with prune.
Show ahead/behind counts per local branch. Both frontends (Tauri app, VSCode extension) go through
`RepoClient`. Refines section 4b of
`docs/superpowers/specs/2026-10-27-parity-batch4-roadmap-design.md` (issue #91). Also fixes #97
(see Decisions).

## Current state

- `git-core::graph::RefDecorations::load` collects local and remote branch tips (direct refs only)
  into `HashMap<Oid, Vec<String>>`. `RefDecorations::graph_commit` builds every `GraphCommit`, so
  `graph_log`, file history (`history.rs`) and search (`search.rs`) share one decoration pass.
  `GraphCommit` has `branch_refs` and `remote_branch_refs`. It has no tags.
- `graph_log` calls `revwalk.push_ref("refs/heads/<name>")?` for every selected branch. One
  missing name fails the whole call (#97). `useAppState.refresh()` puts `getCommitGraph` in a
  single `Promise.all`, so the whole refresh fails until the selection changes. Nothing prunes the
  persisted selection on branch delete or rename (`useBranchActions` only sets it).
- `git-core::remote`:
  - `push_current_branch(repo, remote, credentials, reporter, cancel)` pushes
    `refs/heads/<current>:refs/heads/<current>` through the private `push_refs` (negotiation-time
    cancel, per-ref rejection mapping, optional lease). It always pushes to the **same name**,
    even when the current branch tracks a differently named upstream branch on that remote. The
    sync bar's Push button says "Push to origin/feature-x" but pushes `origin/<local name>`.
    `force_push_current_branch` has the same mismatch in both its refspec and its lease.
  - `fetch_remote(repo, remote, operation_id, credentials, reporter, cancel)` uses default
    `FetchOptions`. Its `prune` is `FetchPrune::Unspecified`, so libgit2 already reads
    `remote.<name>.prune` and then `fetch.prune` (libgit2 1.9, `lookup_remote_prune_config`).
    Every progress record says `TransferOperation::Fetch`.
  - `ahead_behind(repo, local, remote, remote_branch)` (private) gives `(Option, Option)`.
    `current_upstream` reads `branch.<name>.remote` / `.merge` from config.
  - `set_current_upstream` uses `Branch::set_upstream`, and writes the two config keys itself
    when the tracking ref does not exist yet.
- `git-core::branch::BranchInfo` is `{ name, is_current }`.
- `repo-service` transfers: each `Command::{FetchRemote, PushCurrentBranch, …}` handler sends
  `TransferEvent::Started`, replies with the operation id, runs, then `complete()` sends
  `TransferEvent::Completed { operation_id, operation, error: Option<TransferErrorKind> }`.
  `TransferOp` carries the id and the shared `CancelRegistry`. Ids are `fetch-N`, `push-N`, …
  from `NEXT_TRANSFER_ID`.
- Tauri and the sidecar turn `TransferEvent` into `TransferProgressDto` (`transfer-progress` /
  `transfer-complete` events; sidecar `transferProgress` notifications). The frontend's
  `useRemoteTransferActions` tracks one active operation id. `Completed` refreshes, and `Failed`
  sets `state.error` from `transferFailureMessage`. After a `PushBranch` failure with
  `NonFastForward`, it offers "Force push (with lease)", which force-pushes the **current**
  branch.
- Undo snapshots cover `refs/heads/*` and `refs/tags/*` only (`git-core::undo`). Remote-tracking
  refs and branch config do not invalidate the undo stack.
- `BranchTree` loads a remote's branch list lazily into a `remoteBranches` cache. Only the remote
  folder's own "Fetch" item clears that cache. A fetch from the sync bar or palette, or a push
  that creates a remote branch, leaves an open folder stale until the app restarts.
- `SyncBar`: Fetch (upstream's remote, else the first remote), Pull and Push (disabled without an
  upstream). It shows the current branch's `↑n ↓m`.
- `TagPanel` already creates, deletes (with `ConfirmDialog` "Delete local tag X?") and pushes tags.
- `ContextMenu` items are plain `menuitem` buttons. `ConfirmDialog` styles its confirm button as
  danger. `FormDialog` is the shell for small forms.

## Decisions

- **No new dependency.** Everything is `git2`. `docs/LICENSE_COMPLIANCE.md` does not change.
- **Not undoable.** These are network operations, as in 3b. None of the new commands get an arm
  in `undo::plan_for` (its `_ => return None` covers them). The tag badge's "Delete tag…" reuses
  the existing `DeleteTag` command, which is already undoable. "Checkout (detached)" reuses
  `CheckoutCommit`, which is also undoable.
- **Tags on rows: every tag under `refs/tags/`, peeled to a commit.** Lightweight and annotated
  tags (nested annotated tags too) use `Reference::peel_to_commit`. A tag that peels to a tree or
  blob is skipped. The names are sorted per commit. Tags are collected in `RefDecorations::load`,
  so file history and search results get them too. They render there without a menu.
- **The tag badge looks different from branch badges.** It has an outline instead of a fill and a
  leading tag icon (`lucide-react` `Tag`, already a dependency). Right-clicking it opens a tag
  menu:
  - "Push <tag> to <remote>", one item per remote. With no remote, a single disabled "Push tag"
    with the title "No remote configured."
  - "Checkout (detached)".
  - "Delete tag…": the same `ConfirmDialog` copy as `TagPanel`.

  The context menu event is stopped so the row's commit menu does not also open. Keyboard users
  have `TagPanel`, which already offers push and delete for every tag, plus the commit menu's own
  "Checkout (detached)". Badges stay non-focusable spans, so the listbox keeps one tab stop.
- **"Delete tag…" deletes the local tag only.** Deleting a tag on the remote is out of scope.
- **`push_branch` validates before connecting.** The local branch must exist (`find_branch`), and
  `refs/heads/<remote_branch>` must pass `Reference::is_valid_name`. Otherwise it returns
  `git2::Error` "invalid branch name", like `validate_tag_name`. The refspec is
  `refs/heads/<local>:refs/heads/<remote_branch>`. There is no force: a non-fast-forward is
  `NonFastForward`, as today.
- **`set_upstream` runs only after a successful push.** It writes `branch.<local>.remote` and
  `.merge = refs/heads/<remote_branch>` through a shared `set_branch_upstream` helper. That
  helper is `set_current_upstream`'s body, generalized to any local branch:
  `Branch::set_upstream`, with a raw config write when the tracking ref is missing. A failed push
  leaves the config alone.
- **Push honors `push.default` (user decision, after review).** Pushing to the upstream's
  branch name unconditionally would push a branch `feat` created from `origin/main` (so it tracks
  `origin/main`) straight onto `origin/main`. One git-core rule, `resolve_push_target` /
  `push_target(repo, local, remote) -> PushTarget::{Branch { remote, branch }, NeedsChoice
  { remote, local_branch }}`, decides where a current-branch push lands. `push.default` unset
  means `simple`. On the upstream's remote:
  - `upstream` / `tracking`: the upstream's branch name.
  - `current`: the local name.
  - `simple` and `matching`: the local name when it equals the upstream's branch name, else
    `NeedsChoice`. (`matching` pushes every same-named branch in git; the app only pushes one
    branch, so it applies the `simple` check.)
  - `nothing`: always `NeedsChoice`.

  To an explicitly chosen remote that is not the upstream's (or with no upstream), every mode but
  `nothing` pushes under the local name, and `nothing` asks. `push_current_branch` and
  `force_push_current_branch` refuse `NeedsChoice` with `RemoteError::PushTargetNeedsChoice`
  before connecting (the worker replies with that message and starts no transfer). Force push
  and its lease (`refs/remotes/<remote>/<target>`) use the same target, so a force push never
  goes to a differently named upstream unless `push.default=upstream`, and the target is
  validated like `push_branch`'s. `UpstreamInfo.push_branch` carries the resolved branch (null =
  ask) to the frontend. There, Push (sync bar, branch menu, remote folder, palette) opens "Push
  to…" prefilled with that remote and the local name instead of pushing when it is null. A
  missing `pushBranch` (old fixtures) reads as `simple`.
- **The force-push offer only follows a push of the current branch.** After a rejected
  `pushBranch` of any other local branch, the frontend offers no force push, because
  `forcePushCurrentBranch` would overwrite a different branch. The offer also needs the target
  (remote branch) to equal what `force_push_current_branch` resolves: `UpstreamInfo.pushBranch`
  when pushing to the upstream's remote (no offer when that is null), else the local name.
- **`delete_remote_branch` pushes `:refs/heads/<branch>` through `push_refs`.** It has
  negotiation-time cancel and the same rejection mapping. A refusal (protected branch, server
  hook) is `RejectedRemoteRef`. On success it deletes `refs/remotes/<remote>/<branch>` if it is
  still there. libgit2's push `update_tips` normally deletes it already, and the explicit delete
  makes this independent of that. Local branches whose upstream was the deleted branch keep their
  config, like git's "gone" upstream. Their `ahead`/`behind` become `None`. `branch` must be a
  valid name and not `HEAD`.
- **`prune: Option<bool>`, not `bool`.** `None` maps to `FetchPrune::Unspecified`, which lets
  libgit2 apply `remote.<name>.prune`, then `fetch.prune`, per remote. That is the roadmap's
  "default reads fetch.prune / remote.<name>.prune" with no code of ours. `Some(true)` maps to
  `On` and `Some(false)` maps to `Off`. Pull keeps `None`.
- **`RemoteInfo` gains `prune: bool`.** This is the effective config default:
  `remote.<name>.prune`, else `fetch.prune`, else `false`. The UI uses it only to show the Prune
  checkbox's state before the user has set it. It travels with the `listRemotes` call `refresh()`
  already makes, so there is no new read.
- **The Prune checkbox is a per-repo, per-machine UI preference in `localStorage`**
  (`fetch-prune:<repoPath>` = `"on"` / `"off"`, read and written in `try`/`catch`). This is the
  same mechanism as the branch tree's folder state (`lib/persistedOpenState.ts`), and it works in
  both frontends. When it is unset, every fetch passes `prune: null`, so git config decides per
  remote, and the checkbox shows the fetch target's `RemoteInfo.prune`. Once the user toggles it,
  every fetch in that repo passes the explicit value: sync bar Fetch, Fetch all, the remote
  folder's Fetch, and the palette's "Fetch <remote>". The alternative was the `config` crate, like
  the graph selection. It was rejected because this preference is not repository state, and the
  crate's TOML would need a new array-of-tables section for one boolean.
- **`fetch_all` fetches remotes in `repo.remotes()` order, one at a time, under one operation
  id.** It reports progress as `TransferOperation::FetchAll`. The fetch body is factored into a
  private `fetch_with(…, operation)` that `fetch_remote` (`Fetch`) and `fetch_all` (`FetchAll`)
  share. Each remote gets its own credential provider: the worker builds it from that remote's
  auth profile through a `credentials_for: &mut dyn FnMut(&str) -> Result<Box<dyn
  CredentialProvider + 'c>, RemoteError>` callback. If a remote fails (transport, credentials,
  missing auth profile), the error is recorded as `(name, transfer_error_kind())` and the next
  remote is tried. A cancel stops the loop and returns `Err(Cancelled)`. Remotes already fetched
  stay fetched. No remotes at all gives `Ok` with both lists empty.
- **`FetchAllOutcome { fetched: Vec<String>, failed: Vec<(String, TransferErrorKind)> }`.**
  `fetched` is an addition to the roadmap's shape. The worker needs it to tell "some failed" from
  "all failed".
- **Per-remote failures travel as their own event.** `TransferEvent::RemoteFailed { operation_id,
  remote_name, error }` is sent once per failed remote, before `Completed`. Its wire form is a
  `transfer-progress` event with `phase: "RemoteFailed"`, `errorKind` and a new optional
  `remoteName` field (omitted on every other event). The alternative was adding a field to
  `Completed`. That would have touched about 20 construction sites for one operation, and it
  would not survive a later live per-remote report.
- **The overall result of a Fetch all.** If at least one remote succeeded, or there are no remotes,
  `Completed` has `error: None`. If every remote failed, `Completed` has
  `error: Some(<first failure's kind>)`, so the phase is `Failed`. If cancelled, it has
  `error: Some(Cancelled)`. The frontend collects `RemoteFailed` names for the active operation.
  - Partial: it refreshes, then shows `Could not fetch from backup, mirror. The other remotes
    were fetched.`
  - Total: it shows `Could not fetch from backup, mirror.`
  - Cancelled: it shows `Fetch cancelled.`

  The error is set in the same `setState` that clears `transfer`/`pending`, after `refresh()`,
  because `refresh()` resets `error` to `null`. That also stops App's "Fetch complete" toast.
- **`BranchInfo` gains `upstream: Option<String>` ("origin/main") plus `ahead`/`behind:
  Option<usize>`.** `upstream` comes from `branch.<name>.remote` / `.merge` (a shared
  `configured_upstream` helper also used by `current_upstream`), even when the tracking ref is
  missing. The Push dialog needs it to tick "Set as upstream" only for branches without one.
  `ahead`/`behind` use the existing `ahead_behind`, so a missing tracking ref (never fetched, or
  deleted remotely) gives `None` and not zero. This costs one `graph_ahead_behind` per local
  branch with an upstream, per refresh. That is the same work `git branch -vv` does, and it is
  acceptable for the branch counts this UI targets.
- **The branch tree shows only the non-zero parts.** It shows `↑2`, `↓1`, both, or nothing when
  the branch is in sync or the counts are unknown. The span has
  `aria-label="2 ahead, 1 behind origin/main"` and the same `title`. The sync bar keeps showing
  both numbers for the current branch.
- **One Push dialog (`PushBranchDialog`) for every entry point.**
  - Entry points: the local branch menu "Push to…" (every local branch), the palette "Push branch
    to…" (current branch), and the sync bar's Push when the current branch has no upstream.
  - Fields: a "Remote" select, a "Remote branch" text field defaulting to the local name, and a
    "Set as upstream" checkbox, checked when `BranchInfo.upstream` is `None`.
  - Default remote: the branch's upstream remote, else `origin` if it exists, else the first
    remote.
  - Validation: a blank or invalid remote branch name (`branchNameProblem`) disables Push and shows
    the problem inline.
  - Submitting closes the dialog and starts the transfer, like the force-push confirm.
  - `aria-label`: `Push <local>`.
- **The sync bar's Push without an upstream opens that dialog.** It is enabled when HEAD is on a
  branch, the branch has no upstream, and at least one remote exists. The title is "Push this
  branch to a remote and set its upstream…". With an upstream, it pushes as today. A detached HEAD
  keeps the current disabled state.
- **Delete remote branch uses one dialog (`DeleteRemoteBranchDialog`) with two modes.**
  - From a remote branch's menu ("Delete <remote>/<branch>…", destructive), the remote and branch
    are preset, and the dialog is just the confirmation.
  - From the palette ("Delete remote branch…"), it opens with a "Remote" select and a "Branch"
    select that loads through `listRemoteBranches` when the remote changes.
  - Body: "Delete <remote>/<branch> on the remote? This can't be undone." The danger button is
    "Delete remote branch". `aria-label`: "Delete remote branch". ConfirmDialog's danger style is
    reused through its CSS module's `confirmButton` class.
- **The Fetch split button.** The sync bar has "Fetch" (unchanged target) plus a chevron button
  (`aria-label="More fetch actions"`, `aria-haspopup="menu"`). The chevron opens a `ContextMenu`
  under it with "Fetch all remotes" and "Prune". `ContextMenuItem` gains `checked?: boolean`.
  When it is set, the item is `role="menuitemcheckbox"` with `aria-checked`, and a check mark
  that is hidden when unchecked. Selecting "Prune" toggles the preference and closes the menu,
  like VS Code's menus. The chevron is disabled exactly when Fetch is.
- **The branch tree's remote-branch cache follows `refreshGeneration`.** `BranchTree` takes
  `refreshGeneration`. When it changes, open folders reload their list in place, with no
  "Loading…" flash, and closed folders forget theirs, so their next expand reloads. This fixes the
  stale tree after a sync bar fetch or a push, and it is how a deleted or newly published remote
  branch shows up. Re-expanding without a refresh in between still uses the cache. The
  folder-menu Fetch keeps its explicit reload, which is now redundant but harmless.
- **#97 is fixed here, in two places.** Deleting and renaming branches is this batch's territory,
  and both halves are small.
  - `graph_log` skips selected names whose `refs/heads/<name>` does not exist, the way
    `search_commits` does. A stale selection, including one left by a CLI delete, can no longer
    fail `refresh()`.
  - `useBranchActions.deleteBranch` / `renameBranch` update the persisted selection after the
    backend call succeeds. A deleted name is removed, and a renamed one is replaced in place. If
    removing the name would leave the selection empty, it becomes every remaining local branch,
    which means "show all", so isolating a branch and then deleting it does not leave an empty
    graph. The pure logic is in `lib/graphSelection.ts`.
- **Palette:**
  - "Fetch all remotes" (keywords fetch, all, remotes, prune) when a remote exists.
  - "Push branch to…" (push, publish, upstream, remote) when HEAD is on a branch and a remote
    exists.
  - "Delete remote branch…" (delete, remove, remote, branch) when a remote exists.

  All three are hidden while repository operations are disabled. The existing per-remote "Fetch
  <remote>" / "Push to <remote>" entries stay.
- **New response fields are optional in TypeScript** (`tags?`, `upstream?`, `ahead?`, `behind?`,
  `prune?`, `remoteName?`), following `UpstreamInfo.ahead?`/`behind?` from batch 3. The backend
  always sends them, and readers treat a missing value as empty, `null` or `false`. This keeps the
  roughly 100 existing `GraphCommit` / `BranchInfo` / `RemoteInfo` / `TransferProgress` test
  fixtures valid without edits. Only `RepoClient` *methods* force fixture updates.
- **Labels for the new transfer operations in `transferFailureMessage`.**
  - `DeleteRemoteBranch` + `RejectedRemoteRef`: "The remote refused to delete the branch."
  - `DeleteRemoteBranch` + `Cancelled`: "Delete cancelled."
  - `DeleteRemoteBranch`, anything else: "Deleting the remote branch failed".
  - `FetchAll` + `Cancelled`: "Fetch cancelled."
  - Credential kinds keep their shared messages.

## Backend

### `git-core/src/graph.rs`

```rust
pub struct GraphCommit {
    // … existing fields …
    pub remote_branch_refs: Vec<String>,
    /// Tag names (without `refs/tags/`) whose target peels to this commit, sorted.
    pub tags: Vec<String>,
}
```

`RefDecorations` gains `tags: HashMap<Oid, Vec<String>>`. `load` iterates
`repo.references_glob("refs/tags/*")`, peels each reference to a commit (skipping failures), and
sorts each vector. `graph_log`'s `Some(names)` arm becomes "push the ref if `find_reference`
succeeds, skip it if the error is `NotFound`, propagate any other error".

### `git-core/src/branch.rs`

```rust
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    /// `<remote>/<branch>` from `branch.<name>.remote`/`.merge`; `None` without upstream config.
    pub upstream: Option<String>,
    /// Commits on the branch not on its tracking ref; `None` without upstream or tracking ref.
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
}
```

### `git-core/src/remote.rs`

```rust
pub enum TransferOperation {
    Fetch, FetchAll, Pull, PushBranch, ForcePushBranch, PushTags, DeleteRemoteBranch, Clone,
}

pub struct RemoteInfo { pub name: String, pub fetch_url: String, pub push_url: Option<String>, pub prune: bool }

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchAllOutcome {
    pub fetched: Vec<String>,
    pub failed: Vec<(String, TransferErrorKind)>,
}

pub fn fetch_remote(repo, remote_name: &str, prune: Option<bool>, operation_id: String,
    credentials, reporter, cancel) -> Result<(), RemoteError>;

pub fn fetch_all<'c>(repo: &Repository, prune: Option<bool>, operation_id: String,
    credentials_for: &mut dyn FnMut(&str) -> Result<Box<dyn CredentialProvider + 'c>, RemoteError>,
    reporter: &mut dyn TransferReporter, cancel: &dyn Fn() -> bool,
) -> Result<FetchAllOutcome, RemoteError>;

pub fn push_branch(repo, remote_name: &str, local_branch: &str, remote_branch: &str,
    set_upstream: bool, credentials, reporter, cancel) -> Result<(), RemoteError>;

pub fn delete_remote_branch(repo, remote_name: &str, branch: &str,
    credentials, reporter, cancel) -> Result<(), RemoteError>;

pub(crate) fn configured_upstream(config: &git2::Config, local_branch: &str)
    -> Result<Option<(String, String)>, git2::Error>;   // (remote, remote_branch)
pub(crate) fn ahead_behind(…);                          // existing, made pub(crate)
pub fn set_branch_upstream(repo, local_branch: &str, remote_name: &str, remote_branch: &str)
    -> Result<(), RemoteError>;                         // set_current_upstream delegates to it
```

`push_current_branch` / `force_push_current_branch` keep their signatures. The first becomes
`push_branch(repo, remote, &current, &target, false, …)`, with `target` resolved as described in
Decisions.

## `repo-service`, Tauri, sidecar, `RepoClient`

- `TransferEvent::RemoteFailed { operation_id: String, remote_name: String, error:
  TransferErrorKind }`.
- `Command::FetchRemote` gains `prune: Option<bool>`. The new commands are `FetchAllRemotes {
  prune, operation_id, events, reply }`, `PushBranch { remote_name, local_branch, remote_branch,
  set_upstream, operation_id, events, reply }` and `DeleteRemoteBranch { remote_name,
  branch_name, operation_id, events, reply }`. Handlers are in `worker/remote.rs`, next to
  `fetch` / `push_branch`. The existing handler `remote::push_branch` (current branch) is renamed
  `push_current` so the new one can take the name.
- `WorkerHandle`:
  - `fetch_remote(remote_name, prune: Option<bool>, events)`
  - `fetch_all_remotes(prune, events)`, id `fetch-all-N`
  - `push_branch(remote_name, local_branch, remote_branch, set_upstream, events)`, id `push-N`
  - `delete_remote_branch(remote_name, branch_name, events)`, id `delete-remote-N`

  All of them return `Result<String, String>` (the operation id).
- Tauri commands (`commands/remote.rs`, registered in `main.rs`):
  - `fetch_remote(repo_path, remote_name, prune)`
  - `fetch_all_remotes(repo_path, prune)`
  - `push_branch(repo_path, remote_name, local_branch, remote_branch, set_upstream)`
  - `delete_remote_branch(repo_path, remote_name, branch_name)`

  DTO changes:
  - `TransferProgressDto.remote_name: Option<String>` (`skip_serializing_if = "Option::is_none"`)
  - `GraphCommitDto.tags`
  - `BranchInfoDto { upstream, ahead, behind }` (nulls serialized)
  - `RemoteInfoDto.prune`
- Sidecar: the same four methods with camelCase params, and the same DTO changes in its own
  copies.
- `RepoClient`:

```ts
export type TransferOperation =
  "Fetch" | "FetchAll" | "Pull" | "PushBranch" | "PushTags" | "ForcePushBranch" | "DeleteRemoteBranch" | "Clone";
export interface TransferProgress {
  // … existing …
  phase: "Starting" | "Receiving" | "Updating" | "RemoteFailed" | "Completed" | "Failed";
  /** Only on `RemoteFailed`: the remote that failed during a Fetch all. */
  remoteName?: string;
}
export interface BranchInfo { name: string; isCurrent: boolean; upstream?: string | null; ahead?: number | null; behind?: number | null; }
export interface RemoteInfo { /* … */ prune?: boolean; }
export interface GraphCommit { /* … */ tags?: string[]; }

fetchRemote(repoPath: string, remoteName: string, prune: boolean | null): Promise<string>;
fetchAllRemotes(repoPath: string, prune: boolean | null): Promise<string>;
pushBranch(repoPath: string, remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean): Promise<string>;
deleteRemoteBranch(repoPath: string, remoteName: string, branchName: string): Promise<string>;
```

## Frontend

- `lib/fetchPrune.ts`: `readFetchPrune(repoPath): boolean | null`,
  `writeFetchPrune(repoPath, prune)`, `effectiveFetchPrune(override, remotes, upstream): boolean`.
- `lib/graphSelection.ts`: `withoutBranch(selection, name, remaining): string[] | null` and
  `withRenamedBranch(selection, oldName, newName): string[] | null`. `null` means "unchanged, do
  not write".
- `AppState` gains:
  - `fetchPrune: boolean | null`
  - `pushDialog: { localBranch: string } | null`
  - `deleteRemoteBranchDialog: { remoteName: string | null; branchName: string | null } | null`
- `useRemoteTransferActions` gains:
  - `fetchAllRemotes()` and `setFetchPrune(prune)`
  - `pushBranch(remoteName, localBranch, remoteBranch, setUpstream)`
  - `openPushDialog(localBranch)` / `closePushDialog()`
  - `deleteRemoteBranch(remoteName, branchName)`
  - `openDeleteRemoteBranchDialog(remoteName, branchName)` / `closeDeleteRemoteBranchDialog()`
  - `RemoteFailed` collection, and the force-push offer guard

  `fetchRemote` passes the persisted prune value.
- `components/RefBadges.tsx` (new): branch, remote and tag badges for one commit. It is used by
  `CommitGraph` (with the remote filter and the tag menu), `FileHistoryList` and
  `CommitSearchResults`.
- `components/PushBranchDialog.tsx`, `components/DeleteRemoteBranchDialog.tsx` (new), rendered by
  `App` from the new state.
- `BranchTree`:
  - local menu "Push to…"
  - remote branch menu "Delete <remote>/<branch>…"
  - `↑n ↓m` on local rows
  - `refreshGeneration` cache reload
- `SyncBar`: the split Fetch button, `onFetchAll`, `prune`, `onTogglePrune`, and `onPublish` for
  Push without upstream.
- `ContextMenu`: `checked?: boolean`.
- Palette: the three commands in Decisions.

## Testing

- `git-core` `tests/graph.rs`:
  - a lightweight and an annotated tag on the same commit appear sorted on that row only
  - a tag on a tree is ignored
  - `graph_log` with a selection naming a deleted branch returns the other branches' history (#97)
- `git-core` `tests/branch.rs`:
  - `upstream`/`ahead`/`behind` are `None` without upstream
  - upstream configured but never fetched gives `upstream` set and counts `None`
  - with the tracking ref one behind and two ahead, it gives `Some(2)`/`Some(1)`
- `git-core` `tests/remote.rs`, against a bare remote:
  - delete remote branch removes both the remote ref and the tracking ref
  - push a non-current branch without upstream (remote ref created, no config)
  - push a non-current branch with upstream (config written, `BranchInfo.ahead == Some(0)`)
  - push under a different remote branch name
  - invalid remote branch name rejected before connecting
  - a push that fails leaves the upstream config unset
  - `push.default`: unset/`simple`/`matching` refuse a differently named upstream (`NeedsChoice`,
    remote untouched), `upstream` pushes to it, `current` pushes the local name, `nothing` asks
  - force push with a differently named upstream leases that branch
  - fetch all with one unreachable remote: one in `fetched`, one in `failed` with `TransferFailed`,
    and the good remote's tracking ref updated
  - fetch all with no remotes
  - fetch all cancelled before the first remote
  - `prune: Some(true)` removes a remote-deleted branch's tracking ref, and `Some(false)` keeps it
  - `None` with `fetch.prune=true` prunes
  - `RemoteInfo.prune` reads `remote.<name>.prune` over `fetch.prune`
- `repo-service` worker:
  - fetch all streams `Started{FetchAll}`, one `RemoteFailed` for the unreachable remote, and a
    successful `Completed`
  - all-unreachable gives a failed `Completed`
  - push branch with upstream round-trips and `list_branches` shows the upstream
  - delete remote branch round-trips
  - none of them records an undo entry
- `tauri-app`:
  - `TransferProgressDto` for `RemoteFailed` carries `remoteName` and omits it otherwise
  - `BranchInfoDto` serializes `upstream`/`ahead`/`behind`, nulls included
  - `GraphCommitDto.tags`
  - `RemoteInfoDto.prune`
- `vscode-sidecar`: `fetch_all_remotes`, `push_branch` and `delete_remote_branch` stream
  terminal `transferProgress` notifications against a bare remote.
- Frontend (mock `RepoClient`):
  - both clients' wiring
  - `lib/fetchPrune` and `lib/graphSelection` unit tests
  - state:
    - Fetch all partial and total failure messages
    - prune passed on every fetch path once set
    - no force-push offer after a non-current branch push is rejected
    - delete and rename prune the selection
  - components:
    - tag badges and the tag menu
    - ContextMenu `checked`
    - PushBranchDialog defaults and validation
    - DeleteRemoteBranchDialog in both modes
    - BranchTree `↑↓`, the new menu items and cache reload
    - SyncBar split, prune and publish
  - the three palette commands
- E2E Tauri (`e2e/specs/refs-and-remotes.spec.ts`):
  - publish a CLI-created branch to a spec-owned bare remote through "Push to…" with "Set as
    upstream" (asserts the bare ref and the branch config)
  - delete it through the remote branch menu (asserts the bare ref and the tracking ref are gone
    and the row disappears)
  - the spec owns its remote, branch and bare directory, removes them in `after()`, and depends on
    no other spec's state

## Docs

- `docs/USER_GUIDE.md`:
  - extend "Branches and remotes": ahead/behind, Push to…, publishing from the sync bar, deleting a
    remote branch, Fetch all and Prune
  - extend "Tags": graph badges and their menu
- `CHANGELOG.md` under Unreleased: "### Refs and remotes", including the #97 fix and the
  `push.default` push-target behaviour.
- `docs/ARCHITECTURE.md`: a short "Fetch all" note (one operation id, `RemoteFailed` events,
  prune `None` = git config).
- No `docs/LICENSE_COMPLIANCE.md` change.

## Out of scope

- Pushing to a different remote branch name than the one chosen in the dialog.
- Tag badges for remote-only tags.
- Deleting tags on a remote.
- Auto-fetch on a timer.
- Offering to delete the local branch after deleting its remote branch.
- A VSCode E2E flow.
