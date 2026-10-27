# Feature parity, batch 4g: conflicts, picks and polish

## Goal

Close the last four gaps of the batch 4 roadmap (gaps 10–13): whole-file conflict resolution and
"Mark resolved", cherry-pick/revert of several commits as one sequence (with Continue/Abort) and
of merge commits (with a mainline parent), an image preview for binary image diffs, and commit
message polish (summary counter, body ruler, co-author trailers, author override on amend). Both
frontends (Tauri app, VSCode extension) through `RepoClient`. Refines section 4g of
`docs/superpowers/specs/2026-10-27-parity-batch4-roadmap-design.md` (issue #96).

## Shipping: two PRs

The batch ships as two sub-batches, each its own PR, branch and CHANGELOG entry, implemented in
order from one plan (`docs/superpowers/plans/2026-10-27-parity-batch4g.md`, Part A then Part B):

- **4g-1 – Conflicts and picks** (gaps 10–11): `resolve_whole_file`, `mark_resolved`, mainline
  picks, the persisted cherry-pick/revert sequence, the conflict pane header, multi-select
  actions, parent picker, sequence progress in the status strip, and the batch's E2E flow.
- **4g-2 – Image diff and commit message polish** (gaps 12–13): `file_sides` + image preview,
  commit message settings (Preferences), counter and ruler, co-author trailers, amend author.

Why split:

- **Disjoint code.** 4g-1 lives in `merge.rs`, `pick.rs`, `repo.rs`, the worker's undo/in-progress
  bookkeeping, `ConflictResolutionPane`, `CommitGraph` and `OperationStatusStrip`. 4g-2 lives in
  `diff.rs`, `commit.rs`, `config`, `DiffView`/`DiffPane` file sections and `CommitBox`. The only
  shared files are the transport layers (`RepoClient.ts`, both clients, Tauri `main.rs`, sidecar
  `dispatch.rs`), which merge trivially.
- **Different risk.** The sequencer is a persisted state machine that touches undo recording and
  every "operation in progress" gate; it deserves a focused review. 4g-2 is mostly UI.
- **Different overlaps.** 4g-1 overlaps with 4c (conflicts from merges and stashes) and 4d
  (multi-select toolbar). 4g-2 overlaps with 4e (hooks on commit/amend) and 4f (Preferences).
  Splitting keeps each PR's pre-flight short.
- **Size.** One PR would be ~25 tasks and a few thousand lines of diff. Each half is about the
  size of batch 3c.
- The roadmap's single E2E flow (cherry-pick three commits, one conflict, whole-file ours,
  continue) belongs entirely to 4g-1. 4g-2 has no E2E (see Testing).

## Current state

Confirmed on `main` at `e1e2c8a` (batch 4a merged). 4b–4f merge before this batch; see
"Overlaps".

- `git-core::merge`: `conflict_hunks` parses `merge_file_from_index` output into
  `ConflictSegment`s and fails with `MergeError::NotATextConflict` when a stage is missing.
  `resolve_conflict(repo, path, content)` writes the file and `index.add_path`s it.
  `resolve_add_delete_conflict(repo, path, FileConflictChoice::{Ours, Theirs, Delete})` handles
  only add/delete conflicts and refuses content conflicts (`NotAnAddDeleteConflict`). It writes the
  blob with `std::fs::write`, so it skips filters (autocrlf) and file mode. There is no whole-file
  choice for a content conflict and no "mark resolved". `conflict_path` and `find_conflict` are
  crate-private helpers. `abort_merge` handles Merge, CherryPick(Sequence) and Revert(Sequence).
- `git-core::pick`: `cherry_pick(repo, id)` / `revert(repo, id)` share `apply(repo, id, Kind)`.
  The checks run in this order: state Clean (`OperationInProgress`), born HEAD (`UnbornHead`),
  `parent_count() > 1` (`MergeCommitUnsupported`), nothing staged (`DirtyIndex`), then
  `signing::signing_config` (3c preflight). Then `repo.cherrypick/revert(&target, None)`. On a
  conflict it returns `PickOutcome::Conflicted { files }` and leaves
  `CHERRY_PICK_HEAD`/`REVERT_HEAD` and `MERGE_MSG`. A clean result is committed through
  `signing::create_commit` with `HeadUpdate { expected: Some(head.id()), reflog_operation: "commit" }`.
  An empty result is `EmptyResult` after `cleanup_state`. Root commits pick and revert fine
  (checked).
- `git-core::commit::commit(&mut repo, message)` finishes a paused pick (author from
  `CHERRY_PICK_HEAD`) or revert and clears state. `amend_head(repo, message)` keeps the author and
  parents; committer = `repo.signature()`. **There is no `SignatureInput` type and no trailer
  support anywhere**; this batch adds both.
- `git-core::rebase::RebaseState` is **in memory only**. The worker keeps it in a local
  `rebase_state: Option<RebaseState>`, so a paused Browsitory rebase does not survive an app
  restart. The roadmap's "persisted sequencer state (like rebase)" has no rebase precedent. The
  only file Browsitory persists under `<gitdir>/browsitory/` is `undo.json`
  (`repo-service/src/worker/undo_store.rs`: versioned JSON, temp file + `sync_all` + `rename`).
  `sequence.json` follows `undo_store`, not rebase.
- `git-core::repo::head_info` → `HeadInfo { head_id, detached_at, head_pushed, operation:
  Option<OperationKind::{Merge, CherryPick, Revert}> }` from `repo.state()`. Rebase is
  deliberately absent (it has `rebase_progress`).
- Worker (`repo-service/src/worker/mod.rs`): `in_progress = repo.state() != Clean ||
  rebase_state.is_some()`. `undo::plan_for` labels `CherryPick`/`RevertCommit` as
  `"Cherry-pick <short>"` / `"Revert <short>"` (scope Full). A paused operation's entry stays
  `pending` until nothing is in progress, and the worker drops it when the ending command is in
  `aborts` (`AbortMerge`, `AbortRebase`). `hold_reply` must cover every command that can end a
  pending operation (`every_recordable_command_has_its_reply_held`).
- `git-core::diff`: hunks only; a binary file yields `[]`, which `DiffView` renders as "No text
  differences (binary file or mode-only change)". The frontend never sees blob ids.
- `config` crate: one TOML file. `[diff]` holds `DiffSettings`, exposed as
  `get/set_diff_settings` (Tauri command and sidecar method) and `DiffSettingsProvider`/
  `useDiffSettings` in the frontend. The TOML ordering comment in `ConfigFile` says tables must
  precede the array-of-tables fields.
- Frontend:
  - `ConflictResolutionPane` has per-segment Accept ours/theirs/both and "Save resolution". It
    detects add/delete by `message.includes("add/delete conflict")` and then offers Keep our/their
    version / Delete file.
  - `CommitGraph`: the single-commit menu has "Cherry-pick" (disabled on HEAD and merges) and
    "Revert" (disabled on merges). The multi-select toolbar (`role="toolbar"`, aria-label
    "Selected commits") and the multi context menu show four rebase items from
    `planRebaseSelection`, which yields `idsOldestFirst`.
  - `OperationStatusStrip` shows Merging/Cherry-picking/Reverting or "Rebasing, step N of M" with
    an Abort button only.
  - `DiffPane`'s commit dock shows `RebaseProgressPanel` (Continue/Abort rebase) during a rebase,
    otherwise `CommitBox`.
  - `App`'s `announce(label, action)` toasts `label` whenever the mutation ends without
    `state.error`.
  - `CommitBox` is a plain sans-serif textarea with Commit/Amend and an "Amend last commit"
    toggle. It has no counter, ruler, co-author or author field.
- VSCode webview CSP (`extension/src/webviewHtml.ts`) allows `img-src ${webview.cspSource}` only.
  `data:` images are blocked there today. Tauri's CSP already allows `img-src … data:`.

## Decisions

### Conflicts

- **`resolve_whole_file(repo, path, side: ConflictSide)`** with a new `ConflictSide { Ours,
  Theirs }`, separate from `FileConflictChoice`, which keeps `Delete` for the add/delete UI. It
  works for every conflict shape:
  - The chosen stage exists: check it out with
    `CheckoutBuilder::new().force().use_ours(side == Ours).use_theirs(side == Theirs)
    .disable_pathspec_match(true).path(path)` and `repo.checkout_index(None, …)`, so filters
    (autocrlf) and the file mode apply like git's `checkout --ours/--theirs`. Then
    `index.add_path` (which clears the conflict stages) and `index.write()`.
  - The chosen stage is absent (that side deleted the file): remove the working file if present,
    then `index.remove_path` (also clears the stages).
  - A symlink (`0o120000`) or gitlink (`0o160000`) on either side →
    `MergeError::UnsupportedEntry(path)` ("'{path}' is a symlink or submodule conflict. Resolve
    it with git."). It is rare, and getting it wrong is destructive.
  - "Ours" is always stage 2 (HEAD) and "theirs" stage 3 (the commit being applied), as in git.
    During a rebase that means "ours" is the branch being rebased onto. The pane's tooltip says
    so.
- **`mark_resolved(repo, path, force) -> MarkResolvedOutcome { Resolved, MarkersRemain }`.** The
  path must be conflicted (`NoConflict` otherwise). A working file that exists is `add_path`ed,
  and a missing one is `remove_path`ed (resolved as deleted). Unless `force`, a file whose content
  still has conflict markers is not staged and returns `MarkersRemain`. A structured outcome
  replaces an error string, so the UI does not need to match on message text. Marker test
  (`has_conflict_markers`): not binary (no NUL in the first 8000 bytes), and some line starts with
  `<<<<<<<` and a later line starts with `>>>>>>>`. A lone `=======` (a Markdown/RST underline)
  never counts.
- **If 4f already added a `mark_resolved` (merge-tool "Mark resolved"), reuse it.** Extend its
  signature with `force` and the marker check instead of adding a second function. Keep 4f's
  wire method name, and keep 4f's call sites passing `force: true` if their flow already
  confirmed with the user. The same applies to any frontend action 4f added
  (`markResolved` in `RepoClient`).
- **Neither records an undo entry**, like `ResolveConflict` (they happen inside a paused
  operation, whose single entry covers them).
- **Pane header** (`ConflictResolutionPane`): a toolbar with "Use ours (whole file)", "Use theirs
  (whole file)" and "Mark resolved", shown for text conflicts, for the generic error case (for
  example a binary conflict, where `conflict_hunks` output is meaningless) and alongside the
  existing add/delete buttons. Mark resolved returning `MarkersRemain` opens a `ConfirmDialog`:
  "⟨path⟩ still contains conflict markers. Mark it resolved anyway?" → **Mark resolved anyway**
  calls again with `force: true`.

### Mainline picks

- `cherry_pick(repo, id, mainline: Option<u32>)` and `revert(repo, id, mainline)`. A merge commit
  without `mainline` is `PickError::MainlineRequired` ("This is a merge commit. Choose which
  parent to apply it against."). It replaces `MergeCommitUnsupported`. `mainline` must be
  `1..=parent_count`; otherwise, or when given for a non-merge, it is
  `PickError::InvalidMainline` ("Parent {mainline} doesn't exist on this commit."). Passed through
  `CherrypickOptions::mainline` / `RevertOptions::mainline`. Root and single-parent commits keep
  `None`.
- A mainline revert's message matches git's: `Revert "<summary>"\n\nThis reverts commit <id>,
  reversing\nchanges made to <parent id>.\n`.
- Merge commits in the graph menu become **"Cherry-pick…"** / **"Revert…"** (ellipsis = opens a
  dialog). They are still disabled on HEAD for cherry-pick. `ParentPickerDialog` lists each
  parent: "Parent N", short id, the summary if the graph has that commit, and branch badges.
  Parent 1 is the default, with the hint "Parent 1 is usually the branch that was merged into."
  The confirm button is "Cherry-pick" / "Revert". The palette keeps excluding merge commits (it
  has no dialog step).

### Sequences

- **`cherry_pick_many(repo, ids)` / `revert_many(repo, ids)` take ids oldest-first** (the order
  `planRebaseSelection` and the graph already produce). Cherry-pick applies them in that order;
  revert applies them newest-first, as `git revert A..B` does. The backend does not re-sort:
  ordering arbitrary commits topologically needs a full revwalk, and the frontend already holds
  that order. Empty `ids` → `PickError::NothingSelected`.
- **Merge commits are refused in a sequence** (`MainlineRequired`), before anything is touched.
  One mainline cannot sensibly apply to several merges. The multi-select items are disabled with
  "Cherry-pick merge commits one at a time." / "Revert merge commits one at a time.".
- **Same preconditions as a single pick**, checked once up front: Clean state and no sequence
  file (`OperationInProgress`), born HEAD, nothing staged, signing config, every id resolves.
- **An empty step is skipped, not an error.** A commit whose changes are already on the branch
  (or that a whole-file "ours" resolution emptied) is recorded in `skipped` and the sequence goes
  on. git would stop and ask for `--skip`. Stopping would make every multi-pick of
  partly-applied history a chore, and the result toast names the skipped count.
- **`SequenceOutcome`:** `Completed { applied: usize, skipped: Vec<String> }` or
  `Conflicted { files, progress: SequenceProgress }`. `SequenceProgress { kind: SequenceKind
  { CherryPick, Revert }, current_step /* 1-based */, total_steps, current_commit }`.
- **The state machine lives in `git-core`, the file in `repo-service`.** `git-core` has no serde
  dependency, and Browsitory's only persisted state (`undo.json`) is already owned by
  `repo-service` (`undo_store.rs`). So `git-core::pick` works on a plain `SequenceState` value. Its
  entry points return a `SequenceRun { outcome, state: Option<SequenceState> }`: `Some` means
  "persist this", `None` means "no sequence any more". The new
  `repo-service/src/worker/sequence_store.rs` reads and writes the file. `git-core` tests drive the
  state machine with in-memory state, and `repo-service` tests cover the file.
- **State file `<gitdir>/browsitory/sequence.json`** (`repo.path()`, so it is per worktree like
  `undo.json`), written only when a sequence stops:

  ```json
  { "version": 1, "kind": "CherryPick", "commits": ["<oid>", "…"], "next": 1,
    "skipped": [], "originalHead": "<oid>", "expectedHead": "<oid>" }
  ```

  `commits` is in application order. `next` is the index of the stopped step.
  `expectedHead` is HEAD just before that step ran. The file is written to a temp file, then
  `sync_all` and `rename` (the `undo_store` pattern). It is deleted when the sequence completes
  or is aborted. A file that is unreadable, malformed, or has another `version` is logged and
  treated as absent, so the next sequence overwrites it. A corrupt file must never wedge the
  repo.
- **Stops:**
  - Conflict → write the file, return `Conflicted` (git state `CherryPick`/`Revert`, `MERGE_MSG`
    written, exactly like a single pick).
  - Any other error (checkout refused by an unstaged edit, signer failure) after at least one
    step landed or was skipped → write the file with `next` at the failing step and return the
    error. The strip then offers Continue (retry) and Abort.
  - An error on the very first step → no file; the repo is unchanged, as for a single pick.
- **`continue_sequence(repo, state)`:** the worker replies "No cherry-pick or revert is in
  progress." when `SequenceStore::load()` is `None`. Then a signing preflight, and:
  - git state CherryPick/Revert (paused on a conflict, or a signer failure after staging): any
    conflict left → `UnresolvedConflicts` ("Resolve all conflicts before continuing."). Otherwise
    finish the step: write the tree. If the tree equals HEAD's, it is empty → `cleanup_state`,
    add the id to `skipped`. Otherwise commit with `MERGE_MSG` (`repo.message()`, falling back to
    the original message), author = the picked commit's author (cherry-pick) or committer
    (revert), `HeadUpdate { expected: Some(expectedHead) }`, then `cleanup_state`.
  - git state Clean and HEAD == `expectedHead`: the step never ran (error stop) → retry it.
  - git state Clean and HEAD's first parent == `expectedHead`: the user committed the step some
    other way (for example the CLI) → count it as landed.
  - Anything else → `HeadMoved` ("HEAD moved since the cherry-pick paused. Abort it and start
    again.").
  - Then run the remaining steps with the same loop as `*_many`.
- **`abort_sequence(repo, &state) -> SequenceAbort { rewound: bool }`** (no stored state → the
  same worker reply as continue). The worker clears the file only on `Ok`. If git
  state is CherryPick/Revert, run `merge::abort_merge` (restores only touched paths and keeps
  unrelated edits). Then, if HEAD == `expectedHead` and != `originalHead`, check out
  `originalHead`'s tree with libgit2's default SAFE strategy (refuses rather than overwrite a
  local edit, like `start_merge`'s fast-forward ordering) and move the branch ref (or the
  detached HEAD) back with reflog message `"cherry-pick: abort"` / `"revert: abort"`. If the
  checkout is refused, return the error and keep the file, so Abort can be retried after the
  user moves the edit away. If HEAD moved elsewhere, do not move it; just delete the file
  (`rewound: false`, toast "Stopped the cherry-pick. HEAD had moved, so it was left where it
  is."). Delete the file last.
- **Gates.** `SequenceStore::load()` returning `Some` (the file exists and parses) makes the
  worker refuse `CherryPick`/`RevertCommit`/`CherryPickMany`/`RevertMany`/`StartMerge`/
  `StartRebase`/`AmendCommit` with the `OperationInProgress` text ("Finish or abort the current
  merge/cherry-pick/revert/rebase first."), before calling `git-core`. It also joins the worker's
  `in_progress`, so undo/redo refuse and the whole sequence is one pending undo entry. In the
  frontend, `headInfo.sequence !== null` joins `repositoryOperationDisabled` and
  `operationDisabledReason` ("A cherry-pick is in progress." / "A revert is in progress."), and
  hides the amend toggle.
- **`HeadInfo.sequence: Option<SequenceProgress>`** instead of a new `getSequenceProgress` call.
  The refresh already fetches `HeadInfo`, and a sequence is a HEAD-level operation like
  `operation`. `git_core::repo::head_info` always sets `None`, and the worker's `GetHeadInfo`
  handler fills it from `SequenceStore`.
- **Abort routing.** While a sequence is paused on a conflict, `operation` is also
  `CherryPick`/`Revert` and `MERGE_MSG` exists. Every Abort button (strip, commit dock) calls
  `abortSequence` when `headInfo.sequence !== null`, otherwise `abortMerge`. As a backstop, the
  worker's `AbortMerge` arm delegates to `abort_sequence` when a sequence file exists, so a stale
  frontend cannot strand `sequence.json`.
- **Undo.** `CherryPickMany` → "Cherry-pick N commits", `RevertMany` → "Revert N commits" (scope
  Full). `ContinueSequence` gets its reply held. `AbortSequence` gets its reply held and joins
  `aborts`, so an aborted sequence is dropped, not recorded. A completed sequence is one entry.
- **UI.**
  - Multi-select (context menu and toolbar) adds "Cherry-pick N commits" and "Revert N commits"
    after the four rebase items. They are disabled while `pending` or when the selection holds a
    merge commit. They do not depend on `planRebaseSelection` succeeding, so root commits are
    allowed.
  - `OperationStatusStrip` gains `sequence` and `onContinueSequence`/`onAbortSequence`. The label
    is "Cherry-picking 2/5" / "Reverting 2/5". A **Continue** button is disabled with the
    `conflictReason` text while conflicts remain. Abort is "Abort cherry-pick" / "Abort revert".
  - The commit dock shows `RebaseProgressPanel` for a sequence too, generalized with
    `title`/`continueLabel`/`abortLabel` props ("Cherry-pick in progress", "Continue
    cherry-pick", "Abort cherry-pick"; the rebase defaults are unchanged). Continue uses
    `MERGE_MSG` as is, like rebase continue. There is no editable message for a sequence step.
  - Palette: "Continue cherry-pick"/"Continue revert" and "Abort cherry-pick"/"Abort revert"
    while a sequence is active.
- **Result toasts come from the outcome.** Single pick/revert: `Committed` → "Cherry-picked" /
  "Reverted", `Conflicted` → no toast (the strip says it). Sequences: `Completed` →
  "Cherry-picked 3 commits" / "Reverted 3 commits", plus ", skipped 1 already applied" when
  `skipped` is non-empty. `Conflicted` → no toast. This also fixes the pre-existing false
  "Cherry-picked" toast on a conflicted pick (see "Pre-existing bugs").

### Image diff

- **`diff::file_sides(repo, target: SidesTarget, path, old_path, cap) -> FileSides { old, new }`**
  instead of the roadmap's `blob_contents(repo, oid)`. The frontend has no blob ids, and the
  unstaged "after" side is a working-tree file, not a blob. Each side is a
  `FileSide::{Missing, TooLarge { size }, Loaded { bytes }}`. Sides per target:
  - `Working { staged: false }`: index entry → working file.
  - `Working { staged: true }`: HEAD tree → index entry.
  - `Commit { commit_id }`: first parent's tree at `old_path.unwrap_or(path)` (empty tree for a
    root commit) → the commit's tree at `path`.
  - The crate-private `blob_contents(repo, oid, cap)` keeps the roadmap name. It checks
    `odb.read_header` size before reading.
- **Cap `MAX_PREVIEW_BYTES = 20 * 1024 * 1024` per side.** Over the cap → `TooLarge { size }`;
  the UI says "Too large to preview (24.1 MB)".
- **Path safety.** `path`/`old_path` must be relative without `..` components
  (`DiffError::InvalidPath`). A working-tree side that is a symlink (`symlink_metadata`) is
  `Missing`, so a preview never follows a link out of the repository.
- **Wire: base64 in JSON**, encoded by a hand-rolled `repo_service::base64::encode` (RFC 4648,
  ~25 lines, tested against the RFC vectors). That is no new dependency: `base64` is only
  transitive via tauri and would need a LICENSE row if made direct. Both Tauri and the sidecar
  use the same encoder. The frontend uses the string directly in a `data:` URL, so it never
  decodes.
- **Image detection by extension in the frontend** (`lib/imageTypes.ts`: `.png .jpg .jpeg .gif
  .webp .svg`, case-insensitive → MIME type). It needs no content sniffing. A wrong extension
  just fails to decode and shows "Can't display this image".
- **SVG is only ever rendered via `<img src="data:image/svg+xml;base64,…">`**, never inlined or
  put in `<object>`/`<iframe>`. Scripts inside an `<img>` SVG do not run. SVG is text, so its
  section gets a "Show text diff" / "Show image" toggle (default image). Raster formats skip the
  hunk fetch.
- **Dimensions come from the browser** (`naturalWidth`/`naturalHeight` in `onLoad`), with no
  header parser or crate. An SVG without intrinsic size reports what the browser reports.
- **Layout:** side-by-side "Before" / "After" columns. Each shows the image (max-width 100%,
  checkerboard background for transparency), "W × H px · 12.3 KB", or a placeholder: "Added" /
  "Deleted" for `Missing`, "Too large to preview (…)", or "Can't display this image" on
  `onError`. Swipe/onion-skin are out of scope.
- **VSCode CSP** gains `data:` in `img-src` (`webviewHtml.ts`), the same as the Tauri CSP. Data
  URLs carry no network access, so this does not widen what the webview can fetch.
- Conflicted images still go to `ConflictResolutionPane` (whole-file buttons now make binary
  conflicts resolvable).

### Commit message polish

- **`CommitMessageSettings { summary_limit: u32 = 50, body_ruler: u32 = 72, show_ruler: bool =
  true }`**, a new `[commit_message]` TOML table placed right after `[diff]` (tables must precede
  the array-of-tables fields). It is clamped to `10..=200` on write. It is exposed as
  `get/set_commit_message_settings` and a `CommitMessageSettingsProvider` mirroring
  `DiffSettingsProvider`.
- **Preferences.** A "Commit message" section in 4f's Preferences dialog: summary limit (number),
  ruler column (number), "Show ruler" (checkbox). If 4f's dialog is not on `main` at pre-flight,
  Part B creates a minimal `PreferencesDialog` (palette "Preferences…") with only this section,
  which 4f-style sections can join later.
- **Counter:** below the textarea, "Summary 42/50" (`aria-label="Summary length"`). Over the
  limit it gets the warning color and the title "Many tools cut summaries longer than 50
  characters.". It never blocks a commit. Characters are counted as code points of the first
  line (`[...line].length`).
- **Ruler:** when `show_ruler`, the textarea switches to `var(--font-mono)` and draws a 1px
  vertical line at `body_ruler` columns with a `background-image` gradient positioned at
  `calc(var(--ruler-column) * 1ch + padding)`, `background-attachment: local`. A column ruler is
  only meaningful in a monospace font; see Open questions.
- **Trailers in `git-core`:** `Trailer { key, value }` and `append_trailers(message, trailers)
  -> Result<String, CommitError>` with `git interpret-trailers` placement:
  - no trailers → the message unchanged (byte-identical);
  - each trailer validated: key `^[A-Za-z0-9][A-Za-z0-9-]*$`, value trimmed, non-empty, single
    line (`CommitError::InvalidTrailer`);
  - the message is trimmed of trailing whitespace; if it has at least two paragraphs and every
    line of the last paragraph looks like a trailer (`Key: value`), the new lines are appended to
    that block; otherwise a blank line and then the block are added;
  - exact duplicates of a line already in the block are skipped; the result ends with `\n`.
- **`commit(repo, message, trailers)`** and **`amend_head(repo, message, author:
  Option<&SignatureInput>, trailers)`**. `SignatureInput { name, email }` (new) is validated
  (non-empty, no `<`, `>` or newline → `CommitError::InvalidAuthor`). The override keeps the
  original author time and offset, as `git commit --amend --author` does. Committer is always
  `repo.signature()`.
- **Trailers are appended before hooks.** If 4e wired `prepare-commit-msg`/`commit-msg` into
  `commit`/`amend_head`, `append_trailers` runs first and the hooks see (and may rewrite) the
  full message, like `git commit --trailer`. 4e's `no_verify` parameter stays where 4e put it.
  The new parameters go after it.
- **`recent_authors(repo, limit) -> Vec<SignatureInput>`:** walks up to 1,000 commits from HEAD
  (`TOPOLOGICAL | TIME`), dedupes by lowercase email, excludes the current user's email, and
  returns the first `limit` (20) in recency order. An unborn HEAD → `[]`.
- **Co-authors in `CommitBox`:** an "Add co-author" button opens a small listbox popover (loaded
  lazily through `listRecentAuthors`, with a filter field). Picking one adds a removable chip
  "Name <email>" (× has the aria-label "Remove co-author Name"). On commit/amend each chip becomes
  `Co-authored-by: Name <email>`. Chips clear after a successful commit and are part of
  `CommitDraft` (`coAuthors`) so a remount keeps them. They are hidden during a merge/pick
  pause; the message there is git's.
- **Amend author:** in amend mode an "Author" text field appears, prefilled with
  `Name <email>` of HEAD from the graph's commits. If HEAD is not loaded in the graph, it is
  empty with the placeholder "Keep original author". Submitting it unchanged or empty sends
  `author: null`. Anything else must parse as `Name <email>`, or an inline error "Use the form
  Name <email>." blocks Amend.
- **`RepoClient`:** `commit(repoPath, message, trailers?)` and `amendCommit(repoPath, message,
  options?: { author, trailers })` take optional arguments, so the existing call sites are
  unchanged. The transports always send explicit `trailers: []` / `author: null`.

## Overlaps with other batches (4b–4f merge first)

| Batch | Overlap | Handling |
|---|---|---|
| 4c | `start_merge` gains `MergeOptions`; stash apply/pop can leave conflicts (`StashApplyOutcome::Conflicted`) resolved through the existing pane | Whole-file/Mark resolved operate on index conflicts only, so they work for stash conflicts with no stash-specific code. Pre-flight: check that 4c did not change `ConflictResolutionPane`'s props or `find_conflict`/`conflict_path`. Add a `git-core` test for whole-file on a stash conflict if 4c landed `pop_stash`. |
| 4d | Adds compare actions to `CommitGraph`'s multi-select (context menu and toolbar) | Append the pick/revert items after whatever 4d added. Do not reorder 4d's items. |
| 4e | Hooks in `commit`/`amend_head` (`no_verify` parameter, `CommitError::HookFailed`) | Trailers are appended before the hook pipeline. The author override and trailers are added after 4e's parameters. Sequence steps run no commit hooks (4e: "Rebase and cherry-pick do not run commit hooks"). |
| 4f | Preferences dialog; merge-tool "Mark resolved" | Commit message settings become a Preferences section. **If 4f already added `mark_resolved`, reuse it** (extend with `force` and the marker check, keep its wire name). If 4f added a `[tools]`/similar TOML table, place `[commit_message]` next to it, before the array-of-tables fields. |
| 4b | `graph.rs` tag badges | None beyond fixture churn in `GraphCommit` literals. |

## Backend API (`git-core`)

```rust
// merge.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictSide { Ours, Theirs }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkResolvedOutcome { Resolved, MarkersRemain }
pub fn resolve_whole_file(repo: &Repository, path: &str, side: ConflictSide) -> Result<(), MergeError>;
pub fn mark_resolved(repo: &Repository, path: &str, force: bool) -> Result<MarkResolvedOutcome, MergeError>;
pub(crate) fn has_conflict_markers(content: &[u8]) -> bool;
// MergeError += UnsupportedEntry(String)

// pick.rs
pub fn cherry_pick(repo: &Repository, commit_id: &str, mainline: Option<u32>) -> Result<PickOutcome, PickError>;
pub fn revert(repo: &Repository, commit_id: &str, mainline: Option<u32>) -> Result<PickOutcome, PickError>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceKind { CherryPick, Revert }
pub struct SequenceProgress { pub kind: SequenceKind, pub current_step: usize, pub total_steps: usize, pub current_commit: String }
pub enum SequenceOutcome { Completed { applied: usize, skipped: Vec<String> }, Conflicted { files: Vec<String>, progress: SequenceProgress } }
pub struct SequenceAbort { pub rewound: bool }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceState {
    pub kind: SequenceKind,
    pub commits: Vec<String>,   // application order
    pub next: usize,            // index of the stopped step
    pub skipped: Vec<String>,
    pub original_head: String,
    pub expected_head: String,
}
impl SequenceState { pub fn progress(&self) -> SequenceProgress; }
pub struct SequenceRun { pub outcome: Result<SequenceOutcome, PickError>, pub state: Option<SequenceState> }
pub fn cherry_pick_many(repo: &Repository, ids_oldest_first: &[String]) -> SequenceRun;
pub fn revert_many(repo: &Repository, ids_oldest_first: &[String]) -> SequenceRun;
pub fn continue_sequence(repo: &Repository, state: SequenceState) -> SequenceRun;
pub fn abort_sequence(repo: &Repository, state: &SequenceState) -> Result<SequenceAbort, PickError>;
// PickError: MergeCommitUnsupported → MainlineRequired; += InvalidMainline { mainline: u32 },
//   NothingSelected, UnresolvedConflicts, HeadMoved, Merge(#[from] MergeError)
// (NoSequence is a worker-level reply: "No cherry-pick or revert is in progress.")

// repo.rs
pub struct HeadInfo { /* existing */ pub sequence: Option<crate::pick::SequenceProgress> } // None from git-core; worker fills it

// repo-service/src/worker/sequence_store.rs
pub(super) struct SequenceStore { file: PathBuf }
impl SequenceStore {
    pub fn open(repo: &Repository) -> Self;               // <gitdir>/browsitory/sequence.json
    pub fn load(&self) -> Option<SequenceState>;          // unreadable/malformed/other version → None (logged)
    pub fn save(&self, state: &SequenceState) -> Result<(), String>; // temp + sync_all + rename
    pub fn clear(&self) -> Result<(), String>;            // missing file is fine
}

// diff.rs (4g-2)
pub const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;
pub enum SidesTarget<'a> { Working { staged: bool }, Commit { commit_id: &'a str } }
pub enum FileSide { Missing, TooLarge { size: u64 }, Loaded { bytes: Vec<u8> } }
pub struct FileSides { pub old: FileSide, pub new: FileSide }
pub fn file_sides(repo: &Repository, target: SidesTarget<'_>, path: &str, old_path: Option<&str>, cap: u64) -> Result<FileSides, DiffError>;
// DiffError += Io(#[from] std::io::Error), InvalidPath(String)

// commit.rs (4g-2)
pub struct SignatureInput { pub name: String, pub email: String }
pub struct Trailer { pub key: String, pub value: String }
pub fn append_trailers(message: &str, trailers: &[Trailer]) -> Result<String, CommitError>;
pub fn commit(repo: &mut Repository, message: &str, trailers: &[Trailer]) -> Result<String, CommitError>;
pub fn amend_head(repo: &Repository, message: &str, author: Option<&SignatureInput>, trailers: &[Trailer]) -> Result<String, CommitError>;
pub fn recent_authors(repo: &Repository, limit: usize) -> Result<Vec<SignatureInput>, CommitError>;
// CommitError += InvalidTrailer(String), InvalidAuthor(String)
```

## Wire (Tauri commands = sidecar methods, camelCase params)

| Method | Params | Result |
|---|---|---|
| `resolve_whole_file` | `{ repoPath, path, side: "Ours" \| "Theirs" }` | `null` |
| `mark_resolved` | `{ repoPath, path, force }` | `{ kind: "Resolved" \| "MarkersRemain" }` |
| `cherry_pick` / `revert_commit` | `{ repoPath, commitId, mainline: number \| null }` | `PickOutcome` (unchanged) |
| `cherry_pick_many` / `revert_many` | `{ repoPath, commitIds }` (oldest-first) | `SequenceOutcome` |
| `continue_sequence` | `{ repoPath }` | `SequenceOutcome` |
| `abort_sequence` | `{ repoPath }` | `{ rewound: boolean }` |
| `get_head_info` | unchanged | adds `sequence: { kind: "CherryPick" \| "Revert", currentStep, totalSteps, currentCommit } \| null` |
| `get_file_sides` | `{ repoPath, target: { kind: "Working", staged } \| { kind: "Commit", commitId }, path, oldPath }` | `{ old: FileSide, new: FileSide }`, `FileSide = { kind: "Missing" } \| { kind: "TooLarge", size } \| { kind: "Loaded", size, base64 }` |
| `get_commit_message_settings` / `set_commit_message_settings` | `{}` / `{ settings }` | `{ summaryLimit, bodyRuler, showRuler }` / `null` |
| `commit` | `{ repoPath, message, trailers: [{ key, value }] }` | unchanged |
| `amend_commit` | `{ repoPath, message, author: { name, email } \| null, trailers }` | unchanged |
| `list_recent_authors` | `{ repoPath }` | `[{ name, email }]` |

`SequenceOutcome = { kind: "Completed", applied, skipped: string[] } | { kind: "Conflicted",
files: string[], progress: SequenceProgress }`.

## Frontend

- `ConflictResolutionPane`: new props `onResolveWholeFile(path, side)` and
  `onMarkResolved(path, force): Promise<MarkResolvedOutcome | null>`. Header toolbar and the
  confirm dialog as above.
- `components/ParentPickerDialog.tsx` (new): `role="dialog"`, aria-label "Choose parent", radio
  group "Mainline parent".
- `CommitGraph`: merge items open the picker. New props `onCherryPickMany(idsOldestFirst)` and
  `onRevertMany(idsOldestFirst)`, and `onCherryPick(id, mainline)` / `onRevert(id, mainline)`
  gain `mainline`.
- `OperationStatusStrip` and `RebaseProgressPanel` as above. `lib/operationStatus.ts` gains
  `sequenceLabel(progress)` and `pickToast(...)`.
- `state/useHistoryActions.ts`: `cherryPick(id, mainline?)`, `revertCommit(id, mainline?)`,
  `cherryPickMany(ids)`, `revertMany(ids)`, `continueSequence()`, `abortSequence()`, each
  resolving to its outcome, or `null` on failure (the error stays in `state.error`).
  `state/useMergeRebaseActions.ts`: `resolveWholeFile`, `markResolved`.
- 4g-2: `components/ImageDiff.tsx`, `lib/imageTypes.ts`, `state/useCommitMessageSettings.tsx` +
  provider, a Preferences section, and `CommitBox` counter/ruler/co-authors/author.

## Testing

- `git-core` `tests/merge.rs`: whole-file ours and theirs on a content conflict (content,
  conflict cleared, exec bit kept on Unix), on a modify/delete conflict (theirs deleted → file
  removed), symlink conflict refused (Unix); mark resolved refuses with markers and stages with
  `force`, stages a cleaned file, resolves a deleted file, `=======` alone is not a marker, binary
  content never counts. If 4c's `pop_stash` exists: whole-file on a stash conflict.
- `git-core` `tests/pick.rs`: `MainlineRequired` for a merge without mainline; mainline 1
  cherry-pick applies the merged side; mainline revert of a merge restores the first parent's
  tree and uses git's message; `InvalidMainline`; sequence of three clean picks; conflict in the
  middle → file written, progress 2/3, `operation == CherryPick`; continue after resolving lands
  the rest and deletes the file; continue with conflicts left → `UnresolvedConflicts`;
  whole-file-ours makes the step empty → skipped; abort restores the original HEAD, keeps an
  unrelated unstaged edit, and deletes the file; abort after HEAD moved does not rewind;
  `revert_many` applies newest-first; an already-applied commit is skipped; a merge in a sequence
  is refused up front; an error on the first step returns `state: None`; an error on a later step
  returns the state and continue retries it.
- `repo-service` `sequence_store`: save/load round trip, a corrupt or other-version file is
  `None`, clear on a missing file is fine, the file lives under the worktree's own gitdir.
- `repo-service` worker: a single pick while a sequence is paused → the `OperationInProgress`
  text; `get_head_info` reports `sequence` while paused and `None` after continue; a conflicted
  sequence finished by continue is one undo entry "Cherry-pick 3 commits"; an aborted sequence
  records nothing; undo refuses while a sequence is paused; `AbortMerge` during a sequence clears
  the file; held replies (`assert_held!`) for the new commands; base64 RFC 4648 vectors (4g-2).
- `tauri-app` DTO tests: `SequenceOutcomeDto`, `HeadInfoDto.sequence`, `MarkResolvedOutcomeDto`,
  `FileSidesDto` (base64, `TooLarge` size), `CommitMessageSettingsDto`, trailer/author input
  deserialization.
- `vscode-sidecar` round-trips: `cherry_pick_many` + `continue_sequence`, `get_file_sides`,
  `commit` with trailers.
- `git-core` 4g-2: `file_sides` for each target, a missing side, the cap (`TooLarge`, cap set
  small in the test), an invalid path, a symlink working side (Unix); `append_trailers` table
  tests; commit with trailers; amend with an author override keeps the committer and the
  original author time; invalid author/trailer; `recent_authors` dedupe/exclude-self/limit.
- `config`: commit message settings round trip and clamping.
- Frontend (mock `RepoClient`): both clients' wiring for every new method; the pane header
  buttons and the markers confirm; the parent picker; multi-select items and their disabled
  states; strip and progress panel labels; Continue disabled with conflicts; abort routing; pick
  toasts by outcome; `ImageDiff` (both sides, added, deleted, too large, decode error, SVG via
  `<img>` only, dimensions after load); counter thresholds; ruler style; co-author chips →
  trailers; amend author parse/validation; Preferences section persistence.
- E2E Tauri (`e2e/specs/cherry-pick-sequence.spec.ts`, 4g-1): ctrl-select three commits on a
  side branch, "Cherry-pick 3 commits", the strip shows "Cherry-picking 2/3", open the conflicted
  file, "Use ours (whole file)", Continue; three new commits on the base branch, the shared file
  keeps the base content, `sequence.json` is gone.
- No E2E for 4g-2: image rendering and textarea styling are covered by Vitest. A WebKitGTK
  screenshot assertion would be flaky and adds nothing over the component tests. No VSCode E2E
  (the roadmap asks for one Tauri flow per batch).

## Docs

- 4g-1: `docs/USER_GUIDE.md` "Rewriting and moving commits" (merge commits with a parent,
  several commits, Continue/Abort, skipped commits) and a new "Resolving conflicts" subsection
  under "Staging and committing" (whole file, Mark resolved). `CHANGELOG.md` Unreleased "Conflicts
  and picks". `docs/ARCHITECTURE.md` "Cherry-pick sequences" (file format, stop rules, undo
  gating).
- 4g-2: USER_GUIDE "Browsing history and diffs" (image preview) and "Staging and committing"
  (counter, ruler, co-authors, amend author, Preferences). CHANGELOG "Image diff and commit
  message polish".
- No `docs/LICENSE_COMPLIANCE.md` change (no new dependency in either half).

## Pre-existing bugs found while planning

- **False success toast on a conflicted cherry-pick/revert.** `App.tsx` wraps `onCherryPick`/
  `onRevert` in `announce("Cherry-picked" | "Reverted", …)`, and `useHistoryActions.cherryPick`
  discards the `PickOutcome` (`void (await client.cherryPick(...))`). A `Conflicted` outcome is not
  an error, so the toast says "Cherry-picked" while the strip says "Cherry-picking" with
  conflicts. Repro: cherry-pick a commit that conflicts with HEAD from the graph menu. Fixed in
  4g-1 (outcome-driven toasts).
- **Roadmap inaccuracy, not a code bug:** the paused Browsitory rebase is not persisted
  (`rebase_state` is an in-memory `Option` in the worker thread), so it is lost on restart, and
  "like rebase" in the roadmap has no file to copy. Out of scope here. Noted for a later batch.

## Out of scope

Image swipe/onion views, PDF preview, trailer config (`trailer.*`), mainline for multi-commit
sequences, an editable message per sequence step, `--skip` as a separate action (empty steps are
skipped automatically), persisting Browsitory's rebase state, a VSCode E2E flow.

## Open questions (current choice in brackets)

1. Monospace commit message field when the ruler is on [yes; ruler on by default].
2. Empty sequence steps [skip automatically and report]; git stops instead.
3. Co-author entry [chips from recent authors]; alternative: insert the trailer text into the
   message directly.
