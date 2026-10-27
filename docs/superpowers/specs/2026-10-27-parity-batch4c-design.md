# Feature parity, batch 4c: merge and stash options

## Goal

Give "Merge into current branch" the usual fast-forward choices (allow, never, only) and squash,
and give stashes a message, keep-index, include-untracked, per-file stashing, pop, and a view of
a stash's contents including its untracked files. Both frontends (Tauri app, VSCode extension)
through `RepoClient`. Refines section 4c of
`docs/superpowers/specs/2026-10-27-parity-batch4-roadmap-design.md` (issue #92).

## Current state

- `git_core::merge::start_merge(repo, branch_name)` looks up `refs/heads/<branch_name>`, runs
  `merge_analysis`, fast-forwards whenever possible (checkout first, then `set_target`), else
  calls `repo.merge` and returns `Merged` or `Conflicted { files }`. A clean `Merged` does not
  commit: it leaves `MERGE_HEAD`/`MERGE_MSG`, the commit box is pre-filled from
  `get_merge_message`, and `commit::commit` reads `MERGE_HEAD` to write the two-parent commit and
  `cleanup_state`s.
- The frontend treats `state.mergeMessage !== null` as "a merge is in progress" (BranchTree's
  `isMerging`, `repositoryOperationDisabled`, the palette's "Abort merge", CommitBox's
  Abort button, which renders whenever `initialMessage !== undefined`).
- `WorkerHandle::start_merge(branch_name)` is the only merge entry. `undo::plan_for` records
  `StartMerge` as `Merge <branch>` with `SnapshotScope::Full`. A conflicted or uncommitted merge
  stays `pending` until the commit (one entry) or the abort (none), because
  `repo.state() == Merge`.
- `git_core::stash`: `save_stash(repo)` calls `stash_save2(sig, None, INCLUDE_UNTRACKED)`;
  `apply_stash(repo, index)` calls `stash_apply(index, None)`; `drop_stash`; `list_stashes`
  (reflog messages). No pop, no options, no file list.
- The Stash button is in `DiffPane`'s commit dock (`button` text "Stash", disabled while the
  status list is empty or a rebase is paused), not in the sync bar. It calls
  `announce("Changes stashed", appState.saveStash)`.
- `StashPanel` rows already select `{ commitId: stash.commitId }`, so clicking a stash today
  opens `CommitDiffPane` on the stash's worktree commit: its tracked changes against the HEAD it
  was made on. Untracked files (the stash's third parent) never show.
- Per-file actions live in `FileActionsMenu.fileMenuItems`, shown on unstaged, non-conflicted
  rows only (`hasFileActions`). The changed-file lists have no multi-select.
- Per-repo UI preferences that must survive restarts live in the `config` crate
  (`graph_branch_selections`: `get_*_at`/`set_*_at` plus thin wrappers, exposed by Tauri and the
  sidecar without going through the worker).

### Facts about the libraries this design depends on

Checked against the vendored sources (`git2` 0.21.0, `libgit2-sys` 0.18.7+1.9.6):

- `git2::StashSaveOptions` (needed for `stash_save_ext` and therefore for a pathspec) has
  `flags` and `pathspec` setters but **no message setter**. Its `raw()` always copies its private
  `message: None` into the raw options. A path-limited stash cannot get a message through the
  safe API.
- libgit2's `git_stash_apply` refuses with `GIT_EUNCOMMITTED` ("N uncommitted changes exist in
  the index") whenever the index differs from HEAD (`ensure_clean_index`), before touching
  anything.
- With `GIT_STASH_APPLY_REINSTATE_INDEX`, a stash whose staged part cannot be merged returns
  `GIT_ECONFLICT` before touching anything.
- When the worktree part conflicts with committed changes, `git_stash_apply` succeeds (returns
  0) and leaves the conflicts in the index and conflict markers in the files. Local
  modifications in the way make the safe checkout fail with `GIT_ECONFLICT` (class `Checkout`),
  leaving the files alone.
- `git_stash_pop` is apply followed by an unconditional drop: it drops the stash even when the
  apply left conflicts. Browsitory must not call it.
- The untracked-files commit (the stash's third parent) is created with **zero parents**, so
  `commit_files` / `commit_diff` on it list and diff every untracked file as added.
- A message given to `git_stash_save` becomes `On <branch>: <message>` (commit message with a
  trailing newline, reflog message trimmed). Without one it is `WIP on <branch>: <short> <summary>`.
- `git_merge` does not look at the merge analysis: called when a fast-forward is possible it
  still writes `MERGE_HEAD` and an index equal to the merged tree.
- `cleanup_state` removes `MERGE_HEAD`, `MERGE_MSG`, `MERGE_MODE` but not `SQUASH_MSG`.

## Decisions

### Merge

- **`MergeOptions { fast_forward: FastForward, squash: bool }`, `FastForward::{Allow, Never,
  Only}`**, as in the roadmap. `Default` is `Allow` without squash, today's behavior.
- **The merge target may be a full ref name.** `start_merge(repo, target, opts)` resolves
  `target` as a full ref when it starts with `refs/`, else as `refs/heads/<target>`. This fixes a
  pre-existing bug: "Pull has diverged → Merge" passes `refs/remotes/<remote>/<branch>`, which
  today becomes `refs/heads/refs/remotes/...` and fails with "reference not found". The UI still
  offers merging only local branches; remote-tracking merges go through the pull dialog.
- **`Never`** skips the fast-forward branch: `repo.merge` runs even when `is_fast_forward()`, so
  the result is `Merged` with `MERGE_HEAD` set and the commit box pre-filled, and the user's
  commit has two parents. Up-to-date is still `UpToDate`.
- **`Only`** fast-forwards when possible, is `UpToDate` when up to date, and otherwise returns
  `MergeError::NotFastForward(target)` ("Can't fast-forward: '<name>' has diverged from the
  current branch.") without touching anything.
- **Squash ignores `fast_forward`.** `git merge --squash --no-ff` is an error in git; here the
  dialog disables the fast-forward radios while Squash is checked, and the backend does not
  validate the combination. Squash: up to date → `UpToDate`; otherwise `repo.merge`, collect
  conflicts, write `SQUASH_MSG`, `cleanup_state` (no `MERGE_HEAD`, `MERGE_MSG`, `MERGE_MODE`),
  return the new `MergeOutcome::Squashed` or `Conflicted { files }`. HEAD never moves.
  `HeadInfo.operation` stays `None` because `repo.state()` is `Clean`.
- **`SQUASH_MSG` is git's format without the `Date:` line.** `Squashed commit of the
  following:` then, per commit (revwalk from the target hiding HEAD, `TOPOLOGICAL | TIME`, like
  `graph_log`), a blank line, `commit <sha>`, `Author: <name> <email>`, a blank line and the
  message indented by four spaces. Dropping `Date:` avoids a date formatter in git-core; the
  message is meant to be edited.
- **The squash message reaches the commit box through `HeadInfo`.** `HeadInfo` gains
  `squash_message: Option<String>` (contents of `<gitdir>/SQUASH_MSG`). The refresh already
  fetches `getHeadInfo`, so no new read command. It is not folded into `getMergeMessage`,
  because `mergeMessage !== null` means "merge in progress" everywhere in the frontend.
- **CommitBox seeds from `mergeMessage ?? headInfo.squashMessage`, and Abort shows only for a
  real merge.** CommitBox gains `showAbort?: boolean` (default: `initialMessage !== undefined`,
  today's behavior); `DiffPane` passes `showAbort={mergeMessage !== null}`. A squash has nothing
  to abort; Undo reverts it.
- **`SQUASH_MSG` is removed when it stops applying**: after a successful `commit::commit`,
  `merge::abort_merge`, `reset::reset_to` and `undo::restore` (git's `remove_branch_state`
  removes it on commit and reset too). Helper `merge::clear_squash_message(repo)`, ignoring a
  missing file.
- **Default options come from `merge.ff`**: `"only"` → `Only`; a false git boolean (`false`,
  `no`, `off`, `0`) → `Never`; anything else or unset → `Allow`; squash `false`.
  `merge_defaults(repo) -> MergeOptions`. `branch.<name>.mergeOptions` is ignored.
- **The dialog remembers the last confirmed choice per repo in the `config` crate**
  (`merge_preferences`, array-of-tables keyed by repo path, like `graph_branch_selections`).
  `RepoClient.getMergeOptions(repoPath)` returns the remembered choice, else the `merge.ff`
  default. `setMergeOptions` is called when the user confirms the dialog, before the merge runs,
  so a failed merge still remembers the choice.
- **"Pull has diverged → Merge" does not open the dialog** and uses `{ fastForward: "Allow",
  squash: false }`. It is a pull completion, not a branch merge; a fast-forward is impossible
  there anyway.
- **Undo.** `plan_for(StartMerge)` keeps `SnapshotScope::Full`. The label is `Merge <name>`, or
  `Squash merge <name>` when squashing, where `<name>` drops a `refs/heads/` or `refs/remotes/`
  prefix. A no-ff merge is recorded exactly like a merge today (pending until the commit). A
  clean squash is recorded at once (state is Clean): undo restores index and working tree and
  removes `SQUASH_MSG`. A conflicted squash is not recorded: the after-snapshot refuses a
  conflicted index (`UndoError::ConflictedIndex`, existing behavior). The commit that finishes it
  is recorded as a normal commit.

### Stash

- **`StashOptions { message: Option<String>, keep_index: bool, include_untracked: bool, paths:
  Vec<String> }`**. A blank or whitespace-only message is treated as `None`. Flags:
  `KEEP_INDEX` when `keep_index`, `INCLUDE_UNTRACKED` when `include_untracked`.
- **Without paths, `stash_save2` with the message** (native libgit2 wording). **With paths,
  `stash_save_ext` with a pathspec, then "restamp" the new stash when a message was given**,
  because git2 cannot pass one (see Facts). Restamp: read the new top stash commit, build
  `On <branch>: <message>\n` where `<branch>` is the text between `WIP on ` and the first `:`
  of its message, write a commit with the same tree, author, committer and parents, point
  `refs/stash` at it with the reflog message `On <branch>: <message>`, then remove the original
  entry (reflog index 1) with `rewrite_previous_entry = true` and write the reflog. The result
  is indistinguishable from `git stash push -m <message> -- <paths>`. Rejected alternatives:
  a direct `libgit2-sys` dependency with an `unsafe` FFI call (the crate has no `unsafe` today),
  and dropping the message for file stashes (the issue's E2E needs both).
- **Paths are pathspecs.** libgit2's stash has no literal-path flag, so a file name containing
  `*`, `?` or `[` also matches other files. Known limitation, written in the user guide.
  (Superseded: see "Deviations during implementation"; paths are literal.)
- **`StashError` gains user-facing variants**, mapped from libgit2 codes:
  - save, `NotFound` with class `Stash` → `NothingToStash` ("There are no changes to stash.").
  - apply/pop, index has conflicts (checked first) → `UnresolvedConflicts` ("Resolve the current
    conflicts before applying a stash.").
  - apply/pop, `Uncommitted` → `StagedChanges` ("Commit or unstage your staged changes before
    applying a stash.").
  - apply/pop, `Conflict` with class `Checkout` → `WouldOverwrite` ("Applying this stash would
    overwrite local changes to the same files. Commit, stash or discard them first.").
  - apply/pop, `Conflict` otherwise (restore-index merge) → `IndexConflict` ("The stash's
    staged changes conflict with the current files. Apply it without "Restore staged
    changes".").
  - `NotAStash(String)` ("No stash with id <id>.") for `stash_files`.
  Every other failure stays `Git(..)`.
- **`apply_stash(repo, index, restore_index) -> StashApplyOutcome`**, `Applied` or
  `Conflicted { files }` (index conflicts after the apply, paths via `merge::conflict_path`).
- **`pop_stash(repo, index, restore_index)`** = `apply_stash`, then `stash_drop(index)` only on
  `Applied`. On `Conflicted` the stash stays, as with `git stash pop`.
- **"Restore staged changes" is one checkbox shared by Apply and Pop, default on** (the roadmap
  pops with `reinstantiate_index`). Kept in `AppState.stashRestoreIndex` for the session, not
  persisted. Shown in the Stashes panel and in the stash view's header.
- **Stash conflicts use the existing conflict pane.** `repo.state()` stays `Clean`, so there is
  no "Abort merge" button and no `HeadInfo.operation`; the user resolves each file. Not undoable
  (stash ops are never recorded).
- **Apply and Pop are disabled while anything is staged or conflicted**, with the reason as the
  button title, because libgit2 refuses then anyway. The backend errors above remain the source
  of truth (another tool may change the index).
- **Viewing a stash: `stash_files(repo, commit_id) -> StashFiles { tracked: Vec<String>,
  untracked: Vec<String>, untracked_commit_id: Option<String> }` instead of the roadmap's
  `stash_diff(...) -> StashDiff { tracked: Vec<FileDiff>, ... }`.** There is no `FileDiff`
  type, and every other pane loads a file list first and each file's hunks lazily through
  `getCommitDiff`. Tracked files are `commit_files(stash commit)` (vs its first parent, the HEAD
  it was made on). Untracked files are `commit_files(third parent)`, and their hunks come from
  `getCommitDiff(untrackedCommitId, path)`, a whole-file add because that commit has no parent.
  Keyed by commit id, not index: selection is already `{ commitId }` and indices shift. An id
  that is not in the stash list is `NotAStash`.
- **Stash operations stay unrecorded by undo** (`plan_for` returns `None`, existing test
  `read_and_transfer_commands_are_not_recorded` keeps `SaveStash`).

### Frontend

- **Merge dialog (`MergeDialog`)**, opened by BranchTree's "Merge into current branch…" (label
  gains `…`) and the palette. `FormDialog` aria-label `Merge <branch>`, heading `Merge <branch>
  into <current>`. Radio group (fieldset legend "Fast-forward"): "Fast-forward if possible",
  "Always create a merge commit", "Fast-forward only". Checkbox "Squash" with hint "Stage the
  combined changes; you write the commit." Radios disabled while Squash is checked. Buttons
  "Merge" (submit, disabled until the options load) and "Cancel". Open state lives in
  `AppState.mergeDialog: { branchName: string } | null` so the palette can open it.
- **Merge outcomes become toasts**, like `pullOutcome`: `AppState.mergeOutcome` is set by
  `mergeBranch`; App shows "Already up to date", "Fast-forwarded to <branch>" and "Squashed
  <branch>. Review and commit." `Merged` and `Conflicted` stay silent (the commit box and the
  conflict list already show it).
- **Stash split button (`SplitButton` primitive).** The commit dock's main button keeps the
  text "Stash" and stashes everything as today (`stash-management.spec.ts` clicks
  `button=Stash`). A chevron button (aria-label "More stash options", `aria-haspopup="menu"`)
  opens a `ContextMenu` with "Stash all" and "Stash…". If batch 4b already added a split-button
  primitive for Fetch, reuse it instead of creating one.
- **Stash dialog (`StashDialog`)**, `FormDialog` aria-label "Stash changes": "Message" input
  (aria-label "Stash message", placeholder "Optional"), "Keep staged changes" (off), "Include
  untracked files" (on), and, for a file stash, "Only: <path>". Buttons "Stash" (submit) and
  "Cancel". Not persisted. Open state `AppState.stashDialog: { paths: string[] } | null`.
- **"Stash file…" in the file menu**, on every non-conflicted changed file. Unstaged rows get it
  under "Discard changes…"; staged rows, which have no menu today, get a menu holding just this
  item. It opens the stash dialog with `paths: [path]`. No multi-select exists in the changed
  file lists, so "Stash selected files" is one file at a time (the issue's "where supported").
- **Stash view (`StashDiffPane`)**: when the selected commit id is a stash's, `DiffPane`
  renders it instead of `CommitDiffPane`. `section` aria-label "Stash details": the stash
  message as heading, `stash@{n}` and the short id, a toolbar with Apply, Pop, Drop… and the
  "Restore staged changes" checkbox, then "Changes (N)" and, when non-empty, "Untracked (M)"
  groups of `CommitFileSection`s (Blame stays; History is hidden). Empty stash: "This stash has
  no file changes." Drop… opens the same confirm dialog text as the panel.
- **Stashes panel rows** gain Pop between Apply and Drop, and the panel shows the "Restore
  staged changes" checkbox above the list. Popping or dropping the selected stash selects
  "uncommitted" (as drop does today).
- **Apply/Pop outcome toasts**: "Stash applied", "Stash popped", and for conflicts "Stash applied
  with conflicts; the stash was kept. Resolve the files, then drop it." (pop) or "Stash applied
  with conflicts. Resolve the files." (apply).
- **Palette**: `merge-branch:<name>` "Merge <name> into current branch…" (Branches; omitted
  while `repositoryOperationDisabled`); `save-stash` relabelled "Stash all changes" (id kept so
  recents survive); `stash-with-options` "Stash…"; `stash-current-file` "Stash <file>…" when the
  diff pane has a current file; per stash `show-stash:<i>` "Show stash: <msg>", `pop-stash:<i>`
  "Pop stash: <msg>", the existing `apply-stash:<i>`. **`drop-stash:<i>` no longer drops
  directly** (pre-existing bug: it skipped the confirmation); it selects the stash and opens the
  Stashes section, where Drop asks first, the same rule as "Delete tag".

## Backend API

```rust
// git_core::merge
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FastForward { #[default] Allow, Never, Only }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MergeOptions { pub fast_forward: FastForward, pub squash: bool }

pub enum MergeOutcome { UpToDate, FastForwarded, Merged, Squashed, Conflicted { files: Vec<String> } }

// MergeError gains:
#[error("Can't fast-forward: '{0}' has diverged from the current branch.")]
NotFastForward(String),

pub fn start_merge(repo: &Repository, target: &str, opts: MergeOptions) -> Result<MergeOutcome, MergeError>;
pub fn merge_defaults(repo: &Repository) -> MergeOptions;
pub fn squash_message(repo: &Repository) -> Option<String>;
pub(crate) fn clear_squash_message(repo: &Repository) -> std::io::Result<()>;

// git_core::repo::HeadInfo gains
pub squash_message: Option<String>,

// git_core::stash
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashOptions { pub message: Option<String>, pub keep_index: bool, pub include_untracked: bool, pub paths: Vec<String> }
impl Default for StashOptions { /* None, false, true, [] */ }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StashApplyOutcome { Applied, Conflicted { files: Vec<String> } }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashFiles { pub tracked: Vec<String>, pub untracked: Vec<String>, pub untracked_commit_id: Option<String> }

pub fn save_stash(repo: &mut Repository, opts: &StashOptions) -> Result<(), StashError>;
pub fn apply_stash(repo: &mut Repository, index: usize, restore_index: bool) -> Result<StashApplyOutcome, StashError>;
pub fn pop_stash(repo: &mut Repository, index: usize, restore_index: bool) -> Result<StashApplyOutcome, StashError>;
pub fn stash_files(repo: &mut Repository, commit_id: &str) -> Result<StashFiles, StashError>;

// config
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeFastForward { Allow, Never, Only }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergePreference { pub repo_path: PathBuf, pub fast_forward: MergeFastForward, pub squash: bool }
pub fn get_merge_preference(repo_path: &Path) -> Result<Option<MergePreference>, ConfigError>;  // + _at
pub fn set_merge_preference(repo_path: &Path, fast_forward: MergeFastForward, squash: bool) -> Result<(), ConfigError>;  // + _at
```

## `repo-service`, Tauri, sidecar, `RepoClient`

- Worker commands: `StartMerge { branch_name, options: MergeOptions, reply }`,
  `GetMergeDefaults { reply }`, `SaveStash { options: StashOptions, reply }`,
  `ApplyStash { index, restore_index, reply: Sender<Result<StashApplyOutcome, String>> }`,
  `PopStash { index, restore_index, reply }` (same reply type), `GetStashFiles { commit_id,
  reply }`. `hold_reply` keeps holding `StartMerge` only (stash commands are not recorded).
- Tauri: `start_merge(repo_path, branch_name, options: MergeOptionsDto)`,
  `get_merge_options(repo_path)` (config preference, else `merge_defaults` through the worker),
  `set_merge_options(repo_path, options)`, `save_stash(repo_path, options: StashOptionsDto)`,
  `apply_stash(repo_path, index, restore_index)`, `pop_stash(...)`,
  `get_stash_files(repo_path, commit_id)`. DTOs: `MergeOptionsDto { fastForward: "Allow" |
  "Never" | "Only", squash }` (Serialize + Deserialize), `MergeOutcomeDto` gains `Squashed`,
  `StashOptionsDto` (camelCase), `StashApplyOutcomeDto` (kind-tagged), `StashFilesDto`
  (camelCase), `HeadInfoDto.squashMessage`.
- Sidecar: the same method names and params, with its own DTO copies.
- `RepoClient`:

```ts
export type FastForwardMode = "Allow" | "Never" | "Only";
export interface MergeOptions { fastForward: FastForwardMode; squash: boolean; }
export type MergeOutcome =
  | { kind: "UpToDate" } | { kind: "FastForwarded" } | { kind: "Merged" } | { kind: "Squashed" }
  | { kind: "Conflicted"; files: string[] };
export interface StashOptions { message: string | null; keepIndex: boolean; includeUntracked: boolean; paths: string[]; }
export type StashApplyOutcome = { kind: "Applied" } | { kind: "Conflicted"; files: string[] };
export interface StashFiles { tracked: string[]; untracked: string[]; untrackedCommitId: string | null; }
// HeadInfo gains squashMessage: string | null

mergeBranch(repoPath: string, branchName: string, options: MergeOptions): Promise<MergeOutcome>;
getMergeOptions(repoPath: string): Promise<MergeOptions>;
setMergeOptions(repoPath: string, options: MergeOptions): Promise<void>;
saveStash(repoPath: string, options: StashOptions): Promise<void>;
applyStash(repoPath: string, index: number, restoreIndex: boolean): Promise<StashApplyOutcome>;
popStash(repoPath: string, index: number, restoreIndex: boolean): Promise<StashApplyOutcome>;
getStashFiles(repoPath: string, commitId: string): Promise<StashFiles>;
```

## Testing

- `git-core` `tests/merge.rs`: no-ff creates a two-parent commit where ff was possible (after
  `commit::commit`); ff-only refuses a diverged branch and leaves HEAD, index and worktree
  untouched; ff-only fast-forwards when possible; full-ref target (`refs/remotes/origin/x` set up
  locally with `repo.reference`) merges; squash leaves staged changes, writes `SQUASH_MSG` with
  every squashed commit, no `MERGE_HEAD`, `repo.state()` Clean, HEAD unmoved, `head_info`
  reports the message and no operation; squash when ff possible still does not move HEAD; squash
  with a conflict reports `Conflicted` and clean state; commit after squash has one parent and
  removes `SQUASH_MSG`; abort after a conflicted squash removes it; `merge_defaults` for unset,
  `false`, `only`.
- `git-core` `tests/stash.rs`: message without paths; keep-index (staged change still staged
  after the stash and present in the stash's index commit); include-untracked off leaves the
  untracked file; path-limited stash leaves other tracked and untracked files; path-limited stash
  with a message lists `On <branch>: <message>` and keeps the older entry intact; nothing to stash;
  pop drops on success; pop keeps on conflict and reports the file; apply with restore index
  restores staged state; apply with staged changes → `StagedChanges`; blocked by dirty file →
  `WouldOverwrite` (existing test tightened); `stash_files` lists tracked and untracked files and
  `commit_diff` of an untracked file is a whole-file add; `stash_files` on a non-stash id →
  `NotAStash`.
- `git-core` reset/undo: reset and undo restore remove `SQUASH_MSG`.
- `config` `tests/merge_preferences.rs`: none by default, round trip, overwrite, per repo.
- `repo-service`: merge options and squash round-trip through the worker; undo labels
  `Merge feature` / `Squash merge feature` / `Merge origin/main`; squash is undoable; pop and
  stash files round-trip.
- `tauri-app`: DTO serialization for `MergeOptionsDto` (both directions), `MergeOutcomeDto`
  `Squashed`, `StashOptionsDto` deserialization, `StashApplyOutcomeDto`, `StashFilesDto`,
  `HeadInfoDto.squashMessage`.
- `vscode-sidecar`: `start_merge` with options, `get_merge_options`/`set_merge_options`,
  `save_stash` with options + `get_stash_files` + `pop_stash`.
- Frontend (mock `RepoClient`): both clients' wiring; `useAppState` merge with options and outcome,
  pull merge uses Allow, stash save options, pop selection reset, outcome toasts state;
  `MergeDialog`, `StashDialog`, `SplitButton`, `StashDiffPane`, `StashPanel` Pop and checkbox,
  file menu item on staged and unstaged rows, CommitBox seed and `showAbort`; palette entries
  including the drop-stash fix.
- E2E Tauri (`e2e/specs/stash-options.spec.ts`): stash one file with a message from its
  actions menu, open it in the Stashes panel and see the file in the stash view, Pop it from the
  view, and see the file's change back and the entry gone. Self-contained: own file, `git reset
  -q` first (libgit2 refuses to apply with a staged index), cleanup of any leftover entry with
  its message. `e2e/specs/merge.spec.ts` confirms the new dialog.

## Overlap with other batches

- **4b (merges first)** touches several of the same files. Re-check before Task 1:
  - `frontend/src/components/BranchTree.tsx`: 4b adds "Push to…" to the local branch menu next
    to "Merge into current branch", tag-badge menus and ahead/behind on rows.
  - `crates/config/src/lib.rs` `ConfigFile`: 4b persists the Fetch "Prune" checkbox; if it went
    to `config`, both batches add a trailing array-of-tables or plain field. Plain keys must stay
    above tables (see the struct's comment).
  - A split-button primitive: 4b's Fetch split button may have created one. Reuse it.
  - `RepoClient.ts`, both clients, every full `RepoClient` fixture literal, `tauri-app` `main.rs`
    `generate_handler!`, `commands/mod.rs` DTOs, the sidecar's method table: both batches append.
  - `GraphCommit` gains `tags` in 4b and `HeadInfo` gains `squashMessage` here: fixture literals
    in the same test files.
  - `frontend/src/lib/commands.ts` (`buildCommands`, `commandGroup`), `useAppState.ts` state shape.
  - `docs/USER_GUIDE.md` "Branches and remotes", `CHANGELOG.md` Unreleased.
- **4g (later)** edits `merge.rs` (`resolve_whole_file`, `mark_resolved`) and the conflict pane
  header. After 4c, conflicts also exist without `MERGE_HEAD` (squash, stash apply/pop); 4g's
  whole-file and mark-resolved actions must work from index conflicts alone, as
  `resolve_conflict` does, and must not assume `repo.state() == Merge`.
- The E2E stability branch (#98–#100, `fix/e2e-stability`) changes `wdio.conf.ts` storage
  isolation. The new spec uses `expandSidebarSection` (idempotent) and no localStorage state.

## Docs

- `docs/USER_GUIDE.md`: "Branches and remotes" gets a "Merge options" paragraph; "Stashes" is
  rewritten (split button, dialog, file stash, view, pop, restore staged changes, conflicts,
  pathspec caveat).
- `CHANGELOG.md` under Unreleased: "Merge and stash options".
- `docs/ARCHITECTURE.md` "git2 API gotchas": `StashSaveOptions` has no message (restamp),
  `git_stash_pop` drops on conflict, apply refuses a dirty index, `cleanup_state` leaves
  `SQUASH_MSG`.
- No `docs/LICENSE_COMPLIANCE.md` change (no new dependency).

## Deviations during implementation

Recorded after the pre-flight re-check on top of batch 4b (#113) and during implementation.

- **Path-limited stash is built by git-core, not by `stash_save_ext` + restamp.** libgit2
  1.9.6's `git_stash_save_with_opts` with a pathspec stashes the paths but then runs
  `reset_index_and_workdir` on the *whole* tree (`GIT_CHECKOUT_FORCE`, plus
  `REMOVE_UNTRACKED` with `INCLUDE_UNTRACKED`): every other modified, staged or untracked file
  is silently reverted or deleted, and the untracked commit holds *all* untracked files. A
  real-repo test (`path_limited_stash_leaves_other_files`) caught it. `stash::save_stash` with
  paths now writes the index commit (whole index, as git), a parentless untracked commit holding
  only the matched untracked files, and a worktree commit (HEAD's tree plus the matched files'
  worktree contents, clean-filtered), with libgit2's messages (`WIP on <branch>: <short>
  <summary>` or `On <branch>: <message>`). It pushes the stash on `refs/stash`
  (`reference_ensure_log` first) and only then reverts exactly the stashed paths (tracked: to
  HEAD in index and worktree, or to the index version in the worktree with `keep_index`;
  untracked: deleted, emptied directories removed). The "restamp" step and its reflog surgery
  are gone; the older-entries test now runs with three existing stashes. Stashing everything
  still uses `stash_save2`.
- **Stash paths are literal file paths**, not pathspecs (`disable_pathspec_match`), so the
  `*`/`?`/`[` caveat is gone from the user guide.
- **`SplitButton` is extracted from 4b's SyncBar Fetch split button** (4b built it inline, no
  primitive). `SyncBar` now renders `SplitButton` with a `chevronClassName` for its quieter
  toolbar styling; the Stash dock button uses it too. `menuTitle` was added for SyncBar's chevron
  title.
- **`merge.ff` "unset" is tested through the pure `merge::fast_forward_from_merge_ff`**; the
  real-repo test sets every value in the repo's own config, so a global `merge.ff` cannot leak in
  (issue #111). The sidecar round trip sets `merge.ff=true` in its fixture repo for the same
  reason.
- **4b stores the Fetch Prune choice in localStorage** (`frontend/src/lib/fetchPrune.ts`), not in
  the `config` crate, so `merge_preferences` is simply the last `ConfigFile` field.
- **StashPanel**: Apply/Pop are disabled when `applyDisabledReason` is set *or*
  `operationDisabled` (keeps the existing "pending" guard); the reason wins as title. The
  "Restore staged changes" checkbox shows only when there are stashes.
- **Stash view Drop** stays enabled regardless of `disabledReason`, as specified; the panel's
  Drop still follows `operationDisabled`.
- **E2E**: WebdriverIO cannot combine a CSS prefix with `button=Text`; the merge and stash specs
  chain `$(dialog).$("button=…")`. The stash-options spec reloads the page after its on-disk
  setup.
- `HeadInfo` fixtures got `squashMessage: null`; `GraphCommit.tags` (optional) and
  `BranchInfo.upstream/ahead/behind` from 4b needed no changes here. 4b's BranchTree menu items
  ("Push to…") sit next to the relabelled "Merge into current branch…" unchanged.

## Out of scope

Octopus merges, merge strategies (`-X ours`), `branch.<name>.mergeOptions`, stash branches
(`git stash branch`), staged-only stash, stashing ignored files, multi-select of changed files,
literal (non-pathspec) stash paths, an Abort button for squash or stash conflicts, a VSCode E2E
flow for this batch.
