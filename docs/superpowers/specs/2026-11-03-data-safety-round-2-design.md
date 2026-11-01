# Data-safety round 2

## Goal

Close four data-safety follow-ups found while reviewing the first bundle (#237, #238, #235, #242):

| Issue | Area | Problem |
|---|---|---|
| #247 | stash | Stash all loses an edit to a file in a folder removed from the index (`git rm -r --cached d`, not ignored); with Include untracked files on, Pop writes conflict markers and says "would overwrite". |
| #241 | pick | Committing the resolved step of a paused terminal `git cherry-pick A B C` deletes `.git/sequencer`, dropping B and C. |
| #239 | walker | `write_beneath` truncates an existing file before writing; a failed write leaves it empty or partial. |
| #245 | worktree | A worktree add failing during HEAD creation or checkout leaves a registered worktree, its branch checked out, and a folder; retry, prune and remove all fail. |

## Shipping

Four PRs (D6-A), one CHANGELOG section each at the top of `## [Unreleased]`:

- **PR 1** `fix/stash-index-removed`: #247. `### Stash: edits removed from the index`. `git-core/src/stash.rs`, `tests/stash.rs`.
- **PR 2** `fix/keep-sequencer`: #241. `### Terminal cherry-picks`. `git-core/src/commit.rs`, `pick.rs`, `merge.rs` (squash cleanup only), `tests/pick.rs`, `tests/pick_sequence.rs`, `tests/hooks.rs`.
- **PR 3** `fix/atomic-writes`: #239. `### Safe file writes`. `git-core/src/worktree_guard.rs`, `worktree_guard/beneath_{unix,windows}.rs`, `tests/merge.rs`, `tests/discard_ignore.rs`.
- **PR 4** `fix/worktree-checkout-rollback`: #245. Edits the existing `### Worktree add` section. `git-core/src/worktree.rs`, `tests/worktree.rs`.

No source file is shared between PRs; they run in parallel. No new dependency. No frontend change.

## Current state (main at 41bc70b4, checked 2026-11-01)

### libgit2 1.9.6 (vendored `libgit2-sys-0.18.7+1.9.6`)
- stash.c `build_workdir_tree` (l.379): index→workdir diff with `INCLUDE_UNTRACKED` but no
  `RECURSE_UNTRACKED_DIRS` (l.391); `stash_delta_merge` (l.360) turns DELETED+UNTRACKED into
  MODIFIED only for the same path, so `d/f` (DELETED) and `d/` (UNTRACKED tree) stay apart and
  `stash_update_index_from_diff` (l.221) removes `d/f` (l.252) and skips the tree. `build_untracked_tree`
  (l.274) recurses (l.292), so with untracked files `d/f` is in `^3`. Apply merges the untracked
  tree against the index (l.1117); without/with reinstate the index result comes from
  `stage_new_files` (l.1099) / `git_index_read_index(unstashed_index)` (l.1153).
- repository.c `git_repository_state_cleanup` (l.3779) deletes `state_files` (l.3767–3777):
  `MERGE_HEAD`, `MERGE_MODE`, `MERGE_MSG`, `REVERT_HEAD`, `CHERRY_PICK_HEAD`, `BISECT_LOG`,
  `rebase-merge/`, `rebase-apply/`, `sequencer/`. State `CherryPickSequence`/`RevertSequence` =
  pick head + `sequencer/todo` (l.3720–3731).
- worktree.c `git_worktree_add` (l.304): admin `mkdir EXCL` (l.363), destination `mkdir EXCL`
  (l.369), `.git`, `commondir`, `gitdir` (l.402), `create_head` (l.406), `checkout_head` (l.412);
  no cleanup. branch.c l.212: a branch checked out in any worktree can't be deleted.

### git 2.53 (probes)
- #247: `git stash push` records `d/f` = local in the worktree tree; `pop` → ` M d/f`, `pop --index`
  → `D d/f` + `?? d/`, local kept. `--keep-index` deletes `d/f`; pop conflicts (`DU`), local kept.
  `-u` records it in the worktree tree and `^3`; pop fails "d/f already exists, no checkout", local
  restored, stash kept.
- #241: `git commit` of a paused step removes `CHERRY_PICK_HEAD`/`REVERT_HEAD` and `MERGE_MSG`,
  keeps `sequencer/` (`--continue` finishes the rest); removes `sequencer/` when the todo has one
  line left. A single pick/revert keeps a remaining `sequencer/`. A merge commit keeps `BISECT_LOG`.
- #245: `git worktree add` whose checkout fails ("File name too long") removes the destination and
  `.git/worktrees/<name>` and keeps a branch it created with `-b`.

### Code
- `stash.rs`: `save_stash` l.165 (in-the-way l.174–195, route l.196–214), `changed_paths` l.238,
  `save_path_limited_stash` l.269 (status l.297–320, revert l.461–506),
  `kept_entry_in_untracked_folder` l.526, `entry_in_the_way` l.643 (`IgnoredOverwritten` l.707–729).
- `commit.rs`: `cleanup_after_commit` l.432–441, `commit_with` l.536 (merge leftover l.555–563,
  pick leftover l.564–570, finish l.652–665), `clear_pick_leftover` l.745–761.
  `pick.rs`: `step` l.331 (l.373, l.392, l.436), `continue_sequence` l.663 (l.680),
  `finish_paused` l.760 (l.780, l.824, l.845), `abort_sequence` l.1018 (l.1028).
  `merge.rs`: `squash_merge` l.195, `abort_merge` l.712.
- `worktree_guard.rs` `write_beneath` l.1016–1028; `beneath_unix.rs` `write` l.293–331
  (`set_len` l.328), `read_only_or` l.335, `stat_at` l.87; `beneath_windows.rs` `write`
  l.926–970 (`set_len` l.965), `create_leaf` l.898, `reopen_same` l.710, `nt_create` l.449,
  `delete_by_handle` l.210, `set_attributes` l.282, `final_path` l.575, ntdll imports l.356.
  Callers `merge::write_resolved` l.398, `stage::ignore_path` l.386.
- `worktree.rs`: `From<git2::Error>` l.26–34, `create_worktree_with` l.100–190 (rollback
  l.172–188), `remove_worktree` l.239–267.

## Design

### 1. #247: route edits removed from the index (D1-A)
- `stash.rs`: `fn edit_removed_from_index(repo) -> Result<bool, StashError>`: status with
  `include_untracked(true)`, `recurse_untracked_dirs(true)`, `include_ignored(false)`,
  `exclude_submodules(true)`; true when any entry has both `INDEX_DELETED` and `WT_NEW` and its
  work-tree path is a non-folder (`symlink_metadata`, no follow).
- `save_stash`: after `entry_in_the_way`, the route condition becomes `has_hazard ||
  (include_untracked && kept_entry_in_untracked_folder) || edit_removed_from_index`. The
  path-limited stash records such a path as tracked (worktree tree = the edit, index commit
  without it, never in the untracked commit), reverts it to HEAD (index entry restored), or with
  keep-index deletes it (git's result).
- Doc comment cites stash.c l.391/l.360/l.292. No new error, no new message.

### 2. #241: git's post-commit cleanup (D2-A)
- `commit.rs`: `pub(crate) fn clear_commit_state(repo) -> std::io::Result<()>`:
  1. `had_pick` = `CHERRY_PICK_HEAD` or `REVERT_HEAD` exists in `repo.path()`;
  2. remove (NotFound ignored) `MERGE_HEAD`, `MERGE_MODE`, `MERGE_MSG`, `CHERRY_PICK_HEAD`,
     `REVERT_HEAD`;
  3. if `had_pick` and `sequencer/todo` holds at most one line (git's
     `have_finished_the_last_pick`: no `\n`, or nothing after the first one) → `remove_dir_all`
     of `repo.path()/sequencer`. A missing or unreadable todo keeps the folder.
  Never touches `BISECT_LOG`, `rebase-*`, `AUTO_MERGE` or `SQUASH_MSG`.
- `cleanup_after_commit` calls it (failure still logged, D7-A of round 1); `clear_pick_leftover`
  becomes `clear_commit_state(repo).map_err(..)` (`LeftoverPickState` unchanged); the merge
  leftover l.559 uses it (`LeftoverMergeState` unchanged).
- `pick.rs` l.373, l.392, l.680, l.780, l.824 and `merge.rs` l.195 use it instead of
  `cleanup_state` (l.436/l.845 already go through `cleanup_after_commit`). Abort paths
  (`abort_merge` l.712, `abort_sequence` l.1028) and `rebase.rs` keep `cleanup_state`.

### 3. #239: write to a temp file, then rename (D3-A)
- `worktree_guard.rs`: `#[derive(Debug)] pub(crate) struct FileInUse { name: String }`
  (`Display`: "'{name}' is open in another program. Close it, then try again.");
  `pub(crate) fn is_in_use(&io::Error) -> bool`. `write_beneath` doc: the file is replaced
  whole; on failure the old content stays and no temporary file is left; mode (Unix) or settable
  attributes (Windows) and the on-disk name kept; ACLs, extended attributes and alternate
  streams are not; a hardlink is replaced, not written through.
- Both walkers: `pub(super) fn write(..)` = `write_with(.., &mut |file, bytes| file.write_all(bytes))`;
  `write_with` takes the fill function (test seam).
- Unix `write_with`: walk (as now); `stat_at(parent, leaf)`:
  - folder → `InvalidInput` "not a regular file"; FIFO/socket/device → same error, nothing opened;
  - regular file → permission probe `open_at(O_WRONLY|O_NOFOLLOW|O_NONBLOCK)` (no create, no
    truncate; `EACCES` → `read_only_or`), `fstat` it, keep `mode & 0o7777`, close;
  - link or missing → mode = none (created with `0o666` and the umask).
  Temp: `.{leaf}.browsitory-{pid}-{n}.tmp` with `O_WRONLY|O_CREAT|O_EXCL|O_NOFOLLOW`, retry on
  `EEXIST` (`ATTEMPTS`); `fchmod` to the kept mode; fill; `sync_all`; target name = the exact
  entry of the held parent matching `leaf` (exact first, else the single case-insensitive match
  when the leaf existed, else `leaf`); `renameat(parent, tmp, parent, target)`; best-effort
  `fsync(parent)`. Any failure after the temp exists → `unlinkat(parent, tmp)`, original error.
- Windows `write_with`: walk; `open_leaf(Read)` as now: Dir → error; Link → target name `leaf`
  (the rename replaces the link itself); File → `reopen_same(.., GENERIC_READ|GENERIC_WRITE)`
  permission probe (`read_only_or` unchanged), keep `attribute_tag(first).file_attributes &
  SETTABLE_ATTRIBUTES` and the target name `final_path(first).file_name()`, drop both handles.
  Temp: `nt_open(parent, tmp, GENERIC_WRITE|DELETE, FILE_SHARE_READ, FILE_CREATE,
  FILE_NON_DIRECTORY_FILE|FILE_OPEN_REPARSE_POINT, FILE_ATTRIBUTE_NORMAL)` + `in_parent`;
  `set_attributes(kept)`; fill; `sync_all`; rename through the temp handle with
  `NtSetInformationFile` (`FileRenameInformationEx` = 65, flags `REPLACE_IF_EXISTS |
  POSIX_SEMANTICS`, `RootDirectory` = held parent; on `ERROR_INVALID_PARAMETER`/`NOT_SUPPORTED`
  `FileRenameInformation` = 10 with `ReplaceIfExists`); `ERROR_SHARING_VIOLATION` (or
  `ACCESS_DENIED` while the target exists and is not read-only and a share-everything open of
  it is refused) → `FileInUse`. Any failure → `delete_by_handle(temp)`.
- `merge.rs`/`stage.rs` unchanged: `MergeError::Io` shows "failed to write resolved file: 'f.txt'
  is open in another program. …", `StageError::Io` the text alone.

### 4. #245: roll back a failed checkout (D4-A, D5-A)
- `worktree.rs`: `WorktreeError::AddFailed { reason: String }` ("Couldn't create the worktree:
  {reason}.") and `WorktreeError::AddFailedFolderLeft { reason: String, path: PathBuf }`
  ("Couldn't create the worktree: {reason}. Its folder '{path}' couldn't be removed completely;
  delete it by hand."). Every error of `repo.worktree(..)` maps to one of them (`reason` =
  `git2::Error::message()`); branch lookup and creation errors keep their mapping.
- Rollback when `added` fails and the admin folder didn't exist before (`admin_existed` false):
  - `ours` = no `gitdir` file, or its trimmed content canonicalizes to `canonicalize(path)/.git`;
    not ours → leave the admin folder (as now) and skip the folder cleanup below;
  - destination (created by libgit2 this call: `PathExists` ruled it out at l.108): for each
    blob/link path of the branch commit's tree, `worktree_guard::remove_and_prune_beneath(path,
    rel)`; then `remove_beneath(path, ".git")`; then `remove_dir(path)`; folder still there →
    `AddFailedFolderLeft`;
  - `remove_dir_all(admin)`; then the branch this call created (`Branch::delete`, now not checked
    out); then `remove_created_folders`.
- `remove_worktree` unchanged (no force, D5-A).

## Decisions (choice in **bold**; the plan implements it)

The user accepted all A choices (D1-A through D6-A) on 2026-11-01.

**D1 (#247). User-facing.** **A. Route through the path-limited stash (edit recorded, Pop restores).** B. Refuse. Recommendation **A**.

**D2 (#241). User-facing.** **A. git's post-commit cleanup at every commit/clear site (commit box, Continue, single pick, squash); the list goes only after its last step.** B. Commit box only. Recommendation **A**.

**D3 (#239). User-facing (Windows).** **A. A file held open against replacing → "'f.txt' is open in another program. Close it, then try again.", nothing changed.** B. Fall back to the in-place write. Recommendation **A**.

**D4 (#245). User-facing.** **A. Roll back only what the add wrote, the entry and the new branch.** B. Remove the whole new folder (git). C. No rollback, plain message only. Recommendation **A**.

**D5 (#245). User-facing.** **A. No "Remove anyway".** B. Add it with a confirmation. Recommendation **A**.

**D6. Process.** **A. Four PRs.** B. Three (PR 2 + PR 4). C. One. Recommendation **A**.

## Testing

Real temp-dir repositories (`init_repo`, hermetic git), never a mocked `Repository`. git CLI twins
record git's result next to Browsitory's. Unix permission tests skip as root. Windows walker and
worktree tests run on CI (`windows-latest`), pushed red before the fix.

- **#247** (`tests/stash.rs`): characterization on main for the four whole-stash modes
  (untracked × keep-index) of `d/f` and for the top-level `f` with Include untracked, each with a
  git twin; then: stash records "local" in the worktree tree only, status clean (index has `d/f`)
  without keep-index, `d/f` deleted with it; Pop → `Applied`, "local", ` M`-style status; Pop with
  restore_index → index without `d/f`; keep-index: Pop `StagedChanges` with the stash kept and
  "local" in it, then after unstaging Pop restores "local"; `git stash pop --index` of our stash
  works too.
- **#241** (`tests/pick.rs`, `pick_sequence.rs`, `hooks.rs`): a real paused `git cherry-pick A B C`
  (git CLI), resolved, `commit::commit` → `sequencer/todo` unchanged, `CHERRY_PICK_HEAD`/`MERGE_MSG`
  gone, `git cherry-pick --continue` applies B and C (twin: `git commit`); last step → `sequencer/`
  gone, `git status` clean; revert sequence; a single `cherry_pick` with a remaining `sequencer/`
  keeps it; a merge commit during `git bisect start` keeps `BISECT_LOG`; existing leftover tests
  stay green.
- **#239**: walker unit tests per platform through `write_with` (a fill that writes half and fails
  → old bytes, mode/attributes, no temp left; a new file → nothing left); FIFO not replaced; link
  replaced, target untouched; hardlink outside keeps old content (Unix); on-disk case kept
  (Windows, macOS); hidden attribute kept, in-use refusal, scanner-held file replaced (Windows);
  existing swap/junction/SMB tests green. Integration: conflict resolution and Ignore leave no
  temp file and keep the exec bit.
- **#245** (`tests/worktree.rs`): branch whose tree has a 300-byte file name (all OS) and `aux` /
  `a.` (Windows): new branch and existing branch → `AddFailed`, no `.git/worktrees/<name>`, no
  destination, new branch gone / existing branch kept and deletable, retry with a good branch and
  the same name succeeds; git twin (destination and entry gone, `-b` branch kept); a foreign file
  dropped into the destination (Unix: read-only folder makes a checkout path fail) →
  `AddFailedFolderLeft`, entry and branch gone, foreign file kept.

## Docs

- `docs/ARCHITECTURE.md`: "git2 API gotchas" l.148–151 (the rm --cached note: folders and
  Include untracked are routed; why); "Cherry-pick sequences" l.515–530 and "Git hooks" l.671–690
  (`clear_commit_state`, the terminal todo, `BISECT_LOG`); "Worktree writes and symlinks"
  l.744–800 (temp + rename, what is kept, `FileInUse`), l.900 (hardlink residual risk changes for
  writes), l.878–889 (worktree add rollback after `gitdir`, `AddFailed*`).
- `docs/USER_GUIDE.md`: "Stashes" l.537; "Rewriting and moving commits" l.700–725 (a terminal
  sequence carries on with `git cherry-pick --continue`); "Resolving conflicts" l.113 and
  "Discarding and ignoring files" l.149 (in-use message); "Worktrees and submodules" l.587.
- `CHANGELOG.md`: per PR (see the plan).

## Risks

- **#239 Windows**: relative rename with `RootDirectory` and sharing against the held parent are
  CI-verified only; editors that keep files open without `FILE_SHARE_DELETE` now get a refusal
  (D3-A) where the write used to succeed. Crash mid-write leaves a `.….browsitory-….tmp`.
  Unix: ownership of a group-writable file owned by someone else becomes the user's (as git's
  checkout); a folder the user may not create files in now fails.
- **#247**: more whole stashes take the path-limited route, so its refusals
  (`DirectoryReplacedNonDirectory`) apply when such a swap is also present.
- **#241**: a stale one-line todo from an older Browsitory run is removed on the next commit (git
  does the same).
- **#245**: rollback walks the branch's tree (one walk per path) only on a failed add; a racing
  process that wrote at a tree path loses that file.

## Unrelated findings (not fixed here)

1. `cleanup_state` deletes `BISECT_LOG`: before PR 2 every merge/pick commit ended a terminal
   bisect; after it, Abort (`abort_merge` l.712) and rebase finishes still do.
2. `pick::check_ready` refuses picks during a terminal bisect (state `Bisect`); git allows them.
3. `replace_beneath` (undo/redo restores) deletes the old file before writing the new one: same
   class as #239.
4. `remove_worktree` (worktree.rs l.245–249) returns "git operation failed" for **any** target when
   another registered worktree's folder is missing (`canonical_path(worktree.path())?` aborts the
   loop).
5. A leftover of a merge-commit pick with a mainline still takes the picked author (#241 note).
6. Abort of a terminal sequence's paused step removes its whole todo without moving HEAD back
   (git's `--abort` rewinds); worth a USER_GUIDE line or an issue.

## Out of scope

- D4-B/C, D5-B; `replace_beneath` atomicity; Abort semantics for terminal sequences; mainline
  leftovers.
