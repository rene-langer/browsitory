# UX polish

## Goal

Close seven small follow-ups from earlier batches:

| Issue | Area | Problem |
|---|---|---|
| #106 | a11y | A remote branch row's "…" button is named `Actions for <branch>`, the same as a local branch of that name. |
| #141 | UI | The commit box's co-author popover always opens below its button and never flips or shifts at the viewport edges. |
| #146 | a11y | The commit menu opened from the palette should return focus to the commit list once a dialog it opened closes. |
| #154 | errors | Discarding or stashing a file whose folder became a file fails with a raw libgit2 message. |
| #157 | staging | A folder replaced by a link: staging `d/f` first fails, and unstaging `d/f` after the link was staged drops the link and leaves `d/g` as a staged deletion. |
| #177 | errors | The error banner should survive background refreshes and clear only on dismiss or on the next mutation, in Tauri and the VSCode webview alike. |
| #178 | hooks | `post-merge 0` after a clean merge's commit blocks the commit reply with no status line or Cancel. |

## Shipping

One PR on `feat/ux-polish`, with one CHANGELOG section (`### UX polish`) under `## [Unreleased]`,
above `### Hooks bundle`. The plan is `docs/superpowers/plans/2026-10-31-ux-polish.md`. It
changes `git-core` (`stage.rs`, `stash.rs`), `repo-service` (`worker/status.rs`,
`worker/hooks.rs`) and the frontend. There are no IPC/DTO changes. The VSCode E2E
`git-hooks.spec.ts` gains one assertion. With D5-A (the recommendation) there is **no new
dependency**.

## Current state (main at 56d8a069, checked 2026-10-31)

Every issue was checked against the code. Two were checked with probes: a scratch Cargo binary
against `git-core` and a git 2.53.0 shell script, both under `target/ux-probe/`. Vitest probes
used a throwaway config under `target/ux-probe/`, so no file under `frontend/` changed. All probes
were deleted afterwards.

### #106: valid

`BranchTree.tsx` l. 443: `aria-label={`Actions for ${branchName}`}` on remote rows. Local rows use
the same text (l. 359). The swatch on a remote row is already qualified, `Show <remote>/<branch>
in graph` (l. 423). The remote **folder** header is `Actions for remote <name>` (l. 966). Tests
that use the ambiguous name: `BranchTree.test.tsx` l. 578 and l. 1036 (both scoped with `within`
as a workaround) and `App.test.tsx` l. 1015 (`Actions for topic`). No E2E uses it.

### #141: valid, and worse than the issue says

`CoAuthorPicker.tsx` renders `.popover` (`CommitBox.module.css` l. 109) as `position: absolute;
top: calc(100% + space-1); left: 0; width: min(22rem, 80vw)`. The commit box sits in
`DiffPane`'s `.commitDock`, which is `position: sticky; bottom: …` at the **bottom** of the
scrolling pane. So the popover opens downward from near the bottom of the window in most layouts,
not only in small windows, and the scroller clips it. Nothing measures it. `ContextMenu` already
hand-rolls a clamp (`ContextMenu.tsx` l. 50–62, `useLayoutEffect` + `getBoundingClientRect`) and
has a jsdom test that mocks `getBoundingClientRect` (`ContextMenu.test.tsx` l. 92). The project
has no positioning dependency.

### #146: already behaves as requested

The issue asks that a commit menu opened from the palette ("Reset to …", `goToCommitMenu` in
`lib/commands.ts` l. 117) return focus to the commit list after a dialog it opened closes. Two
Vitest probes showed that it already does:

- **Component level:** focus starts on an outside button, the menu is opened by a `contextmenu`
  event, then "Reset main to here…" and **Cancel** in `ResetDialog`. Focus ends on `ul[role=listbox]`
  "Commit history".
- **App level:** focus starts on the repo tab, then Ctrl+K "Reset to c1…", "Reset main to here…"
  and Cancel. Focus ends on the commit list.

Why it works:

1. `ContextMenu.activate` (l. 145) runs `restoreFocus()`, then `item.onSelect()`, then
   `onClose()`.
2. `CommitGraph`'s `onClose` (l. 485) focuses `listRef` synchronously, *before* the dialog's
   first effect runs.
3. `useRestoreFocus` therefore records the list.

`listRef.current?.focus()` in `onClose` predates #144 (it came with #72/#113), so either the
issue described an intermediate state of the #144 branch, or a path the probes didn't take.
Focus does pass through the pre-palette element for one synchronous step (`restoreFocus`), which
is harmless but not intended. The fix is a regression test plus `returnFocusTo={listRef.current}`
on both `CommitGraph` menus, so that focus never hops (see D10).

### #154: partly valid; the literal case is the folder-to-file one

The probes covered both directions:

| Setup | Action | Browsitory today | git 2.53 |
|---|---|---|---|
| **B:** HEAD has `d/f`; the worktree folder `d` (with untracked `d/u`) was replaced by a regular file `d` | Discard `d/f` | `failed to update the index: failed to make directory '…/d': directory exists; class=Filesystem (30)` (wrong and raw) | `git restore d/f` **deletes the untracked file `d`** and restores `d/f` |
| B | Discard all `[d/f, d]` (status order is `d`, `d/f`, but the call takes the confirmed list) | `Couldn't discard 1 file: d/f.` (no reason) | — |
| B | Stash file… `d/f` | `git operation failed: failed to make directory '…/d': directory exists` | `git stash push -- d/f` saves the stash, then fails "could not write d/f: Not a directory" |
| B | Stash all (with or without untracked) | Ok | Ok (without `-u`, git also deletes the untracked `d`; see finding 1) |
| **A:** HEAD has a file `d`; the worktree has a folder `d/` with untracked `d/u` | Discard `d` | Ok, and **`d/u` is deleted** (libgit2 forced checkout) | Same (`git restore d` deletes `d/u`) |
| A | Stash file… `d` | `Can't stash 'd': it was a file and is now a directory. Commit or move its contents first.` (clear, since #115) | Fails after saving |
| A | Stash all without untracked | Ok, and `d/u` is deleted | Same |

So in the issue's wording, "a folder containing an untracked file … replaced by a regular file"
is case B. Its discard and stash errors are raw, and its Discard all hides the reason. Case A
has no error at all, but a silent deletion that git also does. Undo covers discards
(`USER_GUIDE.md` "Undo and redo"), and the undo snapshot (`undo::build_worktree_tree`) includes
untracked files, so `d/u` should be recoverable. That is unproven for a D/F pair, so the plan
proves it (D9).

### #157: valid; git 2.53 behaves the same

A git-core probe (HEAD `d/f`, `d/g`; worktree `d` → link):

- `stage_file("d/f")` before `d`: `'d/f' is beyond a symbolic link.` The index is unchanged.
  `git add d/f` fails the same way. Order `d`, `d/f`, `d/g` works: after the link is staged,
  `d/…` is gone from the index and `stage_file` returns `Ok` (`stage.rs` l. 70–80).
- With the link staged (index `[d:120000]`), `unstage_file("d/f")` leaves index `[d/f]`. Status
  is then `d` New (unstaged), `d/f` Deleted (unstaged) and **`d/g` Deleted (staged)**. `git
  restore --staged d/f` and `git reset -- d/f` give the identical index: the D/F replacement in
  the index is what both do.
- `unstage_file("d")` already restores the whole pair (index `[d/f, d/g]`): `reset_default`'s
  pathspec `d` matches `d/…` in HEAD.

The test `staging_a_file_below_a_symlinked_folder_is_refused_like_git`
(`tests/stage_commit.rs` l. 529) pins today's refusal. `symlink_write_through.rs` l. 1404–1457
covers Stage all and staging `d` itself.

### #177: the policy is valid, but the issue's premise is wrong

The issue says "the file watcher fires a refresh". **Neither frontend has a watcher, a poll or a
focus refresh.** The checks:

- `grep` finds no `FileSystemWatcher`, `notify`, `setInterval` or `visibilitychange` in
  `extension/src`, `crates/vscode-sidecar`, `crates/repo-service` or `frontend/src`. The one
  exception is `UpdateBanner`'s update check.
- App's window `focus` handler calls only `refreshUndoState` (`App.tsx` l. 304–309), which never
  touches `error`.
- A full `refresh()` (`useAppState.ts` l. 382) runs only:
  - on `RepoWorkspace` mount (`App.tsx` l. 257);
  - from the palette's **Refresh**;
  - when an external tool exits (`useExternalTools`);
  - at the end of every successful mutation (`useMutationRunner`, all six variants), a
    transfer's `Completed`, and a pull.

`refresh()` sets `error: null` (l. 446). Mutations don't clear `error` when they **start**;
transfers do (`useRemoteTransferActions.ts` l. 337, l. 513). Instead the trailing `refresh()`
clears it once the mutation **succeeds**. Some App call sites (`announce`, `runPickAction`, the
hook retry) call `dismissError()` first by hand. Two reads also clear `error` on success:
selecting a reflog ref (`useReflogActions.ts` l. 40) and listing pull requests
(`useForgeActions.ts` l. 33).

So the banner that vanished in the VSCode Cancel E2E had another cause. The likeliest candidate
is an overlapping mutation's trailing refresh (for example **Stage all**'s refresh landing after
the commit started), but that is unproven. The plan **reproduces it first** (Task D0) with the
banner assertion the issue asks for, and only then changes the policy. The policy change is
still worth making, because the palette's Refresh and an external tool's exit clear errors
today.

### #178: valid

`worker/status.rs` `commit` (l. 298–338):

- `commit_with` runs with cancel + `ChannelHookReporter`;
- then `op.clear()`;
- then `run_post_hook(repo, "post-merge", &["0"])` for a clean merge (`run_hook` with `&|| false`,
  no reporter, `HOOK_TIMEOUT` 600 s);
- then `reply.send`.

The commit box spins with no status line until the hook exits. **`post-commit` already does what
this issue wants:** `commit::run_post_commit` (`commit.rs` l. 340) goes through
`hooks::run_reported` with the commit's `HookOptions`. So it shows "Running post-commit hook…"
with Cancel, and its failure or cancel is ignored. The frontend's `HookStatus` is generic in the
hook name (`hookFailure.ts` l. 41).

Two related cases still block their replies with no status (`run_post_hook`):

- `post-merge` after a fast-forward or squash (`worker/merge.rs` l. 57–58) and after a fast-forward
  pull (`worker/remote.rs` l. 337);
- `post-checkout` after a switch or checkout.

Their commands have no operation id or hook events (see D2).

Replying first does not unblock the UI on its own. The worker is serial, and
`runMutationWithOutcome` awaits `refresh()` before it clears `pending`. That refresh's 14 reads
queue behind the hook, so the commit box would stop spinning but every control would stay
disabled, still with no explanation.

## Design

### 1. #106: qualify the remote row's actions button

`BranchTree.tsx` l. 443 becomes `aria-label={`Actions for ${remoteName}/${branchName}`}`. It
matches the swatch and the menu items ("Delete origin/topic…"). Local rows keep `Actions for
<branch>`. The three tests switch to the qualified name and drop the `within(...)` workarounds. A
new test renders a local `feat/foo` and a remote `origin/feat/foo` and asserts that the two names
differ and that each is unique (`getByRole` does not throw).

### 2. #141: flip and shift the co-author popover (D5-A: hand-rolled)

- A new pure helper, `frontend/src/lib/anchoredPosition.ts`:

  ```ts
  export interface Placement { top: number; left: number; maxHeight: number; side: "below" | "above" }
  export function placeAnchored(
    anchor: { top: number; bottom: number; left: number },
    size: { width: number; height: number },
    viewport: { width: number; height: number },
    { gap = 4, margin = 4 } = {},
  ): Placement
  ```

  - **Placement:** below when `height + gap` fits below the anchor. Otherwise above when it fits
    there. Otherwise on the side with more room.
  - **Height:** `maxHeight` is the room on the chosen side minus `gap` and `margin`.
  - **Left edge:** `left` = `anchor.left`, clamped to `[margin, viewport.width - width - margin]`
    (and `margin` when the popover is wider than the viewport).
  - **Top:** `top` is `anchor.bottom + gap` below, or `anchor.top - gap - min(height, maxHeight)`
    above.
- `CoAuthorPicker`:
  - The popover becomes `position: fixed`, which escapes the scroller's clipping. Its
    `top`/`left`/`maxHeight` are set inline, and `data-side` is set for tests and CSS.
  - A `useLayoutEffect` measures the button (`getBoundingClientRect`) and the popover
    (`offsetWidth`/`scrollHeight`) and calls `placeAnchored`. It reruns when the popover opens,
    when `authors`/`options.length`/`loadError` change (the list loads asynchronously and changes
    height), on window `resize`, and on `scroll` (capture phase: the diff pane scrolls).
  - The CSS drops `top`/`left`. `.popover` gets `overflow: hidden`. `.options` gets `min-height:
    0; flex: 1 1 auto`, so the list shrinks before the filter field does. The `width: min(22rem,
    80vw)` stays.
- No other popover changes. `ContextMenu` keeps its own clamp. A shared primitive is out of scope:
  only one consumer needs flipping.

**Fixed positioning caveat:** an ancestor with `transform`, `filter` or `contain: paint` would
make `fixed` relative to that ancestor. None exists today (checked `DiffPane.module.css`,
`App.module.css`). The plan's manual smoke test checks this in Tauri and VSCode.

### 3. #146: lock in the focus return (D10-A)

- A regression test in `CommitGraph.test.tsx`:
  1. focus an outside button;
  2. open the menu by `contextmenu`;
  3. choose "Reset main to here…", then **Cancel**;
  4. the commit list has focus.

  The same test covers "Cherry-pick…" on a merge (the parent picker).
- An App-level test in `App.test.tsx`:
  1. focus the repo tab;
  2. open the palette and choose "Reset to c1…";
  3. choose the menu item, then Cancel;
  4. the commit list has focus.

  It needs `Element.prototype.scrollIntoView` stubbed (jsdom lacks it; `goToCommitMenu` calls it).
- Hardening: both `ContextMenu`s in `CommitGraph` pass `returnFocusTo={listRef.current}`, so
  `restoreFocus` goes straight to the list, never through the pre-palette element. `onClose` keeps
  focusing the list (outside clicks don't call `restoreFocus`).

### 4. #154: say what's in the way (D8-A, D9-A)

**New errors** (exact copy, defined once):

- `StageError::FolderIsNowAFile { path, parent }`:
  `Couldn't discard '{path}': '{parent}' is now a file, not a folder. Discard or move '{parent}' first.`
- `StashError::FolderIsNowAFile { path, parent }`:
  `Can't stash '{path}': '{parent}' is now a file, not a folder. Move or delete '{parent}' first.`

**Helper:** `worktree_guard::file_in_place_of_folder(workdir, rel) -> io::Result<Option<String>>`
returns the first proper ancestor of `rel` that exists as a **regular file**. Links are left to the
existing `BeyondSymlink` checks. `NotFound` → `None`.

**Discard** (`stage::discard_file`): for a path in the index, run the check before
`prepare_forced_checkout`, and return `FolderIsNowAFile` when it finds one. A path that is not in
the index is unchanged: a stale `d/u` below a file `d` has nothing to delete, and `Ok` is right.

**Discard all** (`discard_all_unstaged`): process the confirmed paths that are **not in the
index** (untracked deletions) first, then the rest. The order within each group is kept. Then
`[d/f, d]` succeeds as a whole: the untracked file `d` goes first, then `d/f` is restored. A lone
`d/f` (with `d` not confirmed) fails with the new message. `DiscardIncomplete` names only one
failed path, so it then appends that path's reason: `Couldn't discard 1 file: d/f. 'd' is now a
file, not a folder.` With several failed paths it keeps today's text.

**Stash file…** (`save_path_limited_stash`): next to the existing `DirectoryReplacedNonDirectory`
loop (l. 259–285), add the mirror check for each tracked path. It runs **before anything is
recorded**, so the stash list doesn't change, as the existing check guarantees.

**Case A** (a file `d` replaced by a folder holding untracked files, then Discard `d`): no
behaviour change (git parity, D9-A). The plan adds a worker test proving that **Undo** brings
`d/u` back. If it can't, D9-B (refuse) becomes the fallback, and the plan says so.

### 5. #157: links over folders, staging and unstaging (D3-A, D4-A)

**Staging `d/f` below a link `d`** (`stage::stage_file`, l. 74–81): when `d/f` has a stage-0 index
entry (and no conflict entries), **remove it from the index**, i.e. stage its deletion. That is
what status shows ("Deleted"), and nothing is read through the link. Conflict entries keep the
`BeyondSymlink` refusal. Results:

- Stage all works in any order.
- A single click on `d/f` stages just that deletion.
- The link and `d/g` stay as they were.
- Staging the link `d` later still replaces the folder's remaining entries.

The doc comment changes accordingly. `stage_hunk`/`stage_lines` below a link still refuse
(partial content can't be staged from a link).

**Unstaging inside a folder/link pair** (`stage::unstage_file`): before `reset_default`, find the
**pair root** of `path`:

- a proper ancestor `a` of `path` that has a stage-0 **non-tree** entry in the index (a link or
  file that replaced the folder), **or**
- a proper ancestor `a` that is a non-tree entry in **HEAD** (the reverse: a folder replaced a
  committed link or file, and `a/…` is staged).

When there is one, reset pathspec `a` instead of `path`. `reset_default` with `a` restores HEAD's
entries for `a` and everything below it, and drops index-only entries below `a`. The probe showed
this for `unstage_file("d")`. The staged rename extension (#137) is unchanged. Unstage all is
unaffected: the first path of the pair restores it, and the rest are no-ops.

The optimistic update (`withFilesStaged`) predicts only the clicked row. The trailing refresh shows
the real rows. That is acceptable for a rare case, and it is the same as how a staged rename
already behaves.

### 6. #177: errors survive refreshes (D6-A, D7-A)

The policy (from the issue): a background refresh never clears `state.error`. Dismissing it does,
and so does **starting** a mutation.

- `useMutationRunner`: all six variants set `error: null` in the same `setState` that sets
  `pending: true`.
  - The optimistic variants take their snapshot inside that updater, before the clear. A failure
    then restores the snapshot and overrides `error` with the new message, as now.
  - Success leaves `error` at `null`, so the trailing refresh no longer needs to clear it.
- `refresh()` no longer sets `error: null`. On success it clears **only an error that a failed
  refresh produced** (D7-A):
  - a `lastRefreshError` ref holds the message `refresh()`'s catch set;
  - on success, `error` becomes `null` iff `prev.error === lastRefreshError.current`;
  - then the ref is reset.

  If a mutation failed with the very same message (for example "The Browsitory worker stopped…"),
  clearing it once a refresh succeeds is correct: the cause has healed.
- Reads (`selectReflogReference`, `listPullRequests`) apply the same rule: a successful read clears
  only the error that the same read set (same ref technique, one ref per hook). Other reads don't
  touch `error`.
- Commit, amend, Continue, undo/redo and the hook retry go through the runner, so they are
  covered.
- Transfers already clear on start. The `Completed` comment in `useRemoteTransferActions.ts` l. 201
  ("`refresh()` (which resets `error`)") is corrected.
- `dismissError` is unchanged (it also clears `forcePushOffer`, `forcePushConfirm` and
  `hookFailure`). Starting a mutation clears **only** `error`: `hookFailure` keeps its own
  lifecycle, and the "Show hook output…" link stays until a commit lands or it is dismissed.
- App's explicit `dismissError()` calls (`announce`, `runPickAction`, the retry) stay. They are now
  redundant for `error` but still clear the offers, and the toast logic ("pending fell back to
  false with no error") keeps working.
- Both frontends share this code, which covers "same in Tauri and VSCode". The VSCode E2E Cancel
  test asserts the banner, as the issue asks.

### 7. #178: report post-merge like post-commit (D1-A, D2-A)

In `worker/status.rs` `commit`, a clean merge's `post-merge 0` runs **before** `op.clear()`,
through a new `worker::hooks::run_post_hook_reported(repo, name, args, options)`. It wraps
`git_core::hooks::run_reported` with `HookOptions { cancel: Some(&cancel), reporter: Some(&mut
reporter), ..Default::default() }`. The commit's operation id therefore carries
`Started`/`Finished { hook: "post-merge" }`, the commit box shows "Running post-merge hook…" with
**Cancel**, and `cancel_transfer(commit-<n>)` kills it.

- The outcome stays `Committed` whatever happens to the hook (failure, timeout, cancel). Post-hook
  failures are ignored, as in git and as for `post-commit`. `ChannelHookReporter::finished` already
  logs them through `log_hook_result`.
- `no_verify` does not skip it (`git merge --no-verify` still runs `post-merge`, per the hooks
  bundle probe). `run_reported` doesn't consult `no_verify`; a test pins this.
- The reply is still sent after the hook (D1-A). With the status line and Cancel the wait is
  visible and can be stopped, as with `post-commit`.
- No frontend change: `HookStatus` is generic. A Vitest test pins the label for `post-merge`.
- Fast-forward, squash, pull and `post-checkout` keep `run_post_hook` (D2-A, follow-up).

## Open decisions (provisional choice in **bold**; the plan implements it)

**D1. #178: when the commit replies, and whether post-merge is cancellable. User-facing.**

- **A. Report `post-merge` as a running hook on the commit's operation, cancellable. The reply
  comes after it.** This matches `post-commit` today. Cancel kills the hook, and the commit stays.
- B. Report it as running but **non-cancellable** (the issue's second option). The status line
  shows without Cancel (`HookStatus` needs a `cancellable` flag through `HookProgress`, which is an
  IPC change). The user still waits up to 10 minutes.
- C. Reply first, then run `post-merge` in the worker. The commit box clears at once. But the
  trailing `refresh()` queues behind the hook on the serial worker, so `pending` stays true and
  every control stays disabled, with nothing on screen saying why. That is worse than A unless it
  is combined with A's status line. A detached-thread variant (the hook running concurrently with
  later worker commands, with no owner for its timeout or cancel) is not recommended.

Recommendation **A**: smallest change, identical to `post-commit`, and git's Ctrl+C would do the
same.

**D2. #178: scope. Engineering/UX.**

- **A. Only the commit path (the issue).** The other post hooks keep blocking their replies
  without status. File a follow-up issue for fast-forward/squash/pull `post-merge` and
  `post-checkout`.
- B. All post hooks now. `StartMerge`, `SwitchBranch`, `CheckoutCommit` and the pull need
  operation ids, hook events and Cancel, plus DTO, Tauri, sidecar and frontend plumbing (about the
  size of the hooks bundle's Continue work).

**D3. #157: what staging `d/f` below a link `d` does. User-facing.**

- **A. Stage `d/f`'s deletion only** (remove it from the index). It is precise and reads nothing
  through the link, but goes beyond git (git refuses).
- B. Stage the link `d` instead (as the issue suggests). That stages the whole swap, including
  every other `d/…` deletion: more than the user clicked.
- C. Keep refusing a single click (with a clearer message, "Stage 'd' to stage the link"), and
  skip such paths in a Stage all batch.

**D4. #157: what unstaging `d/f` does when the link `d` is staged. User-facing.**

- **A. Restore HEAD's entries for the whole pair** (the link is unstaged and `d/f`, `d/g` are back),
  as the issue proposes. Unstaging part of a D/F pair is impossible in git's index, so the pair is
  the unit.
- B. Keep today's (and git's) result, index `[d/f]` with `d/g` a staged deletion, and explain it
  in a toast.
- C. Refuse with "Unstage 'd' to unstage the files it replaced."

**D5. #141: positioning library or hand-rolled. Engineering.**

- **A. Hand-roll** `lib/anchoredPosition.ts` (about 40 lines plus a pure unit test).
  `ContextMenu` already hand-rolls its clamp, there is one consumer, and there is no dependency.
- B. `@floating-ui/react-dom` 2.1.9 (MIT), which pulls `@floating-ui/dom` 1.8.0, `@floating-ui/core`
  1.8.0 and `@floating-ui/utils` 0.2.12, all MIT (checked with `npm info … license`). That needs
  four `LICENSE_COMPLIANCE.md` rows. It handles more cases (auto-update, nested scroll parents,
  `transform` ancestors) and makes `ContextMenu`'s clamp easy to port later.

**D6. #177: what counts as a "mutation" that clears the error. User-facing.**

- **A. Every write.** That is everything through the mutation runner (all staging/discard,
  commit/amend, branch/tag/stash/worktree/remote edits, merge/rebase/pick/reset, undo/redo, hook
  retries) plus transfers and pulls (already). Reads never clear another action's error: the
  palette's Refresh, diffs, load more, reflog and the PR list. A read that fails and then succeeds
  clears only its own error.
- B. Every user-initiated action, reads included (selecting a commit, opening a diff, the
  palette's Refresh). Errors vanish as soon as the user does anything.
- C. Writes, but only on **success** (today's effective behaviour through the trailing refresh).
  The old error stays visible while the next action runs and is replaced if that action fails.

**D7. #177: a failed refresh's own error. Engineering/UX.**

- **A. A successful refresh clears an error that a failed refresh set** (same message). So "the
  worker stopped" goes away once the worker is back.
- B. A refresh never clears any error, its own included. The user dismisses it.

**D8. #154 (case B): discarding `d/f` when its folder `d` is now a file. User-facing.**

- **A. Refuse with the new message naming `d`.** Discard all handles untracked deletions first, so
  confirming both `d` and `d/f` succeeds.
- B. Do what `git restore d/f` does: delete the untracked file `d` and restore `d/f`. The
  confirmation dialog would have to name `d` (frontend change). Silent deletion of a file the user
  didn't select is why this is not recommended.

**D9. #154 (case A): discarding a file `d` that is now a folder holding untracked files.
User-facing.**

- **A. Keep git's behaviour** (the untracked files below are deleted). A test proves that Undo
  restores them. If Undo can't, fall back to B within this batch.
- B. Refuse, like **Stash file…** already does: "Couldn't discard 'd': it is now a folder holding
  untracked files (d/u). Move them first."

**D10. #146: close as already fixed. Process.**

- **A. Add the regression tests and the `returnFocusTo` hardening, and close it with this PR.**
- B. Close it as "cannot reproduce" with no code.

## Testing

Real temp-dir repos (`mod common;` / `test_support::hermetic_git!()` in git-core, and the worker
test module's `init_repo`/`install_hook`/`log_hook` in repo-service). The frontend uses Vitest
with a mocked `RepoClient`. Links and hooks are `#[cfg(unix)]`.

| Issue | Layer | Tests |
|---|---|---|
| #106 | Vitest | `BranchTree.test.tsx`: local vs. remote names are distinct and unique; the three call sites updated. |
| #141 | Vitest | `lib/anchoredPosition.test.ts` (pure: below, flip above, larger side with `maxHeight`, right-edge shift, left clamp, wider than the viewport). New `CoAuthorPicker.test.tsx` (mocked `getBoundingClientRect`/`innerHeight`: near the bottom → `data-side="above"` and `top` above the button; near the right edge → shifted `left`; a resize re-places it). Manual smoke in Tauri and VSCode. |
| #146 | Vitest | `CommitGraph.test.tsx` (reset dialog, parent picker); `App.test.tsx` (palette → menu → dialog → Cancel → list focused). |
| #154 | git-core | `tests/discard_ignore.rs`: B discard → new error, worktree unchanged; Discard all `[d/f, d]` succeeds; Discard all `[d/f]` names the reason. `tests/stash.rs`: B Stash file… → new error, stash list and worktree unchanged. |
| #154 | repo-service | Worker: A discard `d` → Undo restores `d/u` (D9-A proof). |
| #157 | git-core | `tests/symlink_write_through.rs`: staging `d/f` stages its deletion (outside untouched); Stage all in reverse order; unstaging `d/f` with the link staged restores the pair; the reverse (a folder over a committed link). The flipped `stage_commit.rs` l. 529 test. |
| #177 | Vitest | `useMutationRunner.test.ts` (each variant clears on start; an optimistic failure shows the new error). `useAppState.test.ts` (a refresh keeps a mutation's error; a refresh clears its own error; reflog/PR reads). `App.test.tsx` (the banner survives the palette's Refresh; Stage clears it). |
| #177 | E2E (VSCode) | `extension/e2e/src/specs/git-hooks.spec.ts` Cancel: the banner reads "The pre-commit hook was cancelled." and is still there 3 s later. This is the issue's acceptance criterion, and a black-box flow is warranted because the bug was only seen there. |
| #178 | repo-service | Worker: post-merge reported with the commit's id; `cancel_transfer` stops it and the commit stays; `no_verify` still runs it. Vitest: `CommitBox`/`HookStatus` label for `post-merge`. |

No Tauri E2E: none of these needs a new black-box flow. The existing suites must stay green.

## Docs

- `docs/USER_GUIDE.md`:
  - "Hooks" (l. ~229): the status line also shows for `post-merge` after a merge commit, with
    Cancel (the commit stays).
  - "Stashes" (l. 448–449): staging a file below a link that replaced a folder stages its deletion;
    unstaging one file of such a pair unstages the whole pair; discarding still refuses.
  - "Discarding and ignoring files" (l. 139–157): a file whose folder became a file names it;
    Discard all with both succeeds.
  - "Diagnosing a failure" (l. 788) or "Overview": errors stay until you dismiss them or start
    another action; refreshing doesn't clear them.
- `docs/ARCHITECTURE.md`:
  - "Git hooks" (l. 440–502): `post-merge` after a merge commit is reported and cancellable like
    `post-commit`; the other post hooks are not.
  - "Error handling" (l. 710): the frontend error lifecycle (cleared by dismiss or a mutation's
    start, never by a refresh, except a refresh's own error).
  - "Worktree writes and symlinks" (l. 503): stage/unstage of a D/F pair; `FolderIsNowAFile`.
- `docs/LICENSE_COMPLIANCE.md`: only with D5-B.

## Risks

- **#177 cause unknown.** If D0 shows that the VSCode banner vanished for a reason other than a
  refresh (for example a remount), the policy change alone won't satisfy the E2E. D0 comes first
  for that reason.
- **#177 test churn.** About ten `useAppState.test.ts` assertions expect `error` to be `null` after
  a refresh-driven success. Each must be checked: some will now pass because of the runner's
  clear-on-start, and some will need rewording.
- **#177 stale errors.** Errors now stay visible longer. A banner from a failed action on another
  file stays until dismissed or the next write. That is intended, but it is a visible change.
- **#141 fixed positioning.** It depends on no transformed ancestor. A future `transform` on the
  pane would mis-place the popover; a code comment points to D5-B.
- **#157 beyond git.** Staging `d/f`'s deletion and pair-wise unstaging differ from git's
  refusal/replacement. Someone comparing with `git status` after a partial stage sees a valid
  index that git itself wouldn't produce from the CLI.
- **#178 the wait is unchanged.** D1-A makes the wait visible and cancellable, not shorter. Fast-
  forward, squash and pull still block silently (D2-A).
- **#154 D9-A depends on Undo restoring an untracked file below a D/F swap.** The undo snapshot
  overlays HEAD's file `d` with the untracked `d/u`. If the in-memory index rejects that D/F pair,
  the test fails and D9-B applies.

## Unrelated findings (not fixed here)

1. **Stash all without "Include untracked files" deletes untracked files in the way.** In case A
   (and B's untracked file `d`), libgit2's `stash_save` resets and deletes them, as `git stash`
   without `-u` does. The default includes untracked files, so the default path is safe. Follow-up
   candidate: refuse, as **Stash file…** does.
2. **A path-limited stash of an untracked file `d` restored HEAD's `d/f`** (probe B, Stash file…
   `d`). The revert's pathspec probably matches `d/…` by prefix. Whether the stash records `d/f`'s
   deletion was not checked. Follow-up: verify and pin `disable_pathspec_match`.
3. **`post-checkout` and fast-forward/squash/pull `post-merge` block silently** (D2).
4. **The hooks bundle's VSCode E2E comment** (`git-hooks.spec.ts` l. 110) blames a file watcher
   that doesn't exist. It is replaced by the assertion here.

## Out of scope

- A shared popover primitive, and moving `ContextMenu` onto `placeAnchored`.
- Status and Cancel for post hooks other than a merge commit's `post-merge` (D2-A).
- Changing git-parity deletions in Stash all without untracked files (finding 1).
- A WAI-ARIA tree for the branch sidebar (#143).
