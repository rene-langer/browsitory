# Data-safety bundle

## Goal

Close seven data-safety follow-ups from the folder/file-swap (#186/#203), hooks (#183/#215),
worker-gate (#181/#218), worktree-lock (#162/#191) and Windows-walker (#193/#194/#214) batches:

| Issue | Area | Problem |
|---|---|---|
| #206 | stash | Applying a stash whose untracked `d/x` sits where the index has the file `d` reports success, leaves `d~Updated upstream`, and Pop drops the stash. |
| #208 | stash | Stashing deletes an ignored file or folder in the way of a restored path (both Include-untracked modes). |
| #211 | stash | Stashing deletes an untracked nested repository where the restored tree has a file (both modes). |
| #226 | commit | A leftover `CHERRY_PICK_HEAD` (D7-A) gives the next unrelated commit the picked commit's author. |
| #223 | workers | Workers and gates are keyed by the raw path string; one repository opened via two spellings gets two serial workers. |
| #200 | worktree | Worktree add fails when the destination's parent is missing, leaves `.git/worktrees/<name>` and the new branch, and says "git operation failed". |
| #209 | walker | Writing an existing read-only file (conflict resolution, Add to .gitignore) fails with "Access is denied" / "Permission denied". |

## Shipping

Five PRs (D7-A), one CHANGELOG section each at the top of `## [Unreleased]`:

- **PR 1** `fix/stash-in-the-way`: #206, #208, #211. `### Stash: things in the way`. `git-core/src/stash.rs`, `tests/stash.rs`.
- **PR 2** `fix/pick-leftover-author`: #226. `### Cherry-pick leftovers`. `git-core/src/commit.rs`, `pick.rs`, `tests/pick.rs`, `tests/pick_sequence.rs`.
- **PR 3** `fix/repo-identity`: #223. `### Repository identity`. `repo-service/src/worker/{mod.rs,registry.rs}`, `tauri-app/src/commands/mod.rs`, `vscode-sidecar/src/{dispatch.rs,main.rs}`.
- **PR 4** `fix/worktree-parents`: #200. `### Worktree add`. `git-core/src/worktree.rs`, `tests/worktree.rs`.
- **PR 5** `fix/read-only-writes`: #209. `### Read-only files`. `git-core/src/worktree_guard.rs`, `worktree_guard/beneath_{unix,windows}.rs`, `merge.rs`, `stage.rs`, `tests/merge.rs`, `tests/discard_ignore.rs`.

No source file is shared between PRs; they run in parallel. No new dependency. New user-visible strings are listed per section.

## Current state (main at 34d5576b, checked 2026-11-01)

### libgit2 1.9.6 (vendored `libgit2-sys-0.18.7+1.9.6`)
- `git_stash_apply` (stash.c l.1036): `ensure_clean_index` (l.971, index must equal HEAD);
  modified merge `merge_index_and_tree(stash_parent, repo_index, stash_tree)` (l.1107); untracked
  merge `merge_index_and_tree(NULL, repo_index, untracked_tree)` (l.1117) — against the **index**;
  untracked checkout first (l.1126), modified second (l.1149). A D/F conflict in the untracked merge
  is checked out as `<path>~Updated upstream` with success.
- Stash save resets with `GIT_CHECKOUT_FORCE` (+`REMOVE_UNTRACKED` with untracked) to HEAD, or to
  the index commit with keep-index (`reset_index_and_workdir`, l.650; l.751). Untracked `TREE`
  entries (nested repositories) are never stashed (`stash_update_index_from_diff`, l.221).
- checkout.c: a non-folder blocker of a restored folder → `checkout_action_with_wd_blocker`
  (l.564) `FORCE → REMOVE_AND_UPDATE`, ignored or not; a folder where the restored tree has a file
  → `checkout_action_with_wd_dir` (l.600) `UNMODIFIED → REMOVE_AND_UPDATE`, removed by
  `git_futils_rmdir_r(.., REMOVE_FILES)` (l.1828), nested `.git` included. `wd_item_is_removable`
  (l.343, protects nested repos) applies only to worktree-only items.
- `git_worktree_add` (worktree.c l.304): creates `<commondir>/worktrees/<name>` (`EXCL`, l.363),
  then the work dir with `git_futils_mkdir(worktree, 0755, EXCL)` (l.369, no `MKDIR_PATH`); no
  cleanup on failure.

### #206 / #208 / #211 (`stash.rs`)
- `save_stash` l.144; `untracked_in_the_way` called only when `!include_untracked` (l.155–159).
- `untracked_in_the_way` l.483–555: `include_ignored(false)`; a folder entry (`d/`, nested repo)
  is trimmed and only matched against `folders` and proper prefixes.
- Path-limited route (l.229) is safe: `DirectoryReplacedNonDirectory` (l.286–311) and
  `FolderIsNowAFile`/`UntrackedInTheWay` (l.315–331) refuse; it can't create a mirror stash.
- `apply_stash` l.627: `refuse_linked_folders`, `refuse_untracked_file_over_folder` (l.651,
  l.695–748: untracked path that is a worktree folder only). `pop_stash` l.753 drops on `Applied`.
- Tests: `file_replaced_by_folder()` tests/stash.rs l.1126; `git()` twin helper l.1135;
  `top_level` l.1144; `assert_untracked_in_the_way` l.1237.

### #226 (`commit.rs`, `pick.rs`)
- `commit_with` l.533: merge leftover guard l.552–560 (`merge_already_committed` l.673,
  `CommitError::LeftoverMergeState` l.44–45); `pick_author` l.562–572; `finishes_pick` +
  `cleanup_after_commit` l.645–655.
- Leftover sources (D7-A): `pick::step` l.436, `finish_paused` l.845, `commit_with` l.654.
- `commit_matches_pick` pick.rs l.906 (parent, author incl. time, message, changes).
- Tests: tests/pick.rs l.736 `a_cleanup_failure_after_a_clean_pick_still_counts_it`; merge
  analogues tests/hooks.rs l.1302–1349; `common::ReadOnlyGitDir` common/mod.rs l.263.

### #223 (workers)
- `repo_service::worker::ensure_worker` worker/mod.rs l.1905 (tests l.6023, l.6595);
  `Worker::spawn` l.733 opens with `git_core::repo::open` = `Repository::discover` (repo.rs l.17).
- Tauri `commands/mod.rs`: `AppState { workers, gates }` l.1723–1728; `open_repo` l.1734;
  `close_repo` l.1748, `forget_repo` l.1754; `RepoWorker` l.1999, `repo_worker` l.2056; tests use
  `state.workers`/`state.gates` (l.2192, 2271, 2313, 2427, 3791).
- Sidecar `dispatch.rs`: `Repos` l.34, `worker_handle` l.206, `open_repo` l.221, `close_repo`
  l.241; `main.rs` l.33; tests l.4227, 4329, 4370.
- Frontend: `useOpenRepos.ts` compares raw strings; `useHookActions.ts` l.30 filters events by the
  spelling the command was called with (hook channels use the command's `repo_path`).
  WorktreePanel "Open" uses `list_worktrees`' canonical path (a second spelling of the main
  worktree). VSCode uses lowercase drive letters on Windows.

### #200 (`worktree.rs`)
- `create_worktree_with` l.98–146: `PathExists` check, `nearest_existing` probe (#162), branch
  created at l.140 before `repo.worktree` l.144. `WorktreeError::Git` = "git operation failed".
- `worktree_guard::nearest_existing` worktree_guard.rs l.419.

### #209 (walker)
- Windows `write` beneath_windows.rs l.925–965: `open_leaf(.., Purpose::Read)` then
  `reopen_same(.., GENERIC_READ | GENERIC_WRITE)` l.957 (`reopen_same` l.710) → access denied on
  `FILE_ATTRIBUTE_READONLY`. Deletes already clear it (`delete_classic_with` l.260, #194).
- Unix `write` beneath_unix.rs l.293–330: `openat(O_WRONLY|O_CREAT|O_NOFOLLOW|O_NONBLOCK)` →
  `EACCES` on a 0444 file (non-root). Same user-visible failure.
- Callers: `merge::resolve_conflict` l.373 (write l.384), `merge::resolve_add_delete_conflict`
  l.722 (write l.745), `stage::ignore_path` l.355 (write l.383). `MergeError::Io` "failed to write
  resolved file: {0}", `StageError::Io` "{0}".

## Design

### 1. #206: untracked file below a tracked file (D1-A)
- `fn refuse_untracked_below_tracked_file(repo, index) -> Result<(), StashError>` in `stash.rs`,
  called in `apply_stash` right after `refuse_untracked_file_over_folder`. Builds the set of the
  repository index's stage-0 paths (exact case, as libgit2's merge uses
  `GIT_ITERATOR_DONT_IGNORE_CASE`); walks the stash's untracked tree; for the first file with a
  proper prefix in the set → `StashError::UntrackedBelowTrackedFile { untracked, file }`:
  "This stash can't be applied here: its untracked file '{untracked}' needs a folder where
  '{file}' is tracked. Commit the deletion of '{file}' first, then apply it; the stash is kept."
  (Review: "is tracked" rather than "is a tracked file", since `file` can be a symlink.)
- Nothing written, stash kept (Pop never reaches the drop).

### 2. #208 / #211: anything the reset would delete (D2-A)
- Rename `untracked_in_the_way` → `entry_in_the_way(repo, keep_index, include_untracked) ->
  Result<Option<InTheWay>, StashError>`; `struct InTheWay { path, tracked, kind }`,
  `enum Kind { Untracked, Ignored, Repository }`. Called for every whole stash (both modes),
  before the hazard route, as now.
- Status: `include_untracked(true)`, `recurse_untracked_dirs(true)`, `include_ignored(true)`,
  `recurse_ignored_dirs(false)`, `exclude_submodules(true)`. Per entry (`WT_NEW` or `IGNORED`):
  - `is_dir` = raw path ends with `/`; `path` trimmed.
  - Kind: `IGNORED` → Ignored; `WT_NEW` folder with `<workdir>/<path>/.git` → Repository; other
    `WT_NEW` → Untracked, skipped when `include_untracked` (it is stashed). Untracked symlinks skip
    as now (hazard route).
  - In the way when: a non-folder whose folded path is a restored folder; a folder whose folded
    path is a restored **file**; or any entry with a proper prefix that is a restored file.
- Errors (nothing stashed, nothing changed):
  - `UntrackedInTheWay` (unchanged text);
  - `IgnoredInTheWay { ignored, tracked }`: "Stashing would delete the ignored '{ignored}', which
    is in the way of the tracked '{tracked}'. Move '{ignored}' first."
  - `RepositoryInTheWay { path, tracked }`: "Stashing would delete the repository in '{path}',
    which is in the way of the tracked '{tracked}'. Move '{path}' first."
- Path-limited stashes keep their existing checks.

### 3. #226: leftover pick state (D3-A)
- `pick.rs`: `pub(crate) fn commit_carries_pick(repo, head, expected_parent, picked) ->
  Result<bool, git2::Error>` = parent count 1 and `expected_parent`, author name/email/time equal,
  and the clean-change coverage loop. `commit_matches_pick` = that + the message check.
- `commit.rs` `commit_with`, after the merge guard: if `repo.state()` is `CherryPick` /
  `CherryPickSequence`, `CHERRY_PICK_HEAD` peels to `picked`, HEAD has one parent and
  `commit_carries_pick(repo, head, head.parent_id(0), picked)` → `repo.cleanup_state()`
  (failure → `CommitError::LeftoverPickState`: "This cherry-pick was already committed, but git's
  cherry-pick state can't be cleared. Check the permissions of the .git folder, then commit
  again."). `pick_author` is then `None`, `message_source` sees no `MERGE_MSG`.
- Reverts unchanged (the author is the user either way).

### 4. #223: one worker per repository (D4-A)
- New `crates/repo-service/src/worker/registry.rs`:
  - `pub fn repo_key(path: &Path) -> Result<String, String>`: `git_core::repo::open` (discover);
    root = `workdir()` or `path()` (bare); `std::fs::canonicalize(root)`, fallback: the root with
    trailing separators trimmed; `to_string_lossy`. Error text as `Worker::spawn`'s today.
  - `pub struct WorkerRegistry { inner: Mutex<Inner> }`, `Inner { workers: HashMap<String,
    Worker>, aliases: HashMap<String, String> }`.
  - `open(&self, path) -> Result<(String, WorkerHandle), String>`: always recomputes the key
    (outside the lock); under the lock reuses a live worker for the key or evicts a dead one;
    spawns outside the lock (as `ensure_worker` does), re-locks and prefers an existing entry;
    records `aliases[path] = key`.
  - `get(&self, path) -> Option<(String, WorkerHandle)>`: alias lookup only (no file system).
  - `close(&self, path) -> Option<String>`: removes the alias; when no alias maps to its key,
    removes the worker and returns the key.
  - `ensure_worker` is replaced by `WorkerRegistry::open` (its eviction semantics kept).
- Tauri: `AppState { registry: WorkerRegistry, gates: Mutex<HashMap<String, Arc<RepoGate>>> }`
  (gates keyed by key; lock order registry → gates). `open_repo` → `registry.open`;
  `forget_repo` → `if let Some(key) = registry.close(path) { gates.remove(&key) }`;
  `repo_worker` → `registry.get(path)` (error "repo not open: {path}" unchanged).
- Sidecar: `Repos = Arc<WorkerRegistry>`; `worker_handle`, `open_repo`, `close_repo` use it.
- Unchanged: events keep the caller's spelling; config (recent repos, open tabs, per-repo
  settings) keeps raw paths; the frontend is untouched.

### 5. #200: worktree parents (D5-A)
- `create_worktree_with`, after the case probe, before the branch: for each missing ancestor of
  `path` below `nearest_existing(path)` (top-down) `std::fs::create_dir`, pushing it on
  `created`; `AlreadyExists` for a folder → continue; other errors →
  `WorktreeError::CreateFolder { path, reason }` ("Can't create the folder '{path}': {reason}"),
  after rollback.
- Rollback on any later error: `remove_dir` each `created` (deepest first; only empties);
  `<repo.commondir()>/worktrees/<name>` when it didn't exist before and has no `gitdir` file
  (`remove_dir_all` of that folder only); the branch when this call created it
  (`Branch::delete`, ignoring errors). Original error returned.

### 6. #209: read-only files (D6-A)
- `worktree_guard.rs`: `#[derive(Debug)] pub(crate) struct ReadOnlyFile;` (`Display`: "the file is
  read-only", `std::error::Error`); `pub(crate) fn read_only_error() -> io::Error`
  (`PermissionDenied` wrapping it) and `pub(crate) fn is_read_only(&io::Error) -> bool`.
  `write_beneath` doc: returns that error for a read-only existing file, nothing written.
- Windows `write`: `reopen_same` error `ERROR_ACCESS_DENIED` and
  `attribute_tag(&first)?.file_attributes & FILE_ATTRIBUTE_READONLY != 0` → `read_only_error()`.
- Unix `write`: `open_at` `EACCES` → `stat_at(parent, leaf)`: regular file, `st_uid == geteuid()`,
  `st_mode & S_IWUSR == 0` → `read_only_error()`; else the original error.
- `MergeError::ReadOnlyFile(String)` and `StageError::ReadOnlyFile(String)`: "'{0}' is read-only.
  Make it writable, then try again." Mapped at merge.rs l.384, l.745 and stage.rs l.383.

## Decisions (provisional choice in **bold**; the plan implements it)

On 2026-11-01 the user accepted every recommended choice: D1-A, D2-A, D3-A, D4-A, D5-A, D6-A
and D7-A (five PRs).

**D1 (#206). User-facing.** **A. Refuse up front, keep the stash.** B. Own ordered apply (also fixes `FileReplacedFolder`). C. A + refuse creating such stashes. Recommendation **A**.

**D2 (#208/#211). User-facing.** **A. Refuse ignored entries and nested repositories in both modes.** B. Nested repositories only; document ignored. C. Document only. Recommendation **A**.

**D3 (#226). User-facing (minor).** **A. Clear the leftover when HEAD carries the pick (author+time, changes).** B. Also require the message. C. Refuse until Abort. Recommendation **A**.

**D4 (#223). Engineering.** **A. Backend registry, canonical work-tree key, aliases.** B. A + canonical identity in the frontend (tab dedup, persisted paths; user-facing, migration). C. Lexical normalization only. Recommendation **A**.

**D5 (#200). User-facing.** **A. Create parents (git parity) + rollback.** B. Refuse with a clear message. C. Create through the held-folder walker. Recommendation **A**.

**D6 (#209). User-facing.** **A. Refuse with "'x' is read-only…" on both platforms.** B. Clear read-only and write (stays writable, git's end state). C. Clear, write, restore. Recommendation **A**.

**D7. PRs. Process.** **A. Five.** B. Four (#200 + #226 together). C. One. Recommendation **A**.

## Testing

Real temp-dir repositories (`init_repo`, hermetic git), never a mocked `Repository`. Unix
permission tests skip when `geteuid() == 0`. Windows behaviour is verified on CI (`windows-latest`).

- **#206/#208/#211** (`tests/stash.rs`): characterization first (current deletion / `d~` file),
  each with a git CLI twin recording git's behaviour; then refusals in both modes and keep-index,
  with the stash list, `refs/stash`, status and the entries in the way unchanged; unrelated
  ignored folders don't block; the mirror stash applies after its deletion is committed.
- **#226** (`tests/pick.rs`, `tests/pick_sequence.rs`): leftover after a clean pick (D7-A via a
  `post-commit` that makes `.git` read-only) → next commit authored by the user, state Clean;
  uncleanable leftover → `LeftoverPickState`, HEAD unchanged; hook-rewritten message still
  detected; paused conflict pick still gets the picked author; last-step sequence leftover.
- **#223** (`registry.rs` inline, real repos): spellings (trailing slash, `sub/..`, subfolder,
  symlink on Unix, case on Windows/macOS) share one worker; a linked worktree doesn't; close
  refcounting; dead-worker respawn; unknown spelling → not open. Tauri: two spellings share one
  gate. Sidecar: open one spelling, call and close with another.
- **#200** (`tests/worktree.rs`): nested missing parents + git twin; rollback of created parents
  on a later failure; a file in the way → `CreateFolder`; Unix read-only parent → no
  `.git/worktrees/<name>`, no new branch, retry with the same name succeeds.
- **#209**: walker unit tests on each platform; integration (`tests/merge.rs`,
  `tests/discard_ignore.rs`) via `Permissions::set_readonly(true)`: content, index and conflict
  stages unchanged.

## Docs

- `docs/ARCHITECTURE.md`: "git2 API gotchas" stash bullets l.125–150 (in-the-way kinds, apply
  against the index, mirror refusal, keep-index resets once to the index commit with HEAD as the
  baseline); "Git hooks" l.590–600 (pick leftover); "Threading model" l.162–224 (registry, keys,
  aliases); "Worktree writes and symlinks" l.685–700 (read-only writes) and l.763 (worktree add
  parents).
- `docs/USER_GUIDE.md`: "Stashes" l.472; "Multiple repositories" l.37 (one worker per repository
  whatever the spelling); "Worktrees and submodules" l.547; "Resolving conflicts" l.108,
  "Discarding and ignoring files" l.139 (read-only message).
- `CHANGELOG.md`: one section per PR (see the plan).

## Risks

- **#208 D2-A** refuses Stash all in repositories where an ignored entry sits exactly in the way
  (previously silently deleted); include-ignored status adds one non-recursive status pass.
- **#223**: macOS `realpath` case behaviour unverified (CI test); a repository renamed while open
  keeps its alias (lookups don't touch the file system); every transport test that built the map
  directly changes.
- **#206**: the git-twin result for the mirror pop (issue says git fails) only changes docs.
- **#209**: Windows evidence is CI-only; an ACL-denied (not read-only) file keeps the raw error.
- **#200**: rollback deletes `.git/worktrees/<name>` only when this call created it and it has no
  `gitdir` file.

## Unrelated findings (not fixed here)

1. The worktree path field is free text; a relative path resolves against the app's working
   directory (`init_repo` refuses relative paths, `create_worktree` doesn't).
2. ARCHITECTURE l.131–133 says libgit2 resets to HEAD before checking the index out with
   keep-index; libgit2 1.9.6 resets once to the index commit (stash.c l.751). The conclusion holds
   (HEAD is the checkout baseline, so a blocker of a staged deletion is still removed).
3. A leftover `REVERT_HEAD` (D7-A) makes the next commit's `prepare-commit-msg` source `merge` and
   pre-fills the revert message; the author is unaffected.
4. Two VSCode windows run two sidecars, so one repository still gets two workers across
   processes (only the undo lock is cross-process).
5. Recent repos, open tabs and per-repository settings (graph branch selection) are keyed by the
   raw spelling; two spellings make two entries (D4-B would address it).
6. The mirror-stash check compares exact case, like libgit2's merge; under `core.ignorecase` a
   case-only variant isn't covered.

## Out of scope

- D1-B (own stash apply), D4-B (frontend identity), D6-B/C (writing read-only files).
- Revert leftovers, cross-process worker sharing.
