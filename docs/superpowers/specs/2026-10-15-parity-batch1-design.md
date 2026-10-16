# Feature parity, batch 1: amend, cherry-pick, revert, reset, force push, detached checkout

## Goal

Close the six biggest everyday-workflow gaps vs mature Git clients: amend the last commit, cherry-pick
a commit, revert a commit, reset the current branch to a commit (soft/mixed/hard), force push the
current branch with a lease, and check out a commit as a detached HEAD.

## Current state

- `git-core/commit.rs::commit` writes a commit on HEAD; it finishes an in-progress merge
  (`RepositoryState::Merge`, adds `MERGE_HEAD` parents, then `cleanup_state`). No amend.
- `git-core/merge.rs`: `start_merge` returns `MergeOutcome` (clean or conflicted paths),
  conflicts resolve through `conflict_hunks` / `resolve_conflict` /
  `resolve_add_delete_conflict` and `ConflictResolutionPane`; `abort_merge` is
  `reset --hard HEAD` + `cleanup_state` (works for any in-progress operation).
  `merge_message` reads `MERGE_MSG`.
- Cherry-pick exists only inside `rebase.rs::advance`.
- `remote.rs::push_current_branch` pushes `refs/heads/b:refs/heads/b`; a rejection classifies as
  `TransferErrorKind::NonFastForward`.
- `branch.rs::switch_branch` does safe `checkout_tree` then `set_head`.
- Frontend: `CommitGraph.tsx` single-commit context menu has "Branch from here" and "Rebase onto
  here". `OperationStatusStrip` knows only "Merging" (derived from `getMergeMessage() !== null`)
  and rebase progress. `CommitBox` commits only.

## Decisions

- **Native `git2` everywhere.** No `git` CLI dependency. Force-with-lease is emulated (below).
- **Conflicts reuse the merge flow.** Cherry-pick/revert leave conflicts in the index plus
  `MERGE_MSG`; the existing conflict pane resolves; Commit finishes; Abort restores.
- **Amend is a CommitBox toggle**, plus "Edit commit message…" on HEAD's context menu, which
  turns the toggle on.
- **Force push is offered on non-fast-forward rejection and via a palette command.** No
  permanent button.
- **Single commit only.** Multi-select keeps its existing menu (squash/fixup/drop/rebase).

## Backend (`git-core`)

### `commit.rs`

- `amend_head(repo: &Repository, message: &str) -> Result<String, CommitError>`
  - Writes the current index tree; keeps HEAD commit's parents and author; committer =
    `repo.signature()`; updates HEAD's branch ref (or detached HEAD) via
    `Commit::amend(Some("HEAD"), None, Some(&committer), None, Some(message), Some(&tree))`.
  - Errors: `CommitError::NothingToAmend` on unborn HEAD; `CommitError::OperationInProgress` when
    `repo.state() != Clean`.
- `commit()`: after writing, also call `cleanup_state` when state is `CherryPick` or `Revert`
  (not only `Merge`). Parents for those states are HEAD only (no extra parents).

### `pick.rs` (new)

- `pub enum PickOutcome { Committed { commit_id: String }, Conflicted { files: Vec<String> } }`
  (separate from `MergeOutcome`, whose `UpToDate`/`FastForwarded`/`Merged` don't apply).
- `cherry_pick(repo: &mut Repository, commit_id: &str) -> Result<PickOutcome, PickError>`
- `revert(repo: &mut Repository, commit_id: &str) -> Result<PickOutcome, PickError>`
- Shared preconditions, each a `PickError` variant:
  - `OperationInProgress` — `repo.state() != Clean`.
  - `DirtyIndex` — staged changes present (unstaged working-tree changes are allowed if they
    don't collide; libgit2's safe checkout refuses collisions → `Git` error surfaced as
    "would overwrite local changes").
  - `MergeCommitUnsupported` — commit has more than one parent.
  - `UnbornHead`.
- Flow: `repo.cherrypick(&commit, None)` / `repo.revert(&commit, None)`, then inspect the index:
  - No conflicts → commit with the original message (cherry-pick, original author preserved) or
    `Revert "<summary>"\n\nThis reverts commit <full sha>.` (revert, author = signature), then
    `cleanup_state`. Return `PickOutcome::Committed { commit_id }`.
  - Conflicts → leave state and `MERGE_MSG` (libgit2 writes it). Return
    `PickOutcome::Conflicted { files }`.
  - Empty result (resulting tree equals HEAD's tree, e.g. commit already applied) → no commit,
    `cleanup_state`, `PickError::EmptyResult`.

### `reset.rs` (new)

- `pub enum ResetMode { Soft, Mixed, Hard }`
- `reset_to(repo: &Repository, commit_id: &str, mode: ResetMode) -> Result<(), ResetError>`
  wraps `repo.reset(obj, kind, None)`. Refuses during an in-progress operation
  (`ResetError::OperationInProgress`).

### `branch.rs`

- `checkout_commit(repo: &Repository, commit_id: &str) -> Result<(), BranchError>`: safe
  `checkout_tree` then `set_head_detached` (same refuse-before-move ordering as
  `switch_branch`).

### `remote.rs`

- `force_push_current_branch(repo, remote_name, credentials, reporter, cancel)`:
  1. Expected = local `refs/remotes/<remote>/<branch>` OID (absent → zero OID, meaning "branch
     must not exist on remote").
  2. `push_refs` with `+refs/heads/<branch>:refs/heads/<branch>`,
     `TransferOperation::ForcePushBranch` (new variant, for progress labels), and a lease.
  3. Inside the existing `push_negotiation` callback (fires after the remote advertised its refs,
     before any pack data is sent), compare each `PushUpdate::src()` (remote's current OID) for
     `refs/heads/<branch>` with expected. Mismatch → abort the push there →
     `RemoteError::StaleLease`, classified as new `TransferErrorKind::StaleLease`. Remote
     untouched.
  - Same check-then-update window as `git push --force-with-lease` (between advertisement and
    the remote applying the update); documented in the user guide.
- Refuses on detached HEAD (existing `current_local_branch_name` error).

### Head / operation info

- `head_info(repo) -> HeadInfo` in `repo.rs`:
  - `head_id: Option<String>` — full SHA of HEAD's commit; `None` on unborn HEAD. (`GraphCommit`
    carries no HEAD marker; the frontend needs this for the HEAD-only menu item and amend
    prefill.)
  - `detached_at: Option<String>` — short SHA when HEAD is detached.
  - `head_pushed: bool` — current branch has an upstream whose tip equals or descends from HEAD.
  - `operation: Option<OperationKind>` with `OperationKind { Merge, CherryPick, Revert }` (rebase
    keeps its own `get_rebase_progress`).

## Plumbing

- `repo-service` worker ops + Tauri commands + `vscode-sidecar` methods for each new function,
  following the existing pattern. DTO serialization tests in `tauri-app` for `HeadInfo`,
  `ResetMode`, `PickOutcome`, the new `TransferErrorKind`/`TransferOperation` variants.
- `RepoClient` additions:

```ts
amendCommit(repoPath: string, message: string): Promise<void>;
cherryPick(repoPath: string, commitId: string): Promise<PickOutcome>;
revertCommit(repoPath: string, commitId: string): Promise<PickOutcome>;
resetToCommit(repoPath: string, commitId: string, mode: "soft" | "mixed" | "hard"): Promise<void>;
checkoutCommit(repoPath: string, commitId: string): Promise<void>;
forcePushCurrentBranch(repoPath: string, remoteName: string): Promise<string>; // operation id, like pushCurrentBranch
getHeadInfo(repoPath: string): Promise<HeadInfo>;
```

  Both `tauriRepoClient` and `vscodeRepoClient` implement them; mocks in tests updated.
- Amend prefill uses the existing `getCommitMessage(repoPath, headId)`.

## Frontend

- **Commit context menu** (single commit), after existing items:
  - "Cherry-pick" — disabled when the commit is HEAD, a merge commit, or an operation is in
    progress.
  - "Revert" — disabled for merge commits or during an operation.
  - "Checkout (detached)" — disabled during an operation.
  - "Reset ⟨branch⟩ to here…" — opens `ResetDialog` (Soft / Mixed / Hard radio, default Mixed,
    one-line explanation per mode). Hard uses the danger-confirm styling: "Discards all
    uncommitted changes. Cannot be undone." Disabled on detached HEAD or during an operation.
  - HEAD commit only: "Edit commit message…" — turns on CommitBox's amend toggle and focuses
    the message.
- **CommitBox**: "Amend last commit" checkbox.
  - On: prefill HEAD's message (remember any draft and restore it on toggle off); button label
    "Amend"; allowed with an empty index (message-only amend).
  - If HEAD is reachable from its upstream (already pushed), show a hint: "Already pushed —
    you'll need to force push."
  - Hidden on unborn HEAD and during an operation.
- **OperationStatusStrip**: driven by `getHeadInfo().operation` — labels "Merging",
  "Cherry-picking ⟨short sha⟩", "Reverting ⟨short sha⟩"; Abort button "Abort cherry-pick" /
  "Abort revert" calls existing `abortMerge`.
- **Commit after conflicts**: CommitBox prefills `MERGE_MSG` (already does for merge) for all
  three operations.
- **Force push**:
  - Push failure with `NonFastForward` → error offers "Force push (with lease)…" → confirm
    dialog ("Overwrites ⟨remote⟩/⟨branch⟩ with your local branch.") → `forcePushCurrentBranch`.
  - `StaleLease` error text: "⟨remote⟩/⟨branch⟩ changed since your last fetch. Fetch and review
    before force pushing."
  - Palette command "Force push current branch…" (same confirm).
- **Detached HEAD**: sidebar current-branch marker and SyncBar show "HEAD detached at
  ⟨short sha⟩"; Push/Pull disabled with tooltip.
- **Palette**: "Cherry-pick selected commit", "Revert selected commit", "Checkout selected
  commit", "Reset branch to selected commit…", "Amend last commit", "Force push current
  branch…".
- **Toasts** on success, via the existing `announce(label, action)` pattern (label-only, no
  SHA): "Cherry-picked", "Reverted", "Reset branch", "Checked out commit", "Amended commit",
  "Force pushed".
- Optimistic UI: none — these mutate history; refresh after completion.

## Errors

Backend error variants map to user-facing messages via the existing error-dialog path:

| Variant | Message |
|---|---|
| `OperationInProgress` | "Finish or abort the current merge/cherry-pick/revert/rebase first." |
| `DirtyIndex` | "Commit or unstage your staged changes first." |
| `MergeCommitUnsupported` | "Cherry-picking or reverting merge commits isn't supported yet." |
| `NothingToAmend` | "There is no commit to amend yet." |
| `EmptyResult` | "Nothing to apply — those changes are already on this branch." |
| `StaleLease` | see Force push above |

## Testing

- `git-core` (real temp repos, bare remote in a temp dir for push):
  - amend: tree + message change, parents/author kept, branch ref moved; message-only amend;
    refuses unborn HEAD and in-progress merge.
  - cherry-pick: clean (new commit, original author/message, state Clean); conflict (state
    `CherryPick`, conflicted paths, `MERGE_MSG` set); `commit()` then finishes and cleans state;
    `abort_merge` restores; refuses merge commit, staged changes, in-progress operation.
  - revert: same matrix; revert message format.
  - reset: soft keeps index+workdir, mixed keeps workdir, hard resets both.
  - checkout_commit: detached HEAD at commit; refuses when it would overwrite local changes,
    leaving HEAD unchanged.
  - force push: rewritten branch force-pushes when lease matches; refused with `StaleLease` when
    the bare remote moved after the last fetch; refused on detached HEAD.
  - head_info: detached/attached, each operation kind.
- `tauri-app`: DTO serialization for new types.
- Frontend (Vitest + mocked `RepoClient`): menu items and their disabled states; `ResetDialog`
  modes + hard confirm; amend toggle prefill/restore/label/pushed hint; force-push offer on
  `NonFastForward`, `StaleLease` message; status strip labels; detached-HEAD display; palette
  commands.
- E2E (Tauri): one flow — cherry-pick a commit from another branch, revert it, reset hard back.
  VSCode E2E: cherry-pick smoke test.

## Docs

- `docs/USER_GUIDE.md`: new section "Rewriting and moving commits" covering all six actions and
  the force-with-lease race caveat.
- `CHANGELOG.md` Unreleased: Added entries.

## Out of scope

Multi-commit cherry-pick/revert, mainline selection for merge commits, running git hooks,
amend with author edit, force push of non-current branches.
