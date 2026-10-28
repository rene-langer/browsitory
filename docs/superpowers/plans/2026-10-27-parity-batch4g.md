# Feature Parity Batch 4g (Conflicts, Picks and Polish) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Whole-file conflict resolution and Mark resolved; cherry-pick/revert of several commits as one Continue/Abort-able sequence and of merge commits with a mainline parent; an image preview for image diffs; and commit message polish (summary counter, body ruler, co-author trailers, amend author). Both the Tauri app and the VSCode extension.

**Architecture:** Two PRs from one plan. **Part A (4g-1, conflicts and picks):** `git-core::merge` gains `resolve_whole_file`/`mark_resolved`, and `git-core::pick` gains mainline picks plus a pure sequence state machine (`SequenceState` in, `SequenceRun { outcome, state }` out). `repo-service` persists the state as `<gitdir>/browsitory/sequence.json` (`sequence_store.rs`, modeled on `undo_store.rs`), gates other operations on it, and folds it into undo's "in progress". It also fills `HeadInfo.sequence`. Tauri and the sidecar expose the new methods, `RepoClient` gains them, and the frontend adds the conflict pane header, multi-select actions, a parent picker, and Continue/Abort in the status strip. **Part B (4g-2, image diff and commit polish):** `git-core::diff::file_sides` returns both sides' bytes (capped at 20 MB), base64-encoded by a hand-rolled encoder in `repo-service` and shown through `data:` URLs. `git-core::commit` gains `append_trailers`, an author override on amend, and `recent_authors`. The `config` crate gains `[commit_message]` settings, and `CommitBox` gains a counter, ruler, co-author chips and an author field.

**Tech Stack:** Rust (`git2` 0.21, `thiserror`; `serde`/`serde_json` in repo-service/tauri-app/sidecar/config), React + TypeScript, Vitest + Testing Library, WebdriverIO (tauri-driver).

**Spec:** `docs/superpowers/specs/2026-10-27-parity-batch4g-design.md`

## Pre-flight notes (Part A implementation, 2026-10-28)

Part A landed on `feat/sm-parity-batch4g-1` (not `feat/parity-batch4g-1-conflicts-picks`), on
top of 4a–4f. Drift found and deviations taken (the code wins where the plan disagreed):

- **`mark_resolved` already existed (4f)** with `force`, `MarkResolvedOutcome`, a stricter
  `has_conflict_markers` (marker followed by a space or line end), Tauri/sidecar/`RepoClient`
  methods and a confirm dialog in `DiffPane`'s file section. Reused as is: Task A1 only added
  `ConflictSide`, `resolve_whole_file` and `MergeError::UnsupportedEntry`. The wire result
  stays 4f's plain string (`"Resolved" | "MarkersRemain"`), not the plan's `{ kind }` object,
  and the markers confirm keeps 4f's copy ("Conflict markers remain in {path}. Mark it resolved
  anyway?" / **Mark resolved**): the pane header's **Mark resolved** calls the section's existing
  flow, so the merge-tool notice and the pane share one dialog. `ConflictResolutionPane` takes
  `onMarkResolved: () => void` and both new props are optional.
- **4c changed `start_merge`** (`MergeOptions`): fixtures call it with
  `MergeOptions::default()`; 4c's `pop_stash` exists, so the stash-pop whole-file test was added.
- **4e hooks.** Decision: sequence steps, including the step **Continue** commits, run no
  hooks, matching 4e's rule for single cherry-pick/revert (and the spec's overlap table). Real
  git (checked with git 2.53) runs `prepare-commit-msg` + `post-commit` on a clean
  `git cherry-pick` and `pre-commit`, `prepare-commit-msg`, `commit-msg`, `post-commit` on
  `git cherry-pick --continue`. Running hooks on Continue would need 4e's hook progress/cancel
  plumbing and a "Continue without hooks" retry; left as a follow-up. A single conflicted pick
  finished through the commit box still runs commit hooks (4e).
- **Continue's message** is `MERGE_MSG` cleaned with `git2::message_prettify(_, '#')`, because
  libgit2 appends `#Conflicts:` comment lines that git's commit cleanup would strip.
- **Validation of the stored state against the repository** (beyond the plan): `load` also
  rejects ids that are not commits of this repository and `skipped` ids outside `commits`, and
  logs a bad file once (not on every command). At use time, Continue refuses an unrelated paused
  pick (`CHERRY_PICK_HEAD`/`REVERT_HEAD` not the sequence's current commit, or the other kind)
  with `OperationInProgress`, and refuses staged changes before retrying a step
  (`DirtyIndex`). Abort aborts only its own paused step, never rewinds while another operation is
  paused, and otherwise forgets the sequence with `rewound: false`. A CLI `git cherry-pick
  --abort` leaves the sequence stored; Continue then re-runs that step.
- **An error after a step was staged** (a signer failure leaves `CHERRY_PICK_HEAD`) keeps the
  state even on the first step, so Continue can finish it; `state: None` on error only when the
  repository is untouched.
- **Worker gates** are per-arm `history::refused_during_sequence(&repo, &store, &reply)` checks
  rather than a pre-dispatch helper. `persist` reports a failed save on a conflict stop as an
  error explaining how to finish by hand.
- **Undo:** `ContinueSequence`/`AbortSequence` are not recordable, so `assert_held!` (which
  requires a plan) can't cover them; a separate `ending_commands_have_their_reply_held` test
  covers them (and `AbortMerge`/`AbortRebase`/`RebaseContinue`).
- **4d graph:** the pick items come after the rebase items and 4d's "Compare these commits".
  They are only rendered when the new `onCherryPickMany`/`onRevertMany` props are passed.
- **Frontend toolbar role:** `Toolbar` renders `role="group"`, so the pane header is
  `group "Whole file"`.
- **Tab close (A10):** the plan asked to treat a sequence like a rebase in the unmount busy
  guard. Not done: the sequence lives on disk (that is the point), so a closed tab loses
  nothing; it still joins `repositoryOperationDisabled` (and so blocks closing the active tab,
  like a merge).
- **Palette:** while a sequence is stored, "Abort merge" is replaced by "Abort cherry-pick" /
  "Abort revert". Palette Continue/Abort don't toast (as rebase continue doesn't).
- **#108 (rebase lost on restart), reusable pieces:** `SequenceStore` (atomic versioned JSON
  under `<gitdir>/browsitory/`, validate-on-load, log-once) and the "stored state counts as
  in progress / gates the worker / `HeadInfo` field" pattern would carry over to a persisted
  `RebaseState` almost unchanged.

## Global Constraints

- No new Rust or npm dependency in either part; `docs/LICENSE_COMPLIANCE.md` is untouched. `git-core` stays serde-free.
- Branches: Part A on `feat/parity-batch4g-1-conflicts-picks`, Part B on `feat/parity-batch4g-2-image-commit-polish` (branched from `main` after Part A merged). One PR each.
- Sequence ids are **oldest-first** on the wire and in `git-core`. Cherry-pick applies them in that order, revert applies them reversed.
- Sequence file: `<repo.path()>/browsitory/sequence.json`, `{"version":1,"kind":"CherryPick"|"Revert","commits":[…],"next":N,"skipped":[…],"originalHead":"…","expectedHead":"…"}`, written temp + `sync_all` + `rename`. An unreadable file, malformed JSON or another version → treated as absent (logged with `eprintln!("browsitory: ignoring sequence state: …")`).
- Preview cap: `git_core::diff::MAX_PREVIEW_BYTES = 20 * 1024 * 1024` per side.
- Commit message settings defaults `summaryLimit 50`, `bodyRuler 72`, `showRuler true`, clamped to `10..=200` on write.
- Error copy, exactly:
  - `This is a merge commit. Choose which parent to apply it against.` (`PickError::MainlineRequired`)
  - `Parent {mainline} doesn't exist on this commit.` (`PickError::InvalidMainline`)
  - `Select at least one commit.` (`PickError::NothingSelected`)
  - `Resolve all conflicts before continuing.` (`PickError::UnresolvedConflicts`)
  - `HEAD moved since the cherry-pick paused. Abort it and start again.` (`PickError::HeadMoved`)
  - `No cherry-pick or revert is in progress.` (worker reply when no sequence is stored)
  - `Finish or abort the current merge/cherry-pick/revert/rebase first.` (existing `OperationInProgress` text, reused by the worker gate)
  - `'{path}' is a symlink or submodule conflict. Resolve it with git.` (`MergeError::UnsupportedEntry`)
  - `Invalid trailer "{key}: {value}".` (`CommitError::InvalidTrailer`)
  - `Invalid author. Use a name and an email address.` (`CommitError::InvalidAuthor`)
  - `path must be relative to the repository: {path}` (`DiffError::InvalidPath`)
- UI copy, exactly:
  - Conflict pane: `Use ours (whole file)`, `Use theirs (whole file)`, `Mark resolved`; titles `Replace the file with the current branch's version (HEAD).` / `Replace the file with the incoming version.`; confirm `{path} still contains conflict markers. Mark it resolved anyway?` with buttons `Mark resolved anyway` / `Cancel`.
  - Graph: `Cherry-pick…` / `Revert…` on merge commits; multi `Cherry-pick N commits`, `Revert N commits`; disabled titles `Cherry-pick merge commits one at a time.` / `Revert merge commits one at a time.`
  - Parent picker: dialog aria-label `Choose parent`, radiogroup aria-label `Mainline parent`, rows `Parent N`, hint `Parent 1 is usually the branch that was merged into.`, buttons `Cherry-pick` / `Revert` / `Cancel`.
  - Strip: `Cherry-picking 2/5`, `Reverting 2/5`, buttons `Continue`, `Abort cherry-pick` / `Abort revert`.
  - Progress panel: titles `Cherry-pick in progress` / `Revert in progress`, buttons `Continue cherry-pick` / `Continue revert`, `Abort cherry-pick` / `Abort revert`.
  - Disabled reason: `A cherry-pick is in progress.` / `A revert is in progress.`
  - Palette: `Continue cherry-pick`, `Continue revert`, `Abort cherry-pick`, `Abort revert`.
  - Toasts: `Cherry-picked`, `Reverted`, `Cherry-picked N commits`, `Reverted N commits`, suffix `, skipped M already applied`; abort without rewind `Stopped the cherry-pick. HEAD had moved, so it was left where it is.` (`revert` variant likewise).
  - Image diff: column headings `Before` / `After`, `W × H px · 12.3 KB`, `Added`, `Deleted`, `Too large to preview (24.1 MB)`, `Can't display this image`, toggle `Show text diff` / `Show image`; images `alt="Before: {path}"` / `alt="After: {path}"`.
  - CommitBox: counter `Summary N/L` (aria-label `Summary length`), over-limit title `Many tools cut summaries longer than L characters.`; `Add co-author` button, listbox aria-label `Recent authors`, filter aria-label `Filter authors`, chip remove aria-label `Remove co-author {name}`; author field label `Author`, placeholder `Keep original author`, error `Use the form Name <email>.`
  - Preferences section heading `Commit message`, fields `Summary length limit`, `Ruler column`, `Show ruler`.
- Tests: `git-core`/`repo-service` use real temp-dir repos; frontend mocks `RepoClient` only (never `@tauri-apps/api` or `postMessage`).
- Every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook). Each part's changelog lands in its docs task, so push only after it.

## Review Focus

1. **A sequence must never strand the repo.** A corrupt `sequence.json` is ignored (Task A5 `a_corrupt_or_unknown_state_file_is_ignored`). Abort works after a conflict, after an error stop, and after HEAD moved (Task A4 `abort_*`). `AbortMerge` from a stale UI clears the file (Task A5 `abort_merge_during_a_sequence_clears_it`).
2. **Undo sees one operation.** A conflicted sequence finished by Continue is exactly one entry "Cherry-pick 3 commits", and an aborted one records nothing (Task A5).
3. **Whole-file "ours" that empties a step** is skipped, not committed empty, and not an error (Task A4 `whole_file_ours_empties_the_step_and_it_is_skipped`; the E2E deliberately makes that step non-empty).
4. **Mark resolved never stages conflict markers by accident** (Task A1 `mark_resolved_refuses_while_markers_remain_unless_forced`), and a Markdown `=======` underline is not a marker (`has_conflict_markers` unit test).
5. **SVG is only rendered through `<img>`** (Task B7 `renders SVG only through an img element`), and a preview never reads outside the repository (Task B1 `file_sides_rejects_paths_outside_the_repository`, `a_symlinked_working_file_is_missing`).
6. **Trailers and hooks (4e).** Trailers are appended before any hook pipeline (Task B2 Step 3 note), and an empty trailer list leaves the message byte-identical (`no_trailers_leaves_the_message_unchanged`).

---

## File Structure

| File | Part | Responsibility |
|---|---|---|
| `crates/git-core/src/merge.rs` (+ `tests/merge.rs`) | A | `ConflictSide`, `MarkResolvedOutcome`, `resolve_whole_file`, `mark_resolved`, `has_conflict_markers`, `MergeError::UnsupportedEntry` |
| `crates/git-core/src/pick.rs` (+ `tests/pick.rs`, new `tests/pick_sequence.rs`) | A | mainline, `SequenceKind/Progress/Outcome/State/Run/Abort`, `cherry_pick_many`, `revert_many`, `continue_sequence`, `abort_sequence` |
| `crates/git-core/src/repo.rs` (+ `tests/head_info.rs`) | A | `HeadInfo.sequence` (always `None` from git-core) |
| `crates/repo-service/src/worker/sequence_store.rs` (new) | A | load/save/clear `sequence.json` |
| `crates/repo-service/src/worker/history.rs`, `merge.rs`, `mod.rs`, `undo.rs` | A | commands, gates, `in_progress`, undo labels/holds/aborts, worker tests |
| `crates/tauri-app/src/commands/{history,merge,mod}.rs`, `main.rs` | A, B | commands + DTOs + DTO tests |
| `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs` | A, B | methods + DTOs + round-trips |
| `frontend/src/ipc/{RepoClient,tauriRepoClient,vscodeRepoClient}.ts` (+ tests, fixtures) | A, B | new types and methods |
| `frontend/src/components/ConflictResolutionPane.tsx` (+ test) | A | header toolbar, markers confirm |
| `frontend/src/components/ParentPickerDialog.tsx`, `.module.css`, `.test.tsx` (new) | A | mainline picker |
| `frontend/src/components/CommitGraph.tsx` (+ test) | A | merge items, multi pick/revert items |
| `frontend/src/components/OperationStatusStrip.tsx`, `RebaseProgressPanel.tsx` (+ tests) | A | sequence label, Continue, generalized panel |
| `frontend/src/lib/operationStatus.ts` (+ test) | A | `sequenceLabel`, `pickToast`, `sequenceToast` |
| `frontend/src/state/useHistoryActions.ts`, `useMergeRebaseActions.ts`, `useAppState.ts` (+ tests) | A, B | actions returning outcomes |
| `frontend/src/App.tsx`, `components/DiffPane.tsx`, `lib/commands.ts` (+ tests) | A, B | wiring, gates, palette |
| `e2e/specs/cherry-pick-sequence.spec.ts` (new) | A | E2E |
| `crates/git-core/src/diff.rs` (+ `tests/diff.rs`) | B | `file_sides`, `blob_contents`, `MAX_PREVIEW_BYTES` |
| `crates/git-core/src/commit.rs` (+ `tests/amend.rs`, `tests/stage_commit.rs`, new `tests/recent_authors.rs`) | B | `Trailer`, `SignatureInput`, `append_trailers`, `commit`/`amend_head` signatures, `recent_authors` |
| `crates/config/src/lib.rs` (+ new `tests/commit_message_settings.rs`) | B | `CommitMessageSettings` |
| `crates/repo-service/src/base64.rs` (new), `lib.rs` | B | RFC 4648 encoder |
| `extension/src/webviewHtml.ts` (+ test) | B | `img-src … data:` |
| `frontend/src/lib/imageTypes.ts`, `components/ImageDiff.tsx`, `.module.css`, `.test.tsx` (new) | B | image preview |
| `frontend/src/state/useCommitMessageSettings.tsx`, `CommitMessageSettingsProvider.tsx` (new) | B | settings context |
| `frontend/src/components/CommitBox.tsx`, `.module.css` (+ test) | B | counter, ruler, co-authors, author |
| 4f's Preferences dialog component (+ test) | B | "Commit message" section |
| `CHANGELOG.md`, `docs/USER_GUIDE.md`, `docs/ARCHITECTURE.md` | A, B | docs |

---

# Part A — 4g-1: Conflicts and picks

### Task A0: Pre-flight

**Files:** none modified unless a check fails.

- [ ] **Step 1: Branch from an up-to-date main**

```bash
git fetch origin
git checkout -b feat/parity-batch4g-1-conflicts-picks origin/main
git log --oneline -8   # 4b, 4c, 4d, 4e, 4f must all be merged; stop and ask if any is missing
```

- [ ] **Step 2: Re-check every name this part relies on**

Run each command and compare with what the plan assumes. Where the code differs, adapt the affected task's steps (names, argument positions) before you start it, and note the change in the PR description.

```bash
graft skeleton crates/git-core/src/merge.rs     # find_conflict, conflict_path (pub(crate)), resolve_conflict, resolve_add_delete_conflict, abort_merge, FileConflictChoice
graft skeleton crates/git-core/src/pick.rs      # cherry_pick(repo, id), revert(repo, id), apply, PickError::MergeCommitUnsupported, revert_message
grep -n "pub fn start_merge" crates/git-core/src/merge.rs   # 4c: MergeOptions parameter?
grep -rn "mark_resolved\|markResolved" crates frontend/src extension/src   # 4f: already present?
grep -n "pub struct HeadInfo" -A6 crates/git-core/src/repo.rs
grep -n "in_progress\|let aborts" crates/repo-service/src/worker/mod.rs
grep -n "fn plan_for\|fn hold_reply" crates/repo-service/src/worker/undo.rs
grep -n "multiSelectItems" -A40 frontend/src/components/CommitGraph.tsx   # 4d's compare items
grep -n "OperationStatusStrip" -A10 frontend/src/App.tsx
grep -n "props\|onResolve" frontend/src/components/ConflictResolutionPane.tsx | head
```

- **If 4f already added `mark_resolved`:** reuse it. In Task A1, extend its signature with `force: bool` and the marker check (return `MarkResolvedOutcome`), and keep its wire method name and frontend action name. Update 4f's call sites to pass `force: true` only where 4f's flow already asked the user. Skip the parts of Tasks A6/A7 that would add a duplicate method.
- **If 4c changed `start_merge`'s signature:** the test fixtures in this plan avoid `start_merge` except the existing `make_conflicted_repo`/`make_delete_modify_conflict` helpers in `tests/merge.rs`, which 4c will already have updated.
- **If 4c landed `pop_stash`:** add the extra test in Task A1 Step 1 (marked "if 4c landed").

- [ ] **Step 3: Baseline is green**

```bash
cargo test --workspace && cd frontend && pnpm test -- --run && cd ..
```
Expected: PASS. Fix nothing here. If main is red, stop and report.

---

### Task A1: `git-core::merge` — `resolve_whole_file` and `mark_resolved`

**Files:**
- Modify: `crates/git-core/src/merge.rs`
- Test: `crates/git-core/tests/merge.rs`

**Interfaces:**
- Produces: `ConflictSide { Ours, Theirs }`, `MarkResolvedOutcome { Resolved, MarkersRemain }`, `resolve_whole_file(repo, path, side) -> Result<(), MergeError>`, `mark_resolved(repo, path, force) -> Result<MarkResolvedOutcome, MergeError>`, `pub(crate) fn has_conflict_markers(&[u8]) -> bool`, `MergeError::UnsupportedEntry(String)`.

- [ ] **Step 1: Write the failing tests**

Inline in `merge.rs` (new `#[cfg(test)] mod tests` at the bottom, or the existing one if 4c/4f added it):

```rust
#[test]
fn conflict_markers_need_an_opening_and_a_later_closing_line() {
    assert!(has_conflict_markers(
        b"a\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> feature\n"
    ));
    assert!(has_conflict_markers(b"<<<<<<<\nx\n>>>>>>>"));
    assert!(!has_conflict_markers(b"Title\n=======\n"));
    assert!(!has_conflict_markers(b"<<<<<<< only an opener\n"));
    assert!(!has_conflict_markers(b">>>>>>> closer first\n<<<<<<< then opener\n"));
    assert!(!has_conflict_markers(b"bin\0ary\n<<<<<<<\n>>>>>>>\n"));
    assert!(!has_conflict_markers(b"  <<<<<<< indented\n  >>>>>>> indented\n"));
}
```

`tests/merge.rs` (reuse `make_conflicted_repo`, `make_delete_modify_conflict`; add `use git_core::merge::{ConflictSide, MarkResolvedOutcome, MergeError};` and `use std::path::Path;`):

```rust
fn staged_blob(repo: &git2::Repository, path: &str) -> Option<Vec<u8>> {
    let index = repo.index().unwrap();
    index
        .get_path(Path::new(path), 0)
        .map(|entry| repo.find_blob(entry.id).unwrap().content().to_vec())
}

#[test]
fn resolve_whole_file_takes_ours_or_theirs_for_a_content_conflict() {
    for (side, expected) in [
        (ConflictSide::Ours, "line one\nmain two\nline three\n"),
        (ConflictSide::Theirs, "line one\nfeature two\nline three\n"),
    ] {
        let (dir, repo) = make_conflicted_repo();
        git_core::merge::resolve_whole_file(&repo, "shared.txt", side).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
            expected,
            "{side:?}"
        );
        assert!(!repo.index().unwrap().has_conflicts());
        assert_eq!(staged_blob(&repo, "shared.txt").unwrap(), expected.as_bytes());
    }
}

#[test]
fn resolve_whole_file_on_a_modify_delete_conflict_deletes_or_keeps() {
    // ours (main) deleted shared.txt, theirs (feature) changed it to "v2\n".
    let (dir, repo) = make_delete_modify_conflict();
    git_core::merge::resolve_whole_file(&repo, "shared.txt", ConflictSide::Ours).unwrap();
    assert!(!dir.path().join("shared.txt").exists());
    assert!(staged_blob(&repo, "shared.txt").is_none());
    assert!(!repo.index().unwrap().has_conflicts());

    let (dir, repo) = make_delete_modify_conflict();
    git_core::merge::resolve_whole_file(&repo, "shared.txt", ConflictSide::Theirs).unwrap();
    assert_eq!(std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(), "v2\n");
    assert_eq!(staged_blob(&repo, "shared.txt").unwrap(), b"v2\n");
}

#[test]
fn resolve_whole_file_rejects_a_path_with_no_conflict() {
    let (_dir, repo) = make_conflicted_repo();
    assert!(matches!(
        git_core::merge::resolve_whole_file(&repo, "nope.txt", ConflictSide::Ours),
        Err(MergeError::NoConflict(_))
    ));
}

#[cfg(unix)]
#[test]
fn resolve_whole_file_keeps_the_chosen_sides_executable_bit() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = init_repo();
    let script = dir.path().join("run.sh");
    write_file(dir.path(), "run.sh", "echo base\n");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    commit_all(&repo, "base");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "run.sh", "echo feature\n");
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "run.sh", "echo main\n");
    commit_all(&repo, "main");
    // Same conflict without going through start_merge's (4c-dependent) signature:
    let theirs = repo.find_branch("feature", git2::BranchType::Local).unwrap();
    let annotated = repo.reference_to_annotated_commit(theirs.get()).unwrap();
    repo.merge(&[&annotated], None, None).unwrap();

    git_core::merge::resolve_whole_file(&repo, "run.sh", ConflictSide::Theirs).unwrap();
    assert_eq!(std::fs::read_to_string(&script).unwrap(), "echo feature\n");
    assert_eq!(std::fs::metadata(&script).unwrap().permissions().mode() & 0o111, 0o111);
    let index = repo.index().unwrap();
    assert_eq!(index.get_path(Path::new("run.sh"), 0).unwrap().mode, 0o100755);
}

#[cfg(unix)]
#[test]
fn resolve_whole_file_refuses_a_symlink_conflict() {
    let (dir, repo) = init_repo();
    std::os::unix::fs::symlink("a", dir.path().join("link")).unwrap();
    commit_all(&repo, "base");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0].name.clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    std::fs::remove_file(dir.path().join("link")).unwrap();
    std::os::unix::fs::symlink("b", dir.path().join("link")).unwrap();
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    std::fs::remove_file(dir.path().join("link")).unwrap();
    std::os::unix::fs::symlink("c", dir.path().join("link")).unwrap();
    commit_all(&repo, "main");
    let theirs = repo.find_branch("feature", git2::BranchType::Local).unwrap();
    let annotated = repo.reference_to_annotated_commit(theirs.get()).unwrap();
    repo.merge(&[&annotated], None, None).unwrap();
    assert!(repo.index().unwrap().has_conflicts(), "fixture must conflict");

    assert!(matches!(
        git_core::merge::resolve_whole_file(&repo, "link", ConflictSide::Ours),
        Err(MergeError::UnsupportedEntry(path)) if path == "link"
    ));
    assert!(repo.index().unwrap().has_conflicts());
}

#[test]
fn mark_resolved_refuses_while_markers_remain_unless_forced() {
    let (dir, repo) = make_conflicted_repo();
    // libgit2's merge checkout writes the conflict markers into the working file.
    let on_disk = std::fs::read_to_string(dir.path().join("shared.txt")).unwrap();
    assert!(on_disk.contains("<<<<<<<"), "fixture must leave markers: {on_disk}");

    assert_eq!(
        git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap(),
        MarkResolvedOutcome::MarkersRemain
    );
    assert!(repo.index().unwrap().has_conflicts());

    assert_eq!(
        git_core::merge::mark_resolved(&repo, "shared.txt", true).unwrap(),
        MarkResolvedOutcome::Resolved
    );
    assert!(!repo.index().unwrap().has_conflicts());
    assert_eq!(staged_blob(&repo, "shared.txt").unwrap(), on_disk.as_bytes());
}

#[test]
fn mark_resolved_stages_a_hand_edited_file() {
    let (dir, repo) = make_conflicted_repo();
    write_file(dir.path(), "shared.txt", "line one\nhand two\nline three\n");
    assert_eq!(
        git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap(),
        MarkResolvedOutcome::Resolved
    );
    assert_eq!(staged_blob(&repo, "shared.txt").unwrap(), b"line one\nhand two\nline three\n");
}

#[test]
fn mark_resolved_resolves_a_deleted_file_as_deleted() {
    let (dir, repo) = make_delete_modify_conflict();
    let _ = std::fs::remove_file(dir.path().join("shared.txt"));
    assert_eq!(
        git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap(),
        MarkResolvedOutcome::Resolved
    );
    assert!(staged_blob(&repo, "shared.txt").is_none());
    assert!(!repo.index().unwrap().has_conflicts());
}

#[test]
fn mark_resolved_rejects_a_path_with_no_conflict() {
    let (_dir, repo) = make_conflicted_repo();
    assert!(matches!(
        git_core::merge::mark_resolved(&repo, "nope.txt", false),
        Err(MergeError::NoConflict(_))
    ));
}
```

If 4c landed `pop_stash`, also add `resolve_whole_file_resolves_a_stash_pop_conflict`: commit `a.txt = "base\n"`, edit it to `"stashed\n"`, stash (with 4c's `save_stash`), commit `"committed\n"`, pop (conflicted), `resolve_whole_file(.., Theirs)` → file is `"stashed\n"`, no conflicts.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test merge && cargo test -p git-core --lib merge::tests`
Expected: FAIL to compile (`ConflictSide`, `resolve_whole_file`, `mark_resolved`, `has_conflict_markers` missing).

- [ ] **Step 3: Implement**

In `merge.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictSide { Ours, Theirs }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkResolvedOutcome { Resolved, MarkersRemain }
```

`MergeError::UnsupportedEntry(String)` with `#[error("'{0}' is a symlink or submodule conflict. Resolve it with git.")]`.

`resolve_whole_file`:
1. `let conflict = find_conflict(repo, path)?;`
2. If any present side (`ancestor`/`our`/`their`) has `mode == 0o120000 || mode == 0o160000` → `UnsupportedEntry(path)`.
3. `let workdir = repo.workdir().ok_or(MergeError::NoWorkdir)?;`
4. Chosen entry: `our` for `Ours`, `their` for `Theirs`.
   - `Some(_)`: `let mut checkout = git2::build::CheckoutBuilder::new(); checkout.force().disable_pathspec_match(true).path(path); match side { Ours => checkout.use_ours(true), Theirs => checkout.use_theirs(true) }; repo.checkout_index(None, Some(&mut checkout))?;` then `let mut index = repo.index()?; index.add_path(Path::new(path))?; index.write()?;`.
   - `None`: `match std::fs::remove_file(workdir.join(path)) { Err(e) if e.kind() != NotFound => return Err(e.into()), _ => {} }`, then `index.remove_path(Path::new(path))?; index.write()?;`.
   
   If `checkout_index` with `use_ours/use_theirs` does not write the file for a content conflict (check with the first test), fall back to writing the side's blob and setting the mode (`0o755` for `0o100755`, else `0o644`, Unix only) before `add_path`. Keep the test unchanged either way.

`has_conflict_markers(content: &[u8]) -> bool`: `if content[..content.len().min(8000)].contains(&0) { return false }`. Then iterate lines (`split(|b| *b == b'\n')`). Once a line starts with `b"<<<<<<<"`, set `opened = true`. Return `true` when `opened` and a line starts with `b">>>>>>>"`. Otherwise return `false`.

`mark_resolved(repo, path, force)`:
1. `find_conflict(repo, path)?;`
2. `let full = repo.workdir().ok_or(NoWorkdir)?.join(path);`
3. If `full` does not exist (`symlink_metadata` NotFound): `index.remove_path`, `write`, `Ok(Resolved)`.
4. Else if `!force && has_conflict_markers(&std::fs::read(&full)?)` → `Ok(MarkersRemain)` (nothing staged).
5. Else `index.add_path(Path::new(path))?; index.write()?; Ok(Resolved)`.

Doc comments: "ours" is stage 2 (HEAD) and "theirs" stage 3 (the incoming commit; during a rebase the commit being replayed). Neither function records undo; the paused operation's entry covers them.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test merge && cargo test -p git-core --lib merge::tests`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core/src/merge.rs crates/git-core/tests/merge.rs
git commit -m "feat(git-core): whole-file conflict resolution and mark resolved"
```

---

### Task A2: `git-core::pick` — mainline for merge commits

**Files:**
- Modify: `crates/git-core/src/pick.rs`
- Modify callers: `crates/repo-service/src/worker/history.rs` (pass `None` for now), any other `cherry_pick(`/`revert(` call (`grep -rn "pick::cherry_pick\|pick::revert" crates`)
- Test: `crates/git-core/tests/pick.rs`

**Interfaces:**
- Produces: `cherry_pick(repo, id, mainline: Option<u32>)`, `revert(repo, id, mainline: Option<u32>)`, `PickError::{MainlineRequired, InvalidMainline { mainline: u32 }}`; `MergeCommitUnsupported` removed.
- Also produces (crate-private, used by Task A3): `fn step(repo: &Repository, target: &Commit, kind: Kind, mainline: Option<u32>) -> Result<StepResult, PickError>` with `enum StepResult { Committed(Oid), Empty, Conflicted(Vec<String>) }`, and `fn check_ready(repo) -> Result<Commit /*HEAD*/, PickError>` (state Clean, born HEAD, nothing staged, signing config).

- [ ] **Step 1: Write the failing tests**

In `tests/pick.rs`, replace the merge-commit half of `cherry_pick_refuses_merge_commits_and_paused_operations` with `MainlineRequired`, update every `cherry_pick(&repo, id)` → `cherry_pick(&repo, id, None)` and `revert(&repo, id)` → `revert(&repo, id, None)`, then add:

```rust
/// base(a.txt) → main adds m.txt ; feature adds f.txt ; M = merge(main, feature) on branch
/// "merged". HEAD is left on a branch "target" at base. Built without start_merge so it does
/// not depend on 4c's signature.
fn merge_commit_fixture() -> (tempfile::TempDir, git2::Repository, String, String) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "base\n");
    common::commit_all(&repo, "base");
    let base_ref = base_branch_ref(&repo);
    let (merge_id, main_tip) = {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &base, false).unwrap();
        repo.branch("target", &base, false).unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        common::write_file(dir.path(), "f.txt", "feature\n");
        common::commit_all(&repo, "add f");
        let feature = repo.head().unwrap().peel_to_commit().unwrap();
        repo.set_head(&base_ref).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force().remove_untracked(true)))
            .unwrap();
        common::write_file(dir.path(), "m.txt", "main\n");
        common::commit_all(&repo, "add m");
        let main = repo.head().unwrap().peel_to_commit().unwrap();
        let mut merged = repo.merge_commits(&main, &feature, None).unwrap();
        let tree = repo.find_tree(merged.write_tree_to(&repo).unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        let merge = repo
            .commit(Some(&base_ref), &sig, &sig, "merge feature", &tree, &[&main, &feature])
            .unwrap();
        (merge.to_string(), main.id().to_string())
    };
    repo.set_head("refs/heads/target").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force().remove_untracked(true)))
        .unwrap();
    (dir, repo, merge_id, main_tip)
}

#[test]
fn a_merge_commit_needs_a_mainline() {
    let (_dir, repo, merge, _) = merge_commit_fixture();
    let before = repo.head().unwrap().target().unwrap();
    assert!(matches!(cherry_pick(&repo, &merge, None), Err(PickError::MainlineRequired)));
    assert!(matches!(revert(&repo, &merge, None), Err(PickError::MainlineRequired)));
    assert_eq!(repo.head().unwrap().target().unwrap(), before);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn cherry_picking_a_merge_against_parent_one_applies_the_merged_side() {
    let (dir, repo, merge, _) = merge_commit_fixture();
    let outcome = cherry_pick(&repo, &merge, Some(1)).unwrap();
    assert!(matches!(outcome, PickOutcome::Committed { .. }));
    assert!(dir.path().join("f.txt").exists());
    assert!(!dir.path().join("m.txt").exists());
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_count(), 1);
    assert_eq!(head.message().unwrap(), "merge feature");
}

#[test]
fn reverting_a_merge_against_parent_one_removes_the_merged_side_with_gits_message() {
    let (dir, repo, merge, main_tip) = merge_commit_fixture();
    // Put HEAD on the merge itself.
    repo.set_head(&base_branch_ref(&repo)).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    let outcome = revert(&repo, &merge, Some(1)).unwrap();
    assert!(matches!(outcome, PickOutcome::Committed { .. }));
    assert!(!dir.path().join("f.txt").exists());
    assert!(dir.path().join("m.txt").exists());
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head.message().unwrap(),
        format!(
            "Revert \"merge feature\"\n\nThis reverts commit {merge}, reversing\nchanges made to {main_tip}.\n"
        )
    );
}

#[test]
fn an_out_of_range_or_needless_mainline_is_refused() {
    let (_dir, repo, merge, main_tip) = merge_commit_fixture();
    assert!(matches!(
        cherry_pick(&repo, &merge, Some(3)),
        Err(PickError::InvalidMainline { mainline: 3 })
    ));
    assert!(matches!(
        cherry_pick(&repo, &merge, Some(0)),
        Err(PickError::InvalidMainline { mainline: 0 })
    ));
    assert!(matches!(
        cherry_pick(&repo, &main_tip, Some(1)),
        Err(PickError::InvalidMainline { mainline: 1 })
    ));
    assert_eq!(
        PickError::InvalidMainline { mainline: 3 }.to_string(),
        "Parent 3 doesn't exist on this commit."
    );
    assert_eq!(
        PickError::MainlineRequired.to_string(),
        "This is a merge commit. Choose which parent to apply it against."
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test pick`
Expected: FAIL to compile (arity, `MainlineRequired`).

- [ ] **Step 3: Implement**

- `PickError`: remove `MergeCommitUnsupported`; add `MainlineRequired` and `InvalidMainline { mainline: u32 }` with the Global Constraints texts.
- Split `apply` into `check_ready(repo) -> Result<Commit<'_>, PickError>` (the existing state/HEAD/staged/signing checks in their current order) and `step(repo, head: &Commit, target: &Commit, kind, mainline) -> Result<StepResult, PickError>` (validation, cherrypick/revert with options, cleanup on error, conflicts, empty, commit). `apply` = `check_ready` + resolve target + `step`, mapping `StepResult::Empty` → `Err(EmptyResult)` (unchanged single-pick behavior).
- Mainline validation in `step`, before touching anything: `match (target.parent_count(), mainline) { (n, None) if n > 1 => MainlineRequired, (n, Some(m)) if n <= 1 || m == 0 || m as usize > n => InvalidMainline { mainline: m }, _ => ok }`.
- Options: `let mut opts = git2::CherrypickOptions::new(); if let Some(m) = mainline { opts.mainline(m); } repo.cherrypick(target, Some(&mut opts))`; same with `git2::RevertOptions`.
- `revert_message(target, mainline)`: with `Some(m)` append `", reversing\nchanges made to {parent(m-1) id}."` to the "This reverts commit {id}" line (exact format in the test).
- Update `crates/repo-service/src/worker/history.rs` to pass `None` (Task A5 threads the real value).

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test pick && cargo build --workspace`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check
git add crates/git-core crates/repo-service/src/worker/history.rs
git commit -m "feat(git-core): cherry-pick and revert merge commits with a mainline parent"
```

---

### Task A3: `git-core::pick` — `cherry_pick_many` / `revert_many`

**Files:**
- Modify: `crates/git-core/src/pick.rs`
- Create: `crates/git-core/tests/pick_sequence.rs`

**Interfaces:**
- Produces (exactly as in the spec's API block): `SequenceKind`, `SequenceProgress`, `SequenceOutcome`, `SequenceState` (+ `progress()`), `SequenceRun`, `cherry_pick_many(repo, &[String]) -> SequenceRun`, `revert_many(repo, &[String]) -> SequenceRun`, `PickError::{NothingSelected, UnresolvedConflicts, HeadMoved, Merge(#[from] MergeError)}`. All value types derive `Debug, Clone, PartialEq, Eq`. `SequenceRun` derives `Debug` only (`PickError` is not `Clone`).
- Crate-private, used by Task A4: `fn run(repo, state: SequenceState) -> SequenceRun` (the step loop from `state.next`).

- [ ] **Step 1: Write the failing tests**

`tests/pick_sequence.rs`:

```rust
// crates/git-core/tests/pick_sequence.rs
mod common;

use git2::{Oid, Repository, RepositoryState};
use git_core::pick::{
    cherry_pick, cherry_pick_many, revert_many, PickError, SequenceKind, SequenceOutcome,
};

fn base_branch_ref(repo: &Repository) -> String {
    for name in ["main", "master"] {
        if let Ok(branch) = repo.find_branch(name, git2::BranchType::Local) {
            return branch.get().name().unwrap().to_string();
        }
    }
    panic!("no base branch");
}

fn head(repo: &Repository) -> String {
    repo.head().unwrap().target().unwrap().to_string()
}

/// Summaries of the last `n` first-parent commits, newest first.
fn summaries(repo: &Repository, n: usize) -> Vec<String> {
    let mut commit = repo.head().unwrap().peel_to_commit().unwrap();
    let mut out = vec![commit.summary().unwrap().to_string()];
    while out.len() < n {
        commit = commit.parent(0).unwrap();
        out.push(commit.summary().unwrap().to_string());
    }
    out
}

/// base: shared.txt="base\n", notes.txt. side (from base): "side one" (+one.txt),
/// "side two" (shared.txt="theirs\n", plus two.txt when `two_adds_a_file`), "side three"
/// (+three.txt). main (checked out) then commits "ours edit" (shared.txt="ours\n") when
/// `conflict`, else "main only" (+main.txt). Returns the side ids oldest-first.
fn side_branch(conflict: bool, two_adds_a_file: bool) -> (tempfile::TempDir, Repository, Vec<String>) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "shared.txt", "base\n");
    common::write_file(dir.path(), "notes.txt", "notes\n");
    common::commit_all(&repo, "base");
    let base_ref = base_branch_ref(&repo);
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("side", &base, false).unwrap();
    }
    repo.set_head("refs/heads/side").unwrap();
    let mut ids = Vec::new();
    let mut two: Vec<(&str, &str)> = vec![("shared.txt", "theirs\n")];
    if two_adds_a_file {
        two.push(("two.txt", "2\n"));
    }
    for (files, message) in [
        (vec![("one.txt", "1\n")], "side one"),
        (two, "side two"),
        (vec![("three.txt", "3\n")], "side three"),
    ] {
        for (path, contents) in files {
            common::write_file(dir.path(), path, contents);
        }
        common::commit_all(&repo, message);
        ids.push(head(&repo));
    }
    repo.set_head(&base_ref).unwrap();
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new().force().remove_untracked(true),
    ))
    .unwrap();
    if conflict {
        common::write_file(dir.path(), "shared.txt", "ours\n");
        common::commit_all(&repo, "ours edit");
    } else {
        common::write_file(dir.path(), "main.txt", "main\n");
        common::commit_all(&repo, "main only");
    }
    (dir, repo, ids)
}

#[test]
fn a_clean_sequence_applies_every_commit_oldest_first() {
    let (dir, repo, ids) = side_branch(false, true);
    let run = cherry_pick_many(&repo, &ids);
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed { applied: 3, skipped: vec![] }
    );
    assert!(run.state.is_none());
    assert_eq!(summaries(&repo, 4), ["side three", "side two", "side one", "main only"]);
    assert!(dir.path().join("three.txt").exists());
    assert_eq!(repo.state(), RepositoryState::Clean);
}

#[test]
fn a_conflict_in_the_middle_stops_with_state_and_progress() {
    let (_dir, repo, ids) = side_branch(true, true);
    let original = head(&repo);
    let run = cherry_pick_many(&repo, &ids);
    let SequenceOutcome::Conflicted { files, progress } = run.outcome.unwrap() else {
        panic!("expected a conflict")
    };
    assert_eq!(files, ["shared.txt"]);
    assert_eq!(progress.kind, SequenceKind::CherryPick);
    assert_eq!((progress.current_step, progress.total_steps), (2, 3));
    assert_eq!(progress.current_commit, ids[1]);
    let state = run.state.expect("a stopped sequence returns its state");
    assert_eq!(state.next, 1);
    assert_eq!(state.commits, ids);
    assert_eq!(state.original_head, original);
    assert_eq!(state.expected_head, head(&repo));
    assert_eq!(summaries(&repo, 2), ["side one", "ours edit"]);
    assert_eq!(repo.state(), RepositoryState::CherryPick);
}

#[test]
fn an_already_applied_commit_is_skipped() {
    let (_dir, repo, ids) = side_branch(false, true);
    cherry_pick(&repo, &ids[0], None).unwrap();
    let run = cherry_pick_many(&repo, &ids);
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed { applied: 2, skipped: vec![ids[0].clone()] }
    );
    assert_eq!(summaries(&repo, 3), ["side three", "side two", "side one"]);
}

#[test]
fn revert_many_applies_newest_first() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "base.txt", "base\n");
    common::commit_all(&repo, "base");
    let mut ids = Vec::new();
    for name in ["a", "b", "c"] {
        common::write_file(dir.path(), &format!("{name}.txt"), "x\n");
        common::commit_all(&repo, name);
        ids.push(head(&repo));
    }
    let run = revert_many(&repo, &ids);
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed { applied: 3, skipped: vec![] }
    );
    assert_eq!(summaries(&repo, 3), ["Revert \"a\"", "Revert \"b\"", "Revert \"c\""]);
    for name in ["a", "b", "c"] {
        assert!(!dir.path().join(format!("{name}.txt")).exists());
    }
}

#[test]
fn a_merge_commit_in_a_sequence_is_refused_before_anything_changes() {
    let (dir, repo, ids) = side_branch(false, true);
    let before = head(&repo);
    let merge = {
        let ours = repo.head().unwrap().peel_to_commit().unwrap();
        let theirs = repo.find_commit(Oid::from_str(&ids[2]).unwrap()).unwrap();
        let mut index = repo.merge_commits(&ours, &theirs, None).unwrap();
        let tree = repo.find_tree(index.write_tree_to(&repo).unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(None, &sig, &sig, "merge", &tree, &[&ours, &theirs]).unwrap().to_string()
    };
    let run = cherry_pick_many(&repo, &[ids[0].clone(), merge]);
    assert!(matches!(run.outcome, Err(PickError::MainlineRequired)));
    assert!(run.state.is_none());
    assert_eq!(head(&repo), before);
    assert!(!dir.path().join("one.txt").exists());
}

#[test]
fn nothing_selected_is_refused() {
    let (_dir, repo, _) = side_branch(false, true);
    let run = cherry_pick_many(&repo, &[]);
    assert!(matches!(run.outcome, Err(PickError::NothingSelected)));
    assert!(run.state.is_none());
}

#[test]
fn an_error_on_the_first_step_returns_no_state() {
    let (dir, repo, ids) = side_branch(false, true);
    let before = head(&repo);
    // An untracked file in the way of the first pick's new file: libgit2's SAFE checkout refuses.
    common::write_file(dir.path(), "one.txt", "mine\n");
    let run = cherry_pick_many(&repo, &ids);
    assert!(run.outcome.is_err());
    assert!(run.state.is_none());
    assert_eq!(head(&repo), before);
    assert_eq!(repo.state(), RepositoryState::Clean);
    assert_eq!(std::fs::read_to_string(dir.path().join("one.txt")).unwrap(), "mine\n");
}

#[test]
fn an_error_on_a_later_step_returns_the_state_at_that_step() {
    let (dir, repo, ids) = side_branch(false, true);
    common::write_file(dir.path(), "three.txt", "mine\n");
    let run = cherry_pick_many(&repo, &ids);
    assert!(run.outcome.is_err());
    let state = run.state.expect("later failure keeps the sequence");
    assert_eq!(state.next, 2);
    assert_eq!(state.expected_head, head(&repo));
    assert_eq!(summaries(&repo, 2), ["side two", "side one"]);
    assert_eq!(repo.state(), RepositoryState::Clean);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test pick_sequence`
Expected: FAIL to compile (types and functions missing).

- [ ] **Step 3: Implement**

Types as in the spec. `SequenceState::progress()` → `SequenceProgress { kind, current_step: next + 1, total_steps: commits.len(), current_commit: commits[next].clone() }`. `SequenceKind` maps to the private `Kind`.

`fn start(repo, ids_oldest_first, kind) -> SequenceRun`:
1. `ids.is_empty()` → `NothingSelected`.
2. `let head = check_ready(repo)?` (errors → `SequenceRun { outcome: Err(e), state: None }`).
3. Resolve every id to a commit; any `parent_count() > 1` → `MainlineRequired`.
4. `commits` = ids as given for CherryPick, reversed for Revert.
5. `run(repo, SequenceState { kind, commits, next: 0, skipped: vec![], original_head: head.id().to_string(), expected_head: head.id().to_string() })`.

`fn run(repo, mut state) -> SequenceRun`, loop while `state.next < state.commits.len()`:
- `let head = repo.head()?.peel_to_commit()?; state.expected_head = head.id().to_string();`
- `match step(repo, &head, &target, kind, None)`:
  - `Ok(Committed(_))` → `state.next += 1`
  - `Ok(Empty)` → `state.skipped.push(id); state.next += 1` (`step` already `cleanup_state`d)
  - `Ok(Conflicted(files))` → return `SequenceRun { outcome: Ok(Conflicted { files, progress: state.progress() }), state: Some(state) }`
  - `Err(e)` → if `state.next == 0 && state.skipped.is_empty() && state.expected_head == state.original_head` return `{ Err(e), None }`, else `{ Err(e), Some(state) }`
- After the loop: `Completed { applied: commits.len() - skipped.len(), skipped }`, `state: None`.

`step`'s commit uses `HeadUpdate { expected: Some(head.id()), reflog_operation: "commit" }` as today, so a concurrent HEAD move fails the step instead of committing onto it.

Public wrappers: `cherry_pick_many(repo, ids) = start(repo, ids, SequenceKind::CherryPick)`, `revert_many(repo, ids) = start(repo, ids, SequenceKind::Revert)`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test pick_sequence && cargo test -p git-core --test pick`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all -- --check
git add crates/git-core/src/pick.rs crates/git-core/tests/pick_sequence.rs
git commit -m "feat(git-core): cherry-pick and revert several commits as a sequence"
```

---

### Task A4: `git-core::pick` — `continue_sequence` / `abort_sequence`; `HeadInfo.sequence`

**Files:**
- Modify: `crates/git-core/src/pick.rs`, `crates/git-core/src/repo.rs`
- Modify callers of `HeadInfo { … }` literals (`grep -rn "HeadInfo {" crates`): add `sequence: None`
- Test: `crates/git-core/tests/pick_sequence.rs`, `crates/git-core/tests/head_info.rs`

**Interfaces:**
- Produces: `continue_sequence(repo, SequenceState) -> SequenceRun`, `abort_sequence(repo, &SequenceState) -> Result<SequenceAbort, PickError>`, `SequenceAbort { pub rewound: bool }` (`Debug, Clone, Copy, PartialEq, Eq`), `HeadInfo.sequence: Option<SequenceProgress>`.

- [ ] **Step 1: Write the failing tests**

Append to `tests/pick_sequence.rs` (extend the `use` with `abort_sequence, continue_sequence, SequenceState`, `git_core::merge::{resolve_whole_file, ConflictSide}` and `std::path::Path`):

```rust
fn stage(repo: &Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

fn paused(conflict_adds_a_file: bool) -> (tempfile::TempDir, Repository, Vec<String>, SequenceState) {
    let (dir, repo, ids) = side_branch(true, conflict_adds_a_file);
    let state = cherry_pick_many(&repo, &ids).state.unwrap();
    (dir, repo, ids, state)
}

#[test]
fn continue_after_resolving_lands_the_rest_with_original_authors() {
    let (dir, repo, _ids, state) = paused(true);
    common::write_file(dir.path(), "shared.txt", "resolved\n");
    stage(&repo, "shared.txt");
    let run = continue_sequence(&repo, state);
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed { applied: 3, skipped: vec![] }
    );
    assert!(run.state.is_none());
    assert_eq!(summaries(&repo, 4), ["side three", "side two", "side one", "ours edit"]);
    assert_eq!(std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(), "resolved\n");
    assert!(dir.path().join("two.txt").exists());
    assert_eq!(repo.state(), RepositoryState::Clean);
}

#[test]
fn continue_with_conflicts_left_is_refused_and_keeps_the_state() {
    let (_dir, repo, _ids, state) = paused(true);
    let run = continue_sequence(&repo, state.clone());
    assert!(matches!(run.outcome, Err(PickError::UnresolvedConflicts)));
    assert_eq!(run.state, Some(state));
    assert_eq!(repo.state(), RepositoryState::CherryPick);
}

#[test]
fn whole_file_ours_empties_the_step_and_it_is_skipped() {
    let (_dir, repo, ids, state) = paused(false);
    resolve_whole_file(&repo, "shared.txt", ConflictSide::Ours).unwrap();
    let run = continue_sequence(&repo, state);
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed { applied: 2, skipped: vec![ids[1].clone()] }
    );
    assert_eq!(summaries(&repo, 3), ["side three", "side one", "ours edit"]);
    assert_eq!(repo.state(), RepositoryState::Clean);
}

#[test]
fn continue_retries_a_step_that_failed_without_a_conflict() {
    let (dir, repo, ids) = side_branch(false, true);
    common::write_file(dir.path(), "three.txt", "mine\n");
    let state = cherry_pick_many(&repo, &ids).state.unwrap();
    std::fs::remove_file(dir.path().join("three.txt")).unwrap();
    let run = continue_sequence(&repo, state);
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed { applied: 3, skipped: vec![] }
    );
}

#[test]
fn continue_counts_a_step_the_user_committed_elsewhere() {
    let (dir, mut repo, _ids, state) = paused(true);
    common::write_file(dir.path(), "shared.txt", "resolved\n");
    stage(&repo, "shared.txt");
    git_core::commit::commit(&mut repo, "side two (by hand)").unwrap();
    let run = continue_sequence(&repo, state);
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed { applied: 3, skipped: vec![] }
    );
    assert_eq!(summaries(&repo, 3), ["side three", "side two (by hand)", "side one"]);
}

#[test]
fn abort_restores_the_original_head_and_keeps_unrelated_edits() {
    let (dir, repo, ids) = side_branch(true, true);
    let original = head(&repo);
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    common::write_file(dir.path(), "notes.txt", "local edit\n");
    let state = cherry_pick_many(&repo, &ids).state.unwrap();

    let abort = abort_sequence(&repo, &state).unwrap();
    assert!(abort.rewound);
    assert_eq!(head(&repo), original);
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), branch);
    assert_eq!(repo.state(), RepositoryState::Clean);
    assert!(!repo.index().unwrap().has_conflicts());
    assert!(!dir.path().join("one.txt").exists());
    assert_eq!(std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(), "ours\n");
    assert_eq!(std::fs::read_to_string(dir.path().join("notes.txt")).unwrap(), "local edit\n");
}

#[test]
fn abort_after_head_moved_does_not_rewind_and_continue_refuses() {
    let (dir, repo, _ids, state) = paused(true);
    git_core::merge::abort_merge(&repo).unwrap();
    // Two commits on top of the stop point: neither "still there" nor "one step further".
    for name in ["external one", "external two"] {
        common::write_file(dir.path(), &format!("{name}.txt"), "x\n");
        common::commit_all(&repo, name);
    }
    let moved_to = head(&repo);

    let run = continue_sequence(&repo, state.clone());
    assert!(matches!(run.outcome, Err(PickError::HeadMoved)));
    assert_eq!(run.state, Some(state.clone()));

    let abort = abort_sequence(&repo, &state).unwrap();
    assert!(!abort.rewound);
    assert_eq!(head(&repo), moved_to);
}

#[test]
fn abort_after_an_error_stop_rewinds_the_landed_steps() {
    let (dir, repo, ids) = side_branch(false, true);
    let original = head(&repo);
    common::write_file(dir.path(), "three.txt", "mine\n");
    let state = cherry_pick_many(&repo, &ids).state.unwrap();
    let abort = abort_sequence(&repo, &state).unwrap();
    assert!(abort.rewound);
    assert_eq!(head(&repo), original);
    // The untracked file that blocked step three is not ours to delete.
    assert_eq!(std::fs::read_to_string(dir.path().join("three.txt")).unwrap(), "mine\n");
}
```

`tests/head_info.rs`, extend the existing operation test or add:

```rust
#[test]
fn head_info_from_git_core_never_reports_a_sequence() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "a");
    assert_eq!(git_core::repo::head_info(&repo).unwrap().sequence, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test pick_sequence --test head_info`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

`continue_sequence(repo, mut state)`:
1. `crate::signing::signing_config(repo)` (error → `{ Err, Some(state) }`).
2. `match repo.state()`:
   - `CherryPick | CherryPickSequence | Revert | RevertSequence`: if `repo.index()?.has_conflicts()` → `{ Err(UnresolvedConflicts), Some(state) }`. Else `finish_paused(repo, &mut state)`: `let head = HEAD commit; let tree_id = index.write_tree()?;`. If `tree_id == head.tree_id()`, then `cleanup_state`, `skipped.push(current)`, `next += 1`. Otherwise commit via `signing::create_commit(repo, Some(HeadUpdate { expected: Some(head.id()), reflog_operation: "commit" }), author, &committer, &message, &tree, &[&head])`. The author is the target's author for CherryPick and the committer for Revert. The message is `repo.message().ok()`, else the target's message (CherryPick) or `revert_message(target, None)` (Revert). Then `cleanup_state`, `next += 1`. On error return `{ Err, Some(state) }`.
   - `Clean`: `let head_id = HEAD oid`. If `head_id == expected_head`, retry (no change). Else if the HEAD commit's `parent_id(0) == expected_head`, `next += 1` (landed elsewhere). Else `{ Err(HeadMoved), Some(state) }`.
   - other states → `{ Err(OperationInProgress), Some(state) }`.
3. `run(repo, state)`.

`abort_sequence(repo, state)`:
1. If `repo.state()` is CherryPick/Revert(Sequence): `crate::merge::abort_merge(repo)?` (`PickError::Merge` via `#[from]`).
2. `let head_id = HEAD oid`. If `head_id.to_string() == state.expected_head && state.expected_head != state.original_head`:
   - `let original = repo.find_commit(Oid::from_str(&state.original_head)?)?;`
   - `repo.checkout_tree(original.as_object(), None)?` (default SAFE, which refuses on a local edit in the way).
   - Move HEAD: if `repo.head_detached()?`, `repo.set_head_detached(original.id())?`; else `repo.head()?.set_target(original.id(), reflog)?` with `reflog = "cherry-pick: abort"` / `"revert: abort"`.
   - `Ok(SequenceAbort { rewound: true })`
3. Else, if `head_id == original_head`, `Ok(SequenceAbort { rewound: true })` (nothing landed). Otherwise `Ok(SequenceAbort { rewound: false })`.

`repo.rs`: `pub sequence: Option<crate::pick::SequenceProgress>` on `HeadInfo`, `None` in both constructors, doc comment "Filled by `repo-service` from its stored sequence state; `git-core` has no persistence."

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core && cargo build --workspace`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check
git add crates
git commit -m "feat(git-core): continue and abort a cherry-pick or revert sequence"
```

---

### Task A5: `repo-service` — sequence store, commands, gates and undo

**Files:**
- Create: `crates/repo-service/src/worker/sequence_store.rs`
- Modify: `crates/repo-service/src/worker/mod.rs` (`mod sequence_store;`, `Command` variants, dispatch, `in_progress`, `aborts`, gates, tests), `worker/history.rs` (handlers and `WorkerHandle` methods for picks/sequences), `worker/merge.rs` (`ResolveWholeFile`, `MarkResolved`), `worker/undo.rs` (`plan_for`, `hold_reply`, `assert_held!` list)

**Interfaces:**
- Produces `Command` variants: `CherryPick { commit_id, mainline: Option<u32>, reply }`, `RevertCommit { commit_id, mainline, reply }`, `CherryPickMany { commit_ids: Vec<String>, reply: Sender<Result<SequenceOutcome, String>> }`, `RevertMany { … }`, `ContinueSequence { reply }`, `AbortSequence { reply: Sender<Result<SequenceAbort, String>> }`, `ResolveWholeFile { path, side: ConflictSide, reply: Sender<Result<(), String>> }`, `MarkResolved { path, force: bool, reply: Sender<Result<MarkResolvedOutcome, String>> }`.
- Produces `WorkerHandle` methods: `cherry_pick(id, mainline)`, `revert_commit(id, mainline)`, `cherry_pick_many(ids)`, `revert_many(ids)`, `continue_sequence()`, `abort_sequence()`, `resolve_whole_file(path, side)`, `mark_resolved(path, force)`.

- [ ] **Step 1: Write the failing store tests**

`sequence_store.rs` (inline tests):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use git_core::pick::{SequenceKind, SequenceState};

    fn state() -> SequenceState {
        SequenceState {
            kind: SequenceKind::Revert,
            commits: vec!["a".repeat(40), "b".repeat(40)],
            next: 1,
            skipped: vec!["a".repeat(40)],
            original_head: "c".repeat(40),
            expected_head: "d".repeat(40),
        }
    }

    #[test]
    fn saves_and_loads_a_state_round_trip() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let store = SequenceStore::open(&repo);
        assert_eq!(store.load(), None);
        store.save(&state()).unwrap();
        assert!(repo.path().join("browsitory/sequence.json").exists());
        assert_eq!(store.load(), Some(state()));
        store.clear().unwrap();
        assert_eq!(store.load(), None);
        store.clear().unwrap(); // clearing twice is fine
    }

    #[test]
    fn a_corrupt_or_unknown_state_file_is_ignored() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let store = SequenceStore::open(&repo);
        std::fs::create_dir_all(repo.path().join("browsitory")).unwrap();
        let file = repo.path().join("browsitory/sequence.json");
        std::fs::write(&file, "{not json").unwrap();
        assert_eq!(store.load(), None);
        std::fs::write(
            &file,
            r#"{"version":2,"kind":"CherryPick","commits":[],"next":0,"skipped":[],"originalHead":"","expectedHead":""}"#,
        )
        .unwrap();
        assert_eq!(store.load(), None);
        store.save(&state()).unwrap(); // overwrites the unreadable file
        assert_eq!(store.load(), Some(state()));
    }

    #[test]
    fn the_wire_format_is_camel_case_with_a_version() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let store = SequenceStore::open(&repo);
        store.save(&state()).unwrap();
        let json: serde_json::Value = serde_json::from_slice(
            &std::fs::read(repo.path().join("browsitory/sequence.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(json["version"], 1);
        assert_eq!(json["kind"], "Revert");
        assert_eq!(json["next"], 1);
        assert_eq!(json["originalHead"], "c".repeat(40));
        assert_eq!(json["expectedHead"], "d".repeat(40));
    }
}
```

- [ ] **Step 2: Write the failing worker tests**

In `worker/mod.rs` tests (next to `conflicting_branches`; add `use git_core::pick::{SequenceAbort, SequenceOutcome};` and `use git_core::merge::{ConflictSide, MarkResolvedOutcome};` to the test module):

```rust
/// base: a.txt="base\n". feature: "f one" (+b.txt), "f two" (a.txt="feature\n"),
/// "f three" (+c.txt). main: a.txt="main\n". Returns the feature ids oldest-first.
fn side_commits_with_a_conflict() -> (TempDir, Repository, Vec<String>) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    let base_branch = repo.head().unwrap().shorthand().unwrap().to_string();
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
    }
    repo.set_head("refs/heads/feature").unwrap();
    let mut ids = Vec::new();
    for (path, contents, message) in [
        ("b.txt", "b\n", "f one"),
        ("a.txt", "feature\n", "f two"),
        ("c.txt", "c\n", "f three"),
    ] {
        write_file(dir.path(), path, contents);
        commit_all(&repo, message);
        ids.push(repo.head().unwrap().target().unwrap().to_string());
    }
    repo.set_head(&format!("refs/heads/{base_branch}")).unwrap();
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new().force().remove_untracked(true),
    ))
    .unwrap();
    write_file(dir.path(), "a.txt", "main\n");
    commit_all(&repo, "main change");
    (dir, repo, ids)
}

#[test]
fn a_conflicted_sequence_finished_by_continue_is_one_undo_entry() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let original = repo.head().unwrap().target().unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let outcome = handle.cherry_pick_many(ids.clone()).unwrap();
    assert!(matches!(outcome, SequenceOutcome::Conflicted { .. }));
    let progress = handle.get_head_info().unwrap().sequence.unwrap();
    assert_eq!((progress.current_step, progress.total_steps), (2, 3));
    assert_eq!(handle.undo().unwrap(), UndoOutcome::OperationInProgress);
    assert_eq!(
        handle.cherry_pick(ids[0].clone(), None).unwrap_err(),
        "Finish or abort the current merge/cherry-pick/revert/rebase first."
    );

    handle.resolve_conflict("a.txt".into(), "resolved\n".into()).unwrap();
    let outcome = handle.continue_sequence().unwrap();
    assert_eq!(outcome, SequenceOutcome::Completed { applied: 3, skipped: vec![] });
    assert_eq!(handle.get_head_info().unwrap().sequence, None);
    assert!(!repo.path().join("browsitory/sequence.json").exists());
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Cherry-pick 3 commits".into())
    );
    assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(repo.head().unwrap().target().unwrap(), original);
}

#[test]
fn an_aborted_sequence_records_nothing() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let original = repo.head().unwrap().target().unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.revert_many(vec![]).unwrap_err(); // NothingSelected, no state
    handle.cherry_pick_many(ids).unwrap();
    assert_eq!(handle.abort_sequence().unwrap(), SequenceAbort { rewound: true });
    assert_eq!(repo.head().unwrap().target().unwrap(), original);
    assert_eq!(handle.get_head_info().unwrap().sequence, None);
    assert_eq!(handle.get_undo_state().unwrap(), UndoState { undo: None, redo: None });
    assert_eq!(
        handle.continue_sequence().unwrap_err(),
        "No cherry-pick or revert is in progress."
    );
}

#[test]
fn abort_merge_during_a_sequence_clears_it() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.cherry_pick_many(ids).unwrap();
    handle.abort_merge().unwrap();
    assert_eq!(handle.get_head_info().unwrap().sequence, None);
    assert!(!repo.path().join("browsitory/sequence.json").exists());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn whole_file_and_mark_resolved_round_trip_through_the_worker() {
    let (dir, _repo) = conflicting_branches();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.start_merge("feature".into()).unwrap(); // adapt to 4c's signature if it changed
    assert_eq!(
        handle.mark_resolved("a.txt".into(), false).unwrap(),
        MarkResolvedOutcome::MarkersRemain
    );
    handle.resolve_whole_file("a.txt".into(), ConflictSide::Theirs).unwrap();
    assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "feature\n");
}
```

In `undo.rs` tests: update `assert_held!(CherryPick { commit_id: s() })` → `CherryPick { commit_id: s(), mainline: None }` (same for `RevertCommit`) and add:

```rust
assert_held!(CherryPickMany { commit_ids: vec![] });
assert_held!(RevertMany { commit_ids: vec![] });
assert_held!(ContinueSequence {});
assert_held!(AbortSequence {});
```

(Match `assert_held!`'s actual syntax for unit-like field lists; if it needs at least one field, add the arms to the explicit reply-held list the macro checks instead.) Add label tests next to the existing ones: `CherryPickMany` with 3 ids → `"Cherry-pick 3 commits"`, `RevertMany` with 1 id → `"Revert 1 commit"`.

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p repo-service`
Expected: FAIL to compile.

- [ ] **Step 4: Implement**

`sequence_store.rs`:
- `#[derive(Serialize, Deserialize)] #[serde(rename_all = "camelCase")] struct StoredSequence { version: u32, kind: StoredKind, commits: Vec<String>, next: usize, skipped: Vec<String>, original_head: String, expected_head: String }` with `enum StoredKind { CherryPick, Revert }` and `From` conversions both ways to `git_core::pick::SequenceState`. `const FORMAT_VERSION: u32 = 1;`
- `SequenceStore { file: PathBuf }`. `open(repo)` → `repo.path().join("browsitory").join("sequence.json")`.
- `load(&self) -> Option<SequenceState>`: read. `NotFound` → `None` silently. Any other read error, parse error, version mismatch, or `next >= commits.len()` → `eprintln!("browsitory: ignoring sequence state: {reason}")`, `None`.
- `save(&self, state) -> Result<(), String>`: `create_dir_all(parent)`, temp `sequence.json.{pid}.tmp`, `write_all`, `sync_all`, `rename`. Remove the temp file on failure (copy `UndoStore::save`'s write closure).
- `clear(&self) -> Result<(), String>`: `remove_file`, `NotFound` → `Ok`.

`worker/mod.rs`:
- `let sequence_store = sequence_store::SequenceStore::open(&repo);` next to `undo_recorder`.
- `in_progress = |repo, rebase_state| repo.state() != Clean || rebase_state.is_some() || sequence_store.load().is_some()`.
- `aborts = matches!(command, AbortMerge { .. } | AbortRebase { .. } | AbortSequence { .. })`.
- Gate before dispatch: when `sequence_store.load().is_some()` and the command is `CherryPick | RevertCommit | CherryPickMany | RevertMany | StartMerge | StartRebase | AmendCommit`, reply `Err(git_core::pick::PickError::OperationInProgress.to_string())` and skip the handler. Implement this as a `history::sequence_gate(&mut command, &sequence_store) -> bool` helper that sends the error on the command's own reply and returns `true` when handled. Undo's `finish` still runs for consistency.
- Dispatch: `CherryPickMany`/`RevertMany` → `history::pick_many(&repo, &sequence_store, kind, commit_ids, reply)`. Persist with `run.state` (`Some` → `save`, `None` → `clear`; if the save fails, reply `Err(format!("{outcome_or_error} (could not save the sequence state: {e})"))`). Reply with `run.outcome.map_err(to_string)`. `ContinueSequence` → load or reply `"No cherry-pick or revert is in progress."`, then `continue_sequence` and persist the same way. `AbortSequence` → load (or the same reply), then `abort_sequence`, `clear()` on `Ok`.
- `AbortMerge`: if `sequence_store.load()` is `Some(state)`, run the `AbortSequence` path (`abort_sequence` + `clear`) and reply `Ok(())`. Otherwise call `merge::abort` as today.
- `GetHeadInfo`: `history::head_info(&repo, &sequence_store, reply)` sets `info.sequence = sequence_store.load().map(|s| s.progress())`.
- `ResolveWholeFile`/`MarkResolved` → `merge.rs` handlers calling `git_core::merge::*`.
- Every handler converts errors with `.to_string()`, like its neighbors.

`undo.rs`:
- `plan_for`: `CherryPickMany { commit_ids, .. } => (count_label("Cherry-pick", commit_ids.len()), Full)` and the same for `RevertMany` with `"Revert"`. `fn count_label(verb, n) -> String { format!("{verb} {n} {}", if n == 1 { "commit" } else { "commits" }) }`.
- `hold_reply`: add `CherryPickMany`/`RevertMany`/`ContinueSequence` (reply type `Sender<Result<SequenceOutcome, String>>`, an arm of their own) and `AbortSequence` (its own arm).
- Update the doc comment on `hold_reply` to list `ContinueSequence`/`AbortSequence` as commands that can end a pending operation.

- [ ] **Step 5: Run tests**

Run: `cargo test -p repo-service`
Expected: PASS.

- [ ] **Step 6: Lint and commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check
git add crates/repo-service
git commit -m "feat(repo-service): persisted cherry-pick/revert sequences with undo and operation gates"
```

---

### Task A6: Tauri commands and sidecar methods

**Files:**
- Modify: `crates/tauri-app/src/commands/history.rs` (`cherry_pick`/`revert_commit` gain `mainline: Option<u32>`; new `cherry_pick_many`, `revert_many`, `continue_sequence`, `abort_sequence`), `commands/merge.rs` (`resolve_whole_file`, `mark_resolved`), `commands/mod.rs` (DTOs + tests; `HeadInfoDto.sequence`), `main.rs` (`use` list + `generate_handler!`)
- Modify: `crates/vscode-sidecar/src/dispatch.rs` (method table, params, DTO copies), `tests/protocol_roundtrip.rs`

**Interfaces:**
- Produces the wire rows of the spec's table for Part A.

- [ ] **Step 1: Write the failing Tauri DTO tests**

In `commands/mod.rs` tests:

```rust
#[test]
fn sequence_outcome_and_progress_dtos_serialize_tagged_camel_case() {
    use git_core::pick::{SequenceKind, SequenceOutcome, SequenceProgress};
    let progress = SequenceProgress {
        kind: SequenceKind::CherryPick,
        current_step: 2,
        total_steps: 5,
        current_commit: "abc".into(),
    };
    let conflicted = serde_json::to_value(SequenceOutcomeDto::from(SequenceOutcome::Conflicted {
        files: vec!["a.txt".into()],
        progress: progress.clone(),
    }))
    .unwrap();
    assert_eq!(
        conflicted,
        serde_json::json!({
            "kind": "Conflicted",
            "files": ["a.txt"],
            "progress": { "kind": "CherryPick", "currentStep": 2, "totalSteps": 5, "currentCommit": "abc" }
        })
    );
    let completed = serde_json::to_value(SequenceOutcomeDto::from(SequenceOutcome::Completed {
        applied: 2,
        skipped: vec!["s".into()],
    }))
    .unwrap();
    assert_eq!(completed, serde_json::json!({ "kind": "Completed", "applied": 2, "skipped": ["s"] }));

    let head = serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
        head_id: None,
        detached_at: None,
        head_pushed: false,
        operation: None,
        sequence: Some(SequenceProgress { kind: SequenceKind::Revert, ..progress }),
    }))
    .unwrap();
    assert_eq!(head["sequence"]["kind"], "Revert");
    let none = serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
        head_id: None,
        detached_at: None,
        head_pushed: false,
        operation: None,
        sequence: None,
    }))
    .unwrap();
    assert!(none.as_object().unwrap().contains_key("sequence"));
    assert_eq!(none["sequence"], serde_json::Value::Null);
}

#[test]
fn mark_resolved_and_abort_dtos_and_conflict_side_input() {
    use git_core::merge::MarkResolvedOutcome;
    assert_eq!(
        serde_json::to_value(MarkResolvedOutcomeDto::from(MarkResolvedOutcome::MarkersRemain)).unwrap(),
        serde_json::json!({ "kind": "MarkersRemain" })
    );
    assert_eq!(
        serde_json::to_value(SequenceAbortDto::from(git_core::pick::SequenceAbort { rewound: false })).unwrap(),
        serde_json::json!({ "rewound": false })
    );
    let side: ConflictSideDto = serde_json::from_value(serde_json::json!("Theirs")).unwrap();
    assert_eq!(git_core::merge::ConflictSide::from(side), git_core::merge::ConflictSide::Theirs);
}
```

(Fix the existing `HeadInfo` literal in `head_info_dto…` tests by adding `sequence: None`.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p tauri-app`
Expected: FAIL to compile.

- [ ] **Step 3: Implement the Tauri side**

- DTOs in `commands/mod.rs` next to `PickOutcomeDto`: `SequenceProgressDto` (`kind: &'static str`, camelCase), `SequenceOutcomeDto` (`#[serde(tag = "kind")]`, `Conflicted { files, progress }`, `Completed { applied, skipped }`), `SequenceAbortDto { rewound }`, `MarkResolvedOutcomeDto` (`#[serde(tag = "kind")] enum { Resolved, MarkersRemain }`), `ConflictSideDto` (`Deserialize`, `Ours | Theirs`) + `From` into `ConflictSide`. `HeadInfoDto.sequence: Option<SequenceProgressDto>`.
- Commands (all `#[tauri::command] pub async fn`, through `worker_handle(&state, &repo_path)?`): `cherry_pick(repo_path, commit_id, mainline: Option<u32>)`, `revert_commit(…, mainline)`, `cherry_pick_many(repo_path, commit_ids: Vec<String>)`, `revert_many(…)`, `continue_sequence(repo_path)`, `abort_sequence(repo_path)`, `resolve_whole_file(repo_path, path, side: ConflictSideDto)`, `mark_resolved(repo_path, path, force: bool)`.
- Register the new ones in `main.rs`.

- [ ] **Step 4: Write the failing sidecar round-trip test**

```rust
#[test]
fn cherry_pick_many_conflicts_then_continue_and_whole_file_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().shorthand().unwrap().to_string();
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
    }
    repo.set_head("refs/heads/feature").unwrap();
    write_file(dir.path(), "a.txt", "feature\n");
    commit_all(&repo, "feature edit");
    let feature_tip = repo.head().unwrap().target().unwrap().to_string();
    repo.set_head(&format!("refs/heads/{base}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    write_file(dir.path(), "a.txt", "main\n");
    commit_all(&repo, "main edit");

    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({ "path": repo_path }));

    let started = sidecar.call(
        2,
        "cherry_pick_many",
        serde_json::json!({ "repoPath": repo_path, "commitIds": [feature_tip] }),
    );
    assert_eq!(started["result"]["kind"], "Conflicted");
    assert_eq!(started["result"]["progress"]["totalSteps"], 1);
    let head = sidecar.call(3, "get_head_info", serde_json::json!({ "repoPath": repo_path }));
    assert_eq!(head["result"]["sequence"]["currentStep"], 1);

    let marked = sidecar.call(
        4,
        "mark_resolved",
        serde_json::json!({ "repoPath": repo_path, "path": "a.txt", "force": false }),
    );
    assert_eq!(marked["result"]["kind"], "MarkersRemain");
    sidecar.call(
        5,
        "resolve_whole_file",
        serde_json::json!({ "repoPath": repo_path, "path": "a.txt", "side": "Theirs" }),
    );
    let done = sidecar.call(6, "continue_sequence", serde_json::json!({ "repoPath": repo_path }));
    assert_eq!(done["result"], serde_json::json!({ "kind": "Completed", "applied": 1, "skipped": [] }));
    let aborted = sidecar.call(7, "abort_sequence", serde_json::json!({ "repoPath": repo_path }));
    assert_eq!(aborted["error"]["message"], "No cherry-pick or revert is in progress.");
}
```

Check how existing sidecar tests read a JSON-RPC error and use that field path if it is not `["error"]["message"]`.

- [ ] **Step 5: Implement the sidecar methods**

Method table entries for the six new methods plus `mainline` on `cherry_pick`/`revert_commit` (a `PickParams { repo_path, commit_id, #[serde(default)] mainline: Option<u32> }` replacing `CommitIdParams` for those two). Private DTO copies with the same shapes as Tauri's (the sidecar keeps its own DTOs by convention), and `HeadInfoDto.sequence` in the sidecar's copy.

- [ ] **Step 6: Run tests**

Run: `cargo test -p tauri-app && cargo test -p vscode-sidecar`
Expected: PASS.

- [ ] **Step 7: Lint and commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check
git add crates/tauri-app crates/vscode-sidecar
git commit -m "feat: expose sequences, mainline picks and whole-file resolution over Tauri and the sidecar"
```

---

### Task A7: `RepoClient` — both clients

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts`
- Test: `frontend/src/ipc/tauriRepoClient.test.ts`, `vscodeRepoClient.test.ts`
- Fixtures: every full `RepoClient` literal `tsc` reports (`App.test.tsx` `fakeClient`, `useAppState.test.ts`, `ConflictResolutionPane.test.tsx`, `DiffPane.test.tsx`, `RebasePlanner.test.tsx`, `RepoPicker.test.tsx`, `CommitHeader.test.tsx`, …); every `HeadInfo` literal (add `sequence: null`)

**Interfaces:**

```ts
export type ConflictSide = "Ours" | "Theirs";
export type MarkResolvedOutcome = { kind: "Resolved" } | { kind: "MarkersRemain" };
export type SequenceKind = "CherryPick" | "Revert";
export interface SequenceProgress { kind: SequenceKind; currentStep: number; totalSteps: number; currentCommit: string; }
export type SequenceOutcome =
  | { kind: "Completed"; applied: number; skipped: string[] }
  | { kind: "Conflicted"; files: string[]; progress: SequenceProgress };
export interface SequenceAbort { rewound: boolean; }
// HeadInfo gains: sequence: SequenceProgress | null;

cherryPick(repoPath: string, commitId: string, mainline?: number | null): Promise<PickOutcome>;
revertCommit(repoPath: string, commitId: string, mainline?: number | null): Promise<PickOutcome>;
cherryPickMany(repoPath: string, commitIdsOldestFirst: string[]): Promise<SequenceOutcome>;
revertMany(repoPath: string, commitIdsOldestFirst: string[]): Promise<SequenceOutcome>;
continueSequence(repoPath: string): Promise<SequenceOutcome>;
abortSequence(repoPath: string): Promise<SequenceAbort>;
resolveWholeFile(repoPath: string, path: string, side: ConflictSide): Promise<void>;
markResolved(repoPath: string, path: string, force: boolean): Promise<MarkResolvedOutcome>;
```

- [ ] **Step 1: Write the failing client tests**

`tauriRepoClient.test.ts`:

```ts
it("sends mainline (null by default) and the sequence/conflict methods", async () => {
  vi.mocked(invoke).mockResolvedValue({ kind: "Committed", commitId: "c" });
  await tauriRepoClient.cherryPick("/r", "a");
  expect(invoke).toHaveBeenLastCalledWith("cherry_pick", { repoPath: "/r", commitId: "a", mainline: null });
  await tauriRepoClient.revertCommit("/r", "m", 2);
  expect(invoke).toHaveBeenLastCalledWith("revert_commit", { repoPath: "/r", commitId: "m", mainline: 2 });

  vi.mocked(invoke).mockResolvedValue({ kind: "Completed", applied: 2, skipped: [] });
  await tauriRepoClient.cherryPickMany("/r", ["a", "b"]);
  expect(invoke).toHaveBeenLastCalledWith("cherry_pick_many", { repoPath: "/r", commitIds: ["a", "b"] });
  await tauriRepoClient.revertMany("/r", ["a"]);
  expect(invoke).toHaveBeenLastCalledWith("revert_many", { repoPath: "/r", commitIds: ["a"] });
  await tauriRepoClient.continueSequence("/r");
  expect(invoke).toHaveBeenLastCalledWith("continue_sequence", { repoPath: "/r" });
  vi.mocked(invoke).mockResolvedValue({ rewound: true });
  await expect(tauriRepoClient.abortSequence("/r")).resolves.toEqual({ rewound: true });
  expect(invoke).toHaveBeenLastCalledWith("abort_sequence", { repoPath: "/r" });

  vi.mocked(invoke).mockResolvedValue(null);
  await tauriRepoClient.resolveWholeFile("/r", "a.txt", "Ours");
  expect(invoke).toHaveBeenLastCalledWith("resolve_whole_file", { repoPath: "/r", path: "a.txt", side: "Ours" });
  vi.mocked(invoke).mockResolvedValue({ kind: "MarkersRemain" });
  await expect(tauriRepoClient.markResolved("/r", "a.txt", false)).resolves.toEqual({ kind: "MarkersRemain" });
  expect(invoke).toHaveBeenLastCalledWith("mark_resolved", { repoPath: "/r", path: "a.txt", force: false });
});
```

`vscodeRepoClient.test.ts`: the same calls through `postMessage`/`respond`, asserting the `method` and `params` of each request (copy the "wires searchCommits and cancelSearch" pattern; one `respond(id, …)` per call). Update the existing `cherryPick`/`revertCommit` expectations to include `mainline: null`.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/ipc`
Expected: FAIL.

- [ ] **Step 3: Implement**

Both clients use `loggedInvoke`/`call` like their neighbors and send `mainline: mainline ?? null`. Doc comments in `RepoClient.ts`: ids are oldest-first, revert applies newest-first, and `markResolved` with `force: false` never stages a file that still has conflict markers.

- [ ] **Step 4: Fix full `RepoClient` fixtures**

Run: `cd frontend && pnpm exec tsc --noEmit -p .` and add to each failing literal: `cherryPickMany: async () => ({ kind: "Completed", applied: 0, skipped: [] }), revertMany: async () => ({ kind: "Completed", applied: 0, skipped: [] }), continueSequence: async () => ({ kind: "Completed", applied: 0, skipped: [] }), abortSequence: async () => ({ rewound: true }), resolveWholeFile: async () => {}, markResolved: async () => ({ kind: "Resolved" }),` (in `App.test.tsx`'s `fakeClient` use `unused` for the mutating ones). Add `sequence: null` to every `HeadInfo` literal (`useAppState` initial state included). Repeat until `tsc` is clean.

- [ ] **Step 5: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): RepoClient sequences, mainline picks, whole-file resolution and mark resolved"
```

---

### Task A8: Conflict pane header — whole file and Mark resolved

**Files:**
- Modify: `frontend/src/components/ConflictResolutionPane.tsx`, `.module.css`, `frontend/src/state/useMergeRebaseActions.ts` (+ its test or `useAppState.test.ts`), `frontend/src/components/DiffPane.tsx` (thread two props from `UncommittedFileSection` up to `DiffPane`), `frontend/src/App.tsx` (pass `appState.resolveWholeFile` / `appState.markResolved`)
- Test: `frontend/src/components/ConflictResolutionPane.test.tsx`

**Interfaces:**
- `ConflictResolutionPane` props `onResolveWholeFile: (path: string, side: ConflictSide) => void` and `onMarkResolved: (path: string, force: boolean) => Promise<MarkResolvedOutcome | null>`.
- `useMergeRebaseActions`: `resolveWholeFile(path, side): Promise<void>` (via `runMutation`), `markResolved(path, force): Promise<MarkResolvedOutcome | null>` (runs inside `runMutation`, captures the outcome in a local variable, returns `null` when the mutation failed).

- [ ] **Step 1: Write the failing tests**

```tsx
function renderPane(overrides: Partial<RepoClient> = {}, props: Partial<ComponentProps<typeof ConflictResolutionPane>> = {}) {
  const onResolveWholeFile = vi.fn();
  const onMarkResolved = vi.fn(async () => ({ kind: "Resolved" }) as MarkResolvedOutcome);
  render(
    <ConflictResolutionPane
      repoPath={TEST_REPO_PATH}
      client={fakeClient({ getConflictHunks: async () => [{ kind: "Conflict", ours: "a\n", theirs: "b\n" }], ...overrides })}
      path="shared.txt"
      onResolve={vi.fn()}
      onResolveAddDelete={vi.fn()}
      onResolveWholeFile={onResolveWholeFile}
      onMarkResolved={onMarkResolved}
      {...props}
    />,
  );
  return { onResolveWholeFile, onMarkResolved, ...props };
}

it("offers whole-file ours/theirs in the header", async () => {
  const { onResolveWholeFile } = renderPane();
  const ours = await screen.findByRole("button", { name: "Use ours (whole file)" });
  expect(ours).toHaveAttribute("title", "Replace the file with the current branch's version (HEAD).");
  fireEvent.click(ours);
  fireEvent.click(screen.getByRole("button", { name: "Use theirs (whole file)" }));
  expect(onResolveWholeFile.mock.calls).toEqual([["shared.txt", "Ours"], ["shared.txt", "Theirs"]]);
});

it("asks before marking a file with conflict markers resolved", async () => {
  const onMarkResolved = vi
    .fn()
    .mockResolvedValueOnce({ kind: "MarkersRemain" })
    .mockResolvedValueOnce({ kind: "Resolved" });
  renderPane({}, { onMarkResolved });
  fireEvent.click(await screen.findByRole("button", { name: "Mark resolved" }));
  expect(onMarkResolved).toHaveBeenLastCalledWith("shared.txt", false);
  expect(
    await screen.findByText("shared.txt still contains conflict markers. Mark it resolved anyway?"),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Mark resolved anyway" }));
  await waitFor(() => expect(onMarkResolved).toHaveBeenLastCalledWith("shared.txt", true));
  await waitFor(() => expect(screen.queryByText(/still contains conflict markers/)).toBeNull());
});

it("keeps the header for add/delete and other unparseable conflicts", async () => {
  renderPane({ getConflictHunks: async () => Promise.reject("'x' is an add/delete conflict, not a text conflict") });
  expect(await screen.findByRole("button", { name: "Keep our version" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Mark resolved" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Use theirs (whole file)" })).toBeInTheDocument();
});

it("keeps the header when hunks fail for another reason (binary conflict)", async () => {
  renderPane({ getConflictHunks: async () => Promise.reject("invalid utf-8") });
  expect(await screen.findByRole("alert")).toHaveTextContent("invalid utf-8");
  expect(screen.getByRole("button", { name: "Use ours (whole file)" })).toBeInTheDocument();
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/ConflictResolutionPane`
Expected: FAIL.

- [ ] **Step 3: Implement**

- A `header` element: `<Toolbar aria-label="Whole file">` with the three buttons (the two whole-file buttons carry their `title`s), rendered at the top of every branch of the pane (text, add/delete, error). All three are disabled until `loaded`, like the existing buttons.
- Mark resolved: `const outcome = await onMarkResolved(path, false); if (outcome?.kind === "MarkersRemain") setConfirmMarkers(true);`. Show the existing `ConfirmDialog` primitive (the one `DiffPane` uses for discards; import from where it lives) with `ariaLabel="Mark resolved"`, the message from Global Constraints, `confirmLabel="Mark resolved anyway"`, `onConfirm={() => { setConfirmMarkers(false); void onMarkResolved(path, true); }}`.
- `useMergeRebaseActions`: the two actions, calling `client.resolveWholeFile` / `client.markResolved`. Thread them through `useAppState`'s result, `App.tsx` → `DiffPane` → `UncommittedDiffPane` → `UncommittedFileSection` → the pane (the same path `onResolveAddDeleteConflict` takes).

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): whole-file ours/theirs and mark resolved in the conflict pane"
```

---

### Task A9: Graph — parent picker, multi-select pick/revert, outcome toasts

**Files:**
- Create: `frontend/src/components/ParentPickerDialog.tsx`, `.module.css`, `.test.tsx`
- Modify: `frontend/src/components/CommitGraph.tsx` (+ test), `frontend/src/state/useHistoryActions.ts` (+ test), `frontend/src/lib/operationStatus.ts` (+ test), `frontend/src/App.tsx`

**Interfaces:**
- `ParentPickerDialog({ action: "Cherry-pick" | "Revert", commit: GraphCommit, commits: GraphCommit[], onConfirm(mainline: number), onCancel() })`.
- `CommitGraph` props: `onCherryPick(commitId, mainline: number | null)`, `onRevert(commitId, mainline: number | null)`, new `onCherryPickMany(idsOldestFirst: string[])`, `onRevertMany(idsOldestFirst: string[])`.
- `useHistoryActions`: `cherryPick(id, mainline?) / revertCommit(id, mainline?) : Promise<PickOutcome | null>`, `cherryPickMany(ids) / revertMany(ids) / continueSequence() : Promise<SequenceOutcome | null>`, `abortSequence(): Promise<SequenceAbort | null>`.
- `operationStatus.ts`: `sequenceLabel(p: SequenceProgress): string`, `pickToast(kind: SequenceKind, outcome: PickOutcome): string | null`, `sequenceToast(kind: SequenceKind, outcome: SequenceOutcome): string | null`, `abortToast(kind, abort: SequenceAbort): string`.

- [ ] **Step 1: Write the failing tests**

`operationStatus.test.ts`:

```ts
it("labels sequences and builds toasts from outcomes", () => {
  expect(sequenceLabel({ kind: "CherryPick", currentStep: 2, totalSteps: 5, currentCommit: "c" })).toBe("Cherry-picking 2/5");
  expect(sequenceLabel({ kind: "Revert", currentStep: 1, totalSteps: 3, currentCommit: "c" })).toBe("Reverting 1/3");
  expect(pickToast("CherryPick", { kind: "Committed", commitId: "c" })).toBe("Cherry-picked");
  expect(pickToast("Revert", { kind: "Committed", commitId: "c" })).toBe("Reverted");
  expect(pickToast("CherryPick", { kind: "Conflicted", files: ["a"] })).toBeNull();
  expect(sequenceToast("CherryPick", { kind: "Completed", applied: 3, skipped: [] })).toBe("Cherry-picked 3 commits");
  expect(sequenceToast("Revert", { kind: "Completed", applied: 1, skipped: [] })).toBe("Reverted 1 commit");
  expect(sequenceToast("CherryPick", { kind: "Completed", applied: 2, skipped: ["x"] })).toBe(
    "Cherry-picked 2 commits, skipped 1 already applied",
  );
  expect(
    sequenceToast("CherryPick", {
      kind: "Conflicted",
      files: ["a"],
      progress: { kind: "CherryPick", currentStep: 2, totalSteps: 3, currentCommit: "c" },
    }),
  ).toBeNull();
  expect(abortToast("CherryPick", { rewound: true })).toBe("Aborted cherry-pick");
  expect(abortToast("Revert", { rewound: false })).toBe(
    "Stopped the revert. HEAD had moved, so it was left where it is.",
  );
});
```

`ParentPickerDialog.test.tsx`:

```tsx
it("lists parents, defaults to parent 1, and confirms the chosen mainline", () => {
  const merge = { ...commit("m"), parentIds: ["p1", "p2"] };
  const onConfirm = vi.fn();
  render(
    <ParentPickerDialog
      action="Revert"
      commit={merge}
      commits={[merge, { ...commit("p1"), summary: "main work", branchRefs: ["main"] }]}
      onConfirm={onConfirm}
      onCancel={vi.fn()}
    />,
  );
  const dialog = screen.getByRole("dialog", { name: "Choose parent" });
  const group = within(dialog).getByRole("radiogroup", { name: "Mainline parent" });
  const radios = within(group).getAllByRole("radio");
  expect(radios).toHaveLength(2);
  expect(radios[0]).toBeChecked();
  expect(within(group).getByText(/main work/)).toBeInTheDocument();
  expect(within(dialog).getByText("Parent 1 is usually the branch that was merged into.")).toBeInTheDocument();
  fireEvent.click(radios[1]);
  fireEvent.click(within(dialog).getByRole("button", { name: "Revert" }));
  expect(onConfirm).toHaveBeenCalledWith(2);
});
```

(`commit(id)` builds a `GraphCommit` literal like `CommitSearchResults.test.tsx`'s `c(id)`.)

`CommitGraph.test.tsx` (use the file's `setup`/render helpers):

```tsx
it("opens the parent picker for a merge commit and passes the mainline", async () => {
  // commits: merge "m" (parents a, b) plus a and b; HEAD elsewhere
  const { onCherryPick } = setupWithMerge();
  openContextMenu("m");
  fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick…" }));
  fireEvent.click(within(screen.getByRole("dialog", { name: "Choose parent" })).getByRole("button", { name: "Cherry-pick" }));
  expect(onCherryPick).toHaveBeenCalledWith("m", 1);
});

it("passes a null mainline for a normal commit", () => {
  const { onCherryPick } = setupWithMerge();
  openContextMenu("a");
  fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick" }));
  expect(onCherryPick).toHaveBeenCalledWith("a", null);
});

it("offers Cherry-pick and Revert N commits for a multi-selection, oldest first", () => {
  const { onCherryPickMany, onRevertMany } = setupLinear(); // c3 (newest), c2, c1
  selectWithCtrl(["c3", "c1"]);
  const toolbar = screen.getByRole("toolbar", { name: "Selected commits" });
  fireEvent.click(within(toolbar).getByRole("button", { name: "Cherry-pick 2 commits" }));
  expect(onCherryPickMany).toHaveBeenCalledWith(["c1", "c3"]);
  fireEvent.click(within(toolbar).getByRole("button", { name: "Revert 2 commits" }));
  expect(onRevertMany).toHaveBeenCalledWith(["c1", "c3"]);
});

it("disables multi pick/revert when the selection holds a merge commit", () => {
  setupWithMerge();
  selectWithCtrl(["m", "a"]);
  const toolbar = screen.getByRole("toolbar", { name: "Selected commits" });
  const pick = within(toolbar).getByRole("button", { name: "Cherry-pick 2 commits" });
  expect(pick).toBeDisabled();
  expect(pick).toHaveAttribute("title", "Cherry-pick merge commits one at a time.");
  expect(within(toolbar).getByRole("button", { name: "Revert 2 commits" })).toHaveAttribute(
    "title",
    "Revert merge commits one at a time.",
  );
});
```

Write `setupWithMerge`/`setupLinear`/`openContextMenu`/`selectWithCtrl` from the file's existing helpers (`setup` at line ~1017 and the existing context-menu and ctrl-click tests). Check whether context menu items have role `menuitem` or `button` in `ContextMenu` and use what the existing tests use.

`useHistoryActions` (in `useAppState.test.ts` or its own test): `cherryPick` resolves to the client's outcome; `cherryPickMany` calls `client.cherryPickMany(repoPath, ids)` and resolves to its outcome; a rejecting client resolves `null` and sets `state.error`.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/operationStatus src/components/ParentPicker src/components/CommitGraph src/state`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `ParentPickerDialog` uses the same overlay/dialog primitive as `ResetDialog` (`role="dialog"`, `aria-label="Choose parent"`, focus the checked radio on open, Esc cancels). Rows: `Parent {n}` + `<code>{shortId}</code>` + the summary when `commits` has the parent + branch badges.
- `CommitGraph.singleCommitItems`: for `isMerge`, label `Cherry-pick…` / `Revert…` and `onSelect` → `setParentPicker({ action, commit })`; non-merges call `onCherryPick(id, null)` / `onRevert(id, null)`. Keep `disabled: pending || isHead` for cherry-pick.
- `multiSelectItems`: computed for any `multiSelected` selection (not only when `rebasePlan.ok`). Append after the existing items (and after any 4d compare item):
  - `idsOldestFirst` = selected ids in `commits` order, reversed (same rule as `planRebaseSelection`).
  - `hasMerge = selected.some(c => c.parentIds.length > 1)`.
  - `{ label: \`Cherry-pick ${count} commits\`, onSelect: () => onCherryPickMany(idsOldestFirst), disabled: pending || hasMerge, title: hasMerge ? "Cherry-pick merge commits one at a time." : undefined }`, and the same for Revert.
  - Keep the rebase items' blocked rendering for `!rebasePlan.ok` unchanged.
- `useHistoryActions`: implement the signatures above. Each runs through `runMutation`, captures the client result in a local variable, and returns it (or `null` if `runMutation` caught an error).
- `App.tsx`: drop `announce("Cherry-picked", …)` / `announce("Reverted", …)`. Add a `runPickAction` helper: `dismissError()`, await the action, compute the toast with `pickToast`/`sequenceToast` (kind from the action), and `pushToast` when it is non-null and the workspace is still mounted (reuse the `mounted` ref). Wire `onCherryPick`, `onRevert`, `onCherryPickMany`, `onRevertMany` through it.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): parent picker for merge commits, multi-commit cherry-pick/revert, outcome toasts"
```

---

### Task A10: Sequence progress — strip, commit dock, gates, palette

**Files:**
- Modify: `frontend/src/components/OperationStatusStrip.tsx` (+ test), `RebaseProgressPanel.tsx` (+ test), `DiffPane.tsx` (+ test), `App.tsx` (+ test), `lib/commands.ts` (+ test)

**Interfaces:**
- `OperationStatusStrip` new props: `sequence: SequenceProgress | null`, `onContinueSequence: () => void`, `onAbortSequence: () => void`.
- `RebaseProgressPanel` new optional props: `title = "Rebase in progress"`, `continueLabel = "Continue rebase"`, `abortLabel = "Abort rebase"`.
- `DiffPane` new props: `sequence: SequenceProgress | null`, `onSequenceContinue`, `onSequenceAbort`.

- [ ] **Step 1: Write the failing tests**

`OperationStatusStrip.test.tsx`:

```tsx
it("shows sequence progress with Continue and Abort", () => {
  const onContinueSequence = vi.fn();
  const onAbortSequence = vi.fn();
  const onAbortMerge = vi.fn();
  render(
    <OperationStatusStrip
      merging
      operation="CherryPick"
      rebaseProgress={null}
      sequence={{ kind: "CherryPick", currentStep: 2, totalSteps: 5, currentCommit: "c" }}
      conflictCount={1}
      onAbortMerge={onAbortMerge}
      onAbortRebase={vi.fn()}
      onContinueSequence={onContinueSequence}
      onAbortSequence={onAbortSequence}
    />,
  );
  const strip = screen.getByRole("status", { name: "Operation status" });
  expect(strip).toHaveTextContent("Cherry-picking 2/5");
  const cont = within(strip).getByRole("button", { name: "Continue" });
  expect(cont).toBeDisabled();
  expect(cont).toHaveAttribute("title", "Resolve 1 conflict to continue");
  fireEvent.click(within(strip).getByRole("button", { name: "Abort cherry-pick" }));
  expect(onAbortSequence).toHaveBeenCalled();
  expect(onAbortMerge).not.toHaveBeenCalled();
});

it("shows a sequence stopped by an error (no merge state) and enables Continue", () => {
  const onContinueSequence = vi.fn();
  render(
    <OperationStatusStrip
      merging={false}
      operation={null}
      rebaseProgress={null}
      sequence={{ kind: "Revert", currentStep: 3, totalSteps: 3, currentCommit: "c" }}
      conflictCount={0}
      onAbortMerge={vi.fn()}
      onAbortRebase={vi.fn()}
      onContinueSequence={onContinueSequence}
      onAbortSequence={vi.fn()}
    />,
  );
  expect(screen.getByRole("status", { name: "Operation status" })).toHaveTextContent("Reverting 3/3");
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
  expect(onContinueSequence).toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Abort revert" })).toBeInTheDocument();
});
```

Update the existing strip tests to pass `sequence={null}` and the two new callbacks.

`RebaseProgressPanel.test.tsx`: renders custom `title`/`continueLabel`/`abortLabel`, and the defaults are unchanged.

`DiffPane.test.tsx`: with `sequence` set, the commit dock shows "Cherry-pick in progress", "Step 2 of 5", "Continue cherry-pick" (disabled while a `Conflicted` status entry exists) and "Abort cherry-pick", and no `CommitBox` (`queryByRole("textbox", { name: "Commit message" })` is null). The amend toggle is hidden when `headInfo.sequence !== null` and nothing is paused.

`commands.test.ts`: with `state.headInfo.sequence` set (`kind: "Revert"`), the palette has `Continue revert` and `Abort revert`, and running them calls `appState.continueSequence` / `appState.abortSequence`. With `sequence: null` neither appears.

`App.test.tsx`: when `getHeadInfo` returns a sequence, the strip renders "Cherry-picking 1/2", Continue calls `continueSequence`, and the graph's single-commit "Cherry-pick" menu item is disabled ("A cherry-pick is in progress." as the reason, via `operationDisabledReason`).

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/OperationStatusStrip src/components/RebaseProgressPanel src/components/DiffPane src/lib/commands src/App.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

- Strip: render when `merging || rebaseProgress !== null || sequence !== null`. With `sequence`, the label is `sequenceLabel(sequence)`, Continue is `disabled={conflictCount > 0}` with `title={conflictReason(conflictCount)}`, and Abort is `Abort cherry-pick`/`Abort revert` → `onAbortSequence`. Otherwise the current behavior is unchanged.
- `RebaseProgressPanel`: the three optional props.
- `DiffPane` dock: `sequence !== null` → `RebaseProgressPanel` with `currentStep/totalSteps` from the sequence and the sequence labels (`Cherry-pick in progress`/`Revert in progress`, `Continue cherry-pick`/`Continue revert`, `Abort cherry-pick`/`Abort revert`). `disabled={conflictCount > 0}`, `onContinue={onSequenceContinue}`, `onAbort={onSequenceAbort}`. Amend `available` also requires `headInfo.sequence === null`. The Stash button is disabled while a sequence is active (same reasoning as for rebase).
- `App.tsx`:
  - `repositoryOperationDisabled ||= appState.state.headInfo.sequence !== null`.
  - `operationDisabledReason`: check `headInfo.sequence` before the `mergeMessage` branch → `A cherry-pick is in progress.` / `A revert is in progress.`.
  - `onBusyChange` / tab-close guard: treat an active sequence like a rebase (`rebaseProgressRef` sibling for sequence).
  - Strip and `DiffPane` wiring: continue → `runPickAction` with `appState.continueSequence` (toast via `sequenceToast`), abort → `appState.abortSequence` then `pushToast(abortToast(kind, result))` when non-null. `onAbortMerge` on `DiffPane`/strip routes to `abortSequence` when `headInfo.sequence !== null`.
- `commands.ts`: when `state.headInfo.sequence` is set, push `{ id: "continue-sequence", label: \`Continue ${noun}\`, keywords: ["continue", "cherry", "pick", "revert", "sequence"], run: () => void appState.continueSequence() }` and `{ id: "abort-sequence", label: \`Abort ${noun}\`, … }` (`noun` = `cherry-pick` / `revert`). They stay available while other repository operations are disabled.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): cherry-pick/revert sequence progress with Continue and Abort"
```

---

### Task A11: Tauri E2E — three commits, one conflict, whole-file ours, continue

**Files:**
- Create: `e2e/specs/cherry-pick-sequence.spec.ts`

- [ ] **Step 1: Write the spec**

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const git = (...args: string[]) => execFileSync("git", args, { cwd: E2E_REPO_PATH }).toString().trim();

async function jsClick(selector: string) {
  const el = await $(selector);
  await el.waitForExist({ timeout: 10000 });
  await browser.execute((node) => (node as HTMLElement).click(), el);
}

describe("Browsitory cherry-pick sequence", () => {
  let baseBranch = "";
  let startTip = "";

  before(() => {
    baseBranch = git("rev-parse", "--abbrev-ref", "HEAD");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-shared.txt"), "base\n");
    git("add", "seq-shared.txt");
    git("commit", "-m", "e2e: seq base");
    git("checkout", "-b", "e2e-seq-source");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-one.txt"), "1\n");
    git("add", "seq-one.txt");
    git("commit", "-m", "e2e: seq one");
    // The middle commit conflicts on seq-shared.txt and also adds a file, so taking "ours" for
    // the shared file still leaves a non-empty commit (an empty one would be skipped).
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-shared.txt"), "theirs\n");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-two.txt"), "2\n");
    git("add", "seq-shared.txt", "seq-two.txt");
    git("commit", "-m", "e2e: seq two");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-three.txt"), "3\n");
    git("add", "seq-three.txt");
    git("commit", "-m", "e2e: seq three");
    git("checkout", baseBranch);
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-shared.txt"), "ours\n");
    git("commit", "-am", "e2e: seq ours");
    startTip = git("rev-parse", "HEAD");
  });

  it("cherry-picks three commits, resolves the conflict with whole-file ours, and continues", async () => {
    await browser.refresh();
    const first = await $("li*=e2e: seq one");
    await first.waitForExist({ timeout: 10000 });
    await first.click();
    // Synthetic ctrl-click: see rebase.spec.ts on why a real held modifier is avoided.
    for (const summary of ["e2e: seq two", "e2e: seq three"]) {
      const row = await $(`li*=${summary}`);
      await browser.execute((el) => {
        el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
      }, row);
    }
    await jsClick("button=Cherry-pick 3 commits");

    const strip = await $('[aria-label="Operation status"]');
    await strip.waitForExist({ timeout: 10000 });
    await expect(strip).toHaveText(expect.stringContaining("Cherry-picking 2/3"));

    const conflicted = await $("span*=seq-shared.txt (Conflicted)");
    await conflicted.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), conflicted);
    await jsClick("button=Use ours (whole file)");

    const cont = await $('[aria-label="Operation status"] button=Continue');
    await cont.waitForEnabled({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), cont);

    await browser.waitUntil(() => git("log", "-1", "--format=%s") === "e2e: seq three", { timeout: 10000 });
    expect(git("log", "--format=%s", `${startTip}..HEAD`).split("\n")).toEqual([
      "e2e: seq three",
      "e2e: seq two",
      "e2e: seq one",
    ]);
    expect(fs.readFileSync(path.join(E2E_REPO_PATH, "seq-shared.txt"), "utf8")).toBe("ours\n");
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "seq-two.txt"))).toBe(true);
    expect(fs.existsSync(path.join(E2E_REPO_PATH, ".git", "browsitory", "sequence.json"))).toBe(false);
    await expect($('[aria-label="Operation status"]')).not.toBeExisting();
  });
});
```

- [ ] **Step 2: Run the Tauri E2E**

Follow CLAUDE.md's "E2E: Tauri" sequence exactly (frontend build with `VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo`, `cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override`, then `cd e2e && pnpm install && pnpm test`), under `xvfb-run` on a headless machine. To iterate: `pnpm test -- --spec ./specs/cherry-pick-sequence.spec.ts`.
Expected: PASS, and the full run still passes (this spec adds commits and a branch to the shared fixture repo, like `history-rewrite.spec.ts`). If the "Selected commits" toolbar is below the fold, `jsClick` handles it. If a selector for the conflicted row differs (see `merge.spec.ts`), copy that spec's selector.

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/cherry-pick-sequence.spec.ts
git commit -m "test(e2e): cherry-pick three commits with a conflict resolved by whole-file ours"
```

---

### Task A12: Docs and verification (Part A)

**Files:**
- Modify: `CHANGELOG.md` (under `## [Unreleased]`, after the newest subsection), `docs/USER_GUIDE.md`, `docs/ARCHITECTURE.md` (new `## Cherry-pick sequences` after `## Commit search`)

- [ ] **Step 1: CHANGELOG**

```markdown
### Conflicts and picks

- The conflict pane has **Use ours (whole file)**, **Use theirs (whole file)** and **Mark
  resolved** (which asks first if the file still has conflict markers). They work for content,
  add/delete and binary conflicts, in merges, cherry-picks, reverts, rebases and stash pops.
- Select several commits in the graph to **Cherry-pick N commits** or **Revert N commits** as one
  operation. A conflict pauses it; the status strip shows "Cherry-picking 2/5" with **Continue**
  and **Abort**, and it survives an app restart. Commits whose changes are already on the branch
  are skipped. One undo step covers the whole sequence.
- Merge commits can be cherry-picked or reverted: **Cherry-pick…** / **Revert…** asks which
  parent to apply them against.
- Fixed: a conflicted cherry-pick or revert no longer shows a "Cherry-picked"/"Reverted" toast.
```

- [ ] **Step 2: USER_GUIDE**

- In "Rewriting and moving commits": replace "Merge commits can't be cherry-picked yet." / "can't be reverted yet." with how **Cherry-pick…**/**Revert…** pick a parent (parent 1 = the branch merged into). Add a paragraph **Several commits** covering: selection order does not matter, cherry-picks apply oldest first, reverts newest first, Continue/Abort in the status strip and the commit dock, empty commits skipped, the toast's skipped count, merge commits one at a time, Abort returning the branch to where it started (unless HEAD was moved elsewhere meanwhile), and one undo step.
- New `### Resolving conflicts` under "Staging and committing": per-hunk choices (existing behavior, one sentence), whole-file ours/theirs ("ours" = your current branch, HEAD; "theirs" = the commit being applied; during a rebase "ours" is the branch you are rebasing onto), Mark resolved after editing the file yourself, and the markers confirmation.

- [ ] **Step 3: ARCHITECTURE**

```markdown
## Cherry-pick sequences

- `git-core::pick` is a pure state machine: `cherry_pick_many`/`revert_many`/`continue_sequence`
  take and return a `SequenceState` (`SequenceRun { outcome, state }`); `git-core` has no
  persistence and no serde.
- `repo-service`'s `SequenceStore` persists it as `<gitdir>/browsitory/sequence.json`
  (versioned, temp file + rename, like `undo.json`). An unreadable file is ignored, never fatal.
- A stored sequence counts as "in progress" for undo (one pending entry, dropped on abort) and
  blocks other ref-moving commands in the worker; `GetHeadInfo` reports it as `HeadInfo.sequence`.
- A sequence stops on a conflict (git's own CHERRY_PICK_HEAD/REVERT_HEAD state) or on any later
  error; empty steps are skipped. Continue checks that HEAD is where the stop left it (or one
  commit further, if the user committed the step) before going on.
```

- [ ] **Step 4: Full verification and commit**

```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && pnpm build && cd ..
cd extension && pnpm test -- --run && pnpm run compile && cd ..
```
Expected: all PASS.

```bash
git add CHANGELOG.md docs/USER_GUIDE.md docs/ARCHITECTURE.md
git commit -m "docs: conflicts and picks in user guide, changelog and architecture"
```

Open the 4g-1 PR (title "Conflicts and picks (batch 4g-1)", body referencing #96 and the spec).

---

# Part B — 4g-2: Image diff and commit message polish

### Task B0: Pre-flight

- [ ] **Step 1: Branch after 4g-1 merged**

```bash
git fetch origin
git checkout -b feat/parity-batch4g-2-image-commit-polish origin/main
git log --oneline -5   # 4g-1 must be on main
```

- [ ] **Step 2: Re-check names**

```bash
graft skeleton crates/git-core/src/commit.rs   # commit/amend_head signatures after 4e (no_verify? hook calls?)
graft skeleton crates/git-core/src/diff.rs     # DiffError, commit_and_parent_trees
grep -n "struct ConfigFile" -A30 crates/config/src/lib.rs   # 4f's new tables? ordering comment
grep -rln "Preferences" frontend/src           # 4f's dialog component name and how sections are added
grep -n "Commit {" -A4 crates/repo-service/src/worker/mod.rs   # Command::Commit fields after 4e
grep -n "commit:\|amendCommit" frontend/src/ipc/RepoClient.ts
grep -n "img-src" extension/src/webviewHtml.ts crates/tauri-app/tauri.conf.json
```

- **4e:** if `commit`/`amend_head` gained `no_verify: bool` (or an options struct), add `trailers` (and `author` for amend) after it, or as fields of that struct. Wherever 4e calls `prepare-commit-msg`/`commit-msg`, call `append_trailers` **before** it. If 4e passes the message to hooks through a temp file, the appended message is what goes in.
- **4f:** find the Preferences dialog and its section pattern. If none exists, Task B8 creates `frontend/src/components/PreferencesDialog.tsx` (palette "Preferences…", `role="dialog"` aria-label `Preferences`) holding only the Commit message section.

- [ ] **Step 3: Baseline is green**

```bash
cargo test --workspace && cd frontend && pnpm test -- --run && cd ..
```

---

### Task B1: `git-core::diff::file_sides`

**Files:**
- Modify: `crates/git-core/src/diff.rs`
- Test: `crates/git-core/tests/diff.rs`

**Interfaces:**
- Produces: `MAX_PREVIEW_BYTES`, `SidesTarget<'a>`, `FileSide`, `FileSides` (all `Debug, Clone, PartialEq, Eq`), `file_sides(repo, target, path, old_path, cap)`, `DiffError::{Io, InvalidPath}`.

- [ ] **Step 1: Write the failing tests**

```rust
use git_core::diff::{file_sides, FileSide, SidesTarget, MAX_PREVIEW_BYTES};

fn loaded(bytes: &[u8]) -> FileSide {
    FileSide::Loaded { bytes: bytes.to_vec() }
}

#[test]
fn file_sides_for_unstaged_staged_and_commit_targets() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "img.png", "v1");
    common::commit_all(&repo, "v1");
    let v1 = repo.head().unwrap().target().unwrap().to_string();
    common::write_file(dir.path(), "img.png", "v2");
    common::commit_all(&repo, "v2");
    let v2 = repo.head().unwrap().target().unwrap().to_string();
    common::write_file(dir.path(), "img.png", "v3");
    git_core::stage::stage_file(&repo, "img.png").unwrap();
    common::write_file(dir.path(), "img.png", "v4");

    let unstaged = file_sides(&repo, SidesTarget::Working { staged: false }, "img.png", None, MAX_PREVIEW_BYTES).unwrap();
    assert_eq!((unstaged.old, unstaged.new), (loaded(b"v3"), loaded(b"v4")));
    let staged = file_sides(&repo, SidesTarget::Working { staged: true }, "img.png", None, MAX_PREVIEW_BYTES).unwrap();
    assert_eq!((staged.old, staged.new), (loaded(b"v2"), loaded(b"v3")));
    let commit = file_sides(&repo, SidesTarget::Commit { commit_id: &v2 }, "img.png", None, MAX_PREVIEW_BYTES).unwrap();
    assert_eq!((commit.old, commit.new), (loaded(b"v1"), loaded(b"v2")));
    let root = file_sides(&repo, SidesTarget::Commit { commit_id: &v1 }, "img.png", None, MAX_PREVIEW_BYTES).unwrap();
    assert_eq!((root.old, root.new), (FileSide::Missing, loaded(b"v1")));
}

#[test]
fn file_sides_reports_added_deleted_and_renamed_files() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "old.png", "pixels");
    common::commit_all(&repo, "add");
    common::write_file(dir.path(), "new.png", "fresh");
    let untracked = file_sides(&repo, SidesTarget::Working { staged: false }, "new.png", None, MAX_PREVIEW_BYTES).unwrap();
    assert_eq!((untracked.old, untracked.new), (FileSide::Missing, loaded(b"fresh")));
    std::fs::remove_file(dir.path().join("old.png")).unwrap();
    let deleted = file_sides(&repo, SidesTarget::Working { staged: false }, "old.png", None, MAX_PREVIEW_BYTES).unwrap();
    assert_eq!((deleted.old, deleted.new), (loaded(b"pixels"), FileSide::Missing));

    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.png", "same");
    common::commit_all(&repo, "a");
    std::fs::rename(dir.path().join("a.png"), dir.path().join("b.png")).unwrap();
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new("a.png")).unwrap();
    index.add_path(std::path::Path::new("b.png")).unwrap();
    index.write().unwrap();
    common::commit_all(&repo, "rename");
    let id = repo.head().unwrap().target().unwrap().to_string();
    let renamed = file_sides(&repo, SidesTarget::Commit { commit_id: &id }, "b.png", Some("a.png"), MAX_PREVIEW_BYTES).unwrap();
    assert_eq!((renamed.old, renamed.new), (loaded(b"same"), loaded(b"same")));
}

#[test]
fn a_side_over_the_cap_is_too_large_with_its_size() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "big.png", "0123456789");
    common::commit_all(&repo, "big");
    common::write_file(dir.path(), "big.png", "0123456789abcdef");
    let sides = file_sides(&repo, SidesTarget::Working { staged: false }, "big.png", None, 12).unwrap();
    assert_eq!(sides.old, loaded(b"0123456789"));
    assert_eq!(sides.new, FileSide::TooLarge { size: 16 });
}

#[test]
fn file_sides_rejects_paths_outside_the_repository() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.png", "x");
    common::commit_all(&repo, "a");
    for bad in ["../secret.png", "/etc/passwd", "a/../../b.png"] {
        assert!(
            matches!(
                file_sides(&repo, SidesTarget::Working { staged: false }, bad, None, MAX_PREVIEW_BYTES),
                Err(git_core::diff::DiffError::InvalidPath(_))
            ),
            "{bad}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_symlinked_working_file_is_missing() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "keep.txt", "k");
    common::commit_all(&repo, "base");
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("secret.png"), "secret").unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.png"), dir.path().join("link.png")).unwrap();
    let sides = file_sides(&repo, SidesTarget::Working { staged: false }, "link.png", None, MAX_PREVIEW_BYTES).unwrap();
    assert_eq!(sides.new, FileSide::Missing);
}
```

(If `tests/diff.rs` lacks `tempfile` in scope, it is a dev-dependency of `git-core`; check `[dev-dependencies]` and add `tempfile` there only if it is missing, since it is already a runtime dependency and needs no LICENSE row.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test diff`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

- `fn check_relative(path) -> Result<(), DiffError>`: reject `Path::is_absolute()`, any `Component::ParentDir`/`Prefix`/`RootDir`, and the empty string. The message is `"path must be relative to the repository: {path}"`.
- `fn blob_contents(repo, oid, cap) -> Result<FileSide, DiffError>`: `let (size, _) = repo.odb()?.read_header(oid)?;` if `size as u64 > cap` → `TooLarge { size }`, else `Loaded { bytes: repo.find_blob(oid)?.content().to_vec() }`.
- `fn tree_side(repo, tree: Option<&Tree>, path, cap)`: `tree.get_path(path)` → a blob entry → `blob_contents`; not found or not a blob → `Missing`.
- `fn index_side(repo, path, cap)`: `repo.index()?.get_path(path, 0)` → `blob_contents`; none → `Missing`.
- `fn workdir_side(repo, path, cap)`: `symlink_metadata`; `NotFound` or a symlink or not a file → `Missing`; `len() > cap` → `TooLarge { size: len }`; else `Loaded { bytes: fs::read }`.
- `file_sides`: `check_relative` both paths, then match the target (spec "Sides per target"). For `Commit`, reuse `commit_and_parent_trees` (old = parent tree or `None` for a root).

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test diff`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all -- --check
git add crates/git-core
git commit -m "feat(git-core): file_sides returns both sides of a diff as bytes, capped for preview"
```

---

### Task B2: `git-core::commit` — trailers, author override, recent authors

**Files:**
- Modify: `crates/git-core/src/commit.rs`; every caller of `commit::commit(`/`amend_head(` (`grep -rn "commit::commit(\|amend_head(" crates`), passing `&[]` / `None, &[]` for now
- Test: inline in `commit.rs`, `crates/git-core/tests/amend.rs`, `crates/git-core/tests/stage_commit.rs`, new `crates/git-core/tests/recent_authors.rs`

**Interfaces:**
- Produces: `SignatureInput { pub name: String, pub email: String }`, `Trailer { pub key: String, pub value: String }` (both `Debug, Clone, PartialEq, Eq`), `append_trailers`, `commit(repo, message, trailers)`, `amend_head(repo, message, author, trailers)`, `recent_authors(repo, limit)`, `CommitError::{InvalidTrailer(String), InvalidAuthor}`.

- [ ] **Step 1: Write the failing tests**

Inline in `commit.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn co(value: &str) -> Trailer {
        Trailer { key: "Co-authored-by".into(), value: value.into() }
    }

    #[test]
    fn no_trailers_leaves_the_message_unchanged() {
        assert_eq!(append_trailers("Fix bug  \n\n", &[]).unwrap(), "Fix bug  \n\n");
    }

    #[test]
    fn trailers_go_after_a_blank_line() {
        assert_eq!(
            append_trailers("Fix bug\n", &[co("Ada <ada@x.org>")]).unwrap(),
            "Fix bug\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
        assert_eq!(
            append_trailers("Fix bug\n\nBody text.\n\n\n", &[co("Ada <ada@x.org>"), co("Bob <bob@x.org>")]).unwrap(),
            "Fix bug\n\nBody text.\n\nCo-authored-by: Ada <ada@x.org>\nCo-authored-by: Bob <bob@x.org>\n"
        );
    }

    #[test]
    fn an_existing_trailer_block_is_extended_and_duplicates_skipped() {
        assert_eq!(
            append_trailers("Fix\n\nBody\n\nSigned-off-by: Me <me@x.org>\n", &[co("Ada <ada@x.org>")]).unwrap(),
            "Fix\n\nBody\n\nSigned-off-by: Me <me@x.org>\nCo-authored-by: Ada <ada@x.org>\n"
        );
        assert_eq!(
            append_trailers("Fix\n\nCo-authored-by: Ada <ada@x.org>\n", &[co("Ada <ada@x.org>")]).unwrap(),
            "Fix\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
    }

    #[test]
    fn a_one_paragraph_message_that_looks_like_a_trailer_is_still_the_subject() {
        assert_eq!(
            append_trailers("fix: crash on start\n", &[co("Ada <ada@x.org>")]).unwrap(),
            "fix: crash on start\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
    }

    #[test]
    fn invalid_trailers_are_refused() {
        for (key, value) in [("Bad key", "v"), ("", "v"), ("-lead", "v"), ("Key", "  "), ("Key", "a\nb")] {
            let t = Trailer { key: key.into(), value: value.into() };
            assert!(matches!(append_trailers("x", &[t]), Err(CommitError::InvalidTrailer(_))), "{key:?} {value:?}");
        }
    }
}
```

`tests/amend.rs`:

```rust
#[test]
fn amend_with_an_author_override_keeps_the_committer_and_the_original_author_time() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let before = repo.head().unwrap().peel_to_commit().unwrap();
    let original_time = before.author().when();
    drop(before);

    let author = git_core::commit::SignatureInput { name: "Ada Lovelace".into(), email: "ada@x.org".into() };
    git_core::commit::amend_head(&repo, "first, reworded", Some(&author), &[]).unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.author().name(), Some("Ada Lovelace"));
    assert_eq!(head.author().email(), Some("ada@x.org"));
    assert_eq!(head.author().when().seconds(), original_time.seconds());
    assert_eq!(head.author().when().offset_minutes(), original_time.offset_minutes());
    assert_eq!(head.committer().name(), Some("Test User"));
    assert_eq!(head.message(), Some("first, reworded"));
}

#[test]
fn amend_appends_trailers_and_refuses_an_invalid_author() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let trailers = [git_core::commit::Trailer { key: "Co-authored-by".into(), value: "Bob <bob@x.org>".into() }];
    git_core::commit::amend_head(&repo, "first", None, &trailers).unwrap();
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().message(),
        Some("first\n\nCo-authored-by: Bob <bob@x.org>\n")
    );
    for (name, email) in [("", "a@x.org"), ("Ada", ""), ("A<da", "a@x.org"), ("Ada", "a@x\n.org")] {
        let author = git_core::commit::SignatureInput { name: name.into(), email: email.into() };
        assert!(matches!(
            git_core::commit::amend_head(&repo, "first", Some(&author), &[]),
            Err(git_core::commit::CommitError::InvalidAuthor)
        ));
    }
}
```

`tests/stage_commit.rs`, one test: `commit(&mut repo, "Add a", &[co])` → the message ends with the trailer.

`tests/recent_authors.rs`:

```rust
mod common;

fn commit_as(repo: &git2::Repository, dir: &std::path::Path, name: &str, email: &str, file: &str) {
    common::write_file(dir, file, name);
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(file)).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now(name, email).unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, file, &tree, &parents).unwrap();
}

#[test]
fn recent_authors_are_deduped_by_email_newest_first_without_me() {
    let (dir, repo) = common::init_repo();
    commit_as(&repo, dir.path(), "Ada", "ada@x.org", "1");
    commit_as(&repo, dir.path(), "Test User", "test@example.com", "2");
    commit_as(&repo, dir.path(), "Bob", "bob@x.org", "3");
    commit_as(&repo, dir.path(), "Ada L.", "ADA@x.org", "4");
    let authors = git_core::commit::recent_authors(&repo, 20).unwrap();
    let names: Vec<_> = authors.iter().map(|a| (a.name.as_str(), a.email.as_str())).collect();
    assert_eq!(names, [("Ada L.", "ADA@x.org"), ("Bob", "bob@x.org")]);
    assert_eq!(git_core::commit::recent_authors(&repo, 1).unwrap().len(), 1);
}

#[test]
fn recent_authors_of_an_unborn_repo_is_empty() {
    let (_dir, repo) = common::init_repo();
    assert!(git_core::commit::recent_authors(&repo, 20).unwrap().is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --lib commit::tests && cargo test -p git-core --test amend --test stage_commit --test recent_authors`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

- `append_trailers`: if `trailers` is empty, return `message.to_string()`. Validate every trailer: key matches `^[A-Za-z0-9][A-Za-z0-9-]*$` (manual char check, no regex crate), `value.trim()` non-empty, and the value has no `\n`/`\r`. Otherwise `InvalidTrailer(format!("{key}: {value}"))` with the Display text `Invalid trailer "{0}".`. Then `let body = message.trim_end();` and `let paragraphs: Vec<&str> = body.split("\n\n").collect();`. The body ends in a trailer block when `paragraphs.len() >= 2` and every non-empty line of the last paragraph `is_trailer_line`: `Key: value`, where the key matches the key rule and is followed by `": "`. New lines are `format!("{}: {}", key, value.trim())`, skipping those already present in the block (or added earlier in this call). The result is `body + "\n" + new` when the block exists, else `body + "\n\n" + new`, plus a final `"\n"`. An empty `body` gives just the lines + `"\n"`.
- `commit(repo, message, trailers)`: `let message = append_trailers(message, trailers)?;` at the top (before 4e's hooks, if present), then unchanged.
- `amend_head(repo, message, author, trailers)`: append trailers the same way. Validate `author` (`InvalidAuthor` with the Display text `Invalid author. Use a name and an email address.`): trimmed non-empty name and email, and no `<`, `>`, `\n` or `\r` in either. The author signature is `git2::Signature::new(&a.name.trim(), &a.email.trim(), &head.author().when())` when overridden, else `head.author()`.
- `recent_authors(repo, limit)`: unborn HEAD → `Ok(vec![])`. Revwalk `push_head`, `TOPOLOGICAL | TIME`, take 1,000. `me = repo.signature().ok().and_then(|s| s.email().map(str::to_lowercase))`. Keep the first occurrence per lowercase email, skip `me`, and stop at `limit`.
- Update the callers to `commit(&mut repo, &message, &[])` / `amend_head(&repo, &message, None, &[])` (Task B4 threads the real values).

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core && cargo build --workspace`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check
git add crates
git commit -m "feat(git-core): commit trailers, author override on amend, recent authors"
```

---

### Task B3: `config` — commit message settings

**Files:**
- Modify: `crates/config/src/lib.rs`
- Create: `crates/config/tests/commit_message_settings.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use config::{get_commit_message_settings_at, set_commit_message_settings_at, CommitMessageSettings};

#[test]
fn defaults_round_trip_and_clamp() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    assert_eq!(
        get_commit_message_settings_at(&file).unwrap(),
        CommitMessageSettings { summary_limit: 50, body_ruler: 72, show_ruler: true }
    );
    set_commit_message_settings_at(&file, &CommitMessageSettings { summary_limit: 60, body_ruler: 80, show_ruler: false }).unwrap();
    assert_eq!(
        get_commit_message_settings_at(&file).unwrap(),
        CommitMessageSettings { summary_limit: 60, body_ruler: 80, show_ruler: false }
    );
    set_commit_message_settings_at(&file, &CommitMessageSettings { summary_limit: 1, body_ruler: 999, show_ruler: true }).unwrap();
    assert_eq!(
        get_commit_message_settings_at(&file).unwrap(),
        CommitMessageSettings { summary_limit: 10, body_ruler: 200, show_ruler: true }
    );
}

#[test]
fn commit_message_settings_coexist_with_diff_settings_and_lists() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    config::add_recent_repo_at(&file, std::path::Path::new("/r")).unwrap();
    config::set_diff_settings_at(&file, &config::DiffSettings { ignore_whitespace: true, context_lines: 5, syntax_highlight: false }).unwrap();
    set_commit_message_settings_at(&file, &CommitMessageSettings { summary_limit: 55, body_ruler: 72, show_ruler: true }).unwrap();
    assert_eq!(config::get_diff_settings_at(&file).unwrap().context_lines, 5);
    assert_eq!(config::list_recent_repos_at(&file).unwrap().len(), 1);
    assert!(std::fs::read_to_string(&file).unwrap().contains("[commit_message]"));
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p config --test commit_message_settings` → FAIL.

- [ ] **Step 3: Implement** — `CommitMessageSettings` (same derives and `#[serde(default)]` as `DiffSettings`, `Default` 50/72/true), a `commit_message` field in `ConfigFile` right after `diff` (update the ordering comment), `get/set_commit_message_settings[_at]` with the `10..=200` clamp on write.

- [ ] **Step 4: Run tests** — `cargo test -p config` → PASS.

- [ ] **Step 5: Commit**

```bash
cargo clippy -p config --all-targets -- -D warnings && cargo fmt --all -- --check
git add crates/config
git commit -m "feat(config): commit message summary limit and ruler settings"
```

---

### Task B4: `repo-service` — base64, worker commands

**Files:**
- Create: `crates/repo-service/src/base64.rs`; modify `crates/repo-service/src/lib.rs` (`pub mod base64;`)
- Modify: `crates/repo-service/src/worker/mod.rs` (`GetFileSides`, `ListRecentAuthors`; `Commit`/`AmendCommit` gain fields), `worker/status.rs` (commit handler), `worker/history.rs` (amend handler), `worker/undo.rs` (`assert_held!` literals)

**Interfaces:**
- `pub fn encode(bytes: &[u8]) -> String`.
- `Command::GetFileSides { target: FileSidesTarget, path: String, old_path: Option<String>, reply: Sender<Result<FileSides, String>> }` with an owned `pub enum FileSidesTarget { Working { staged: bool }, Commit { commit_id: String } }` (re-exported from `worker`).
- `Command::ListRecentAuthors { reply: Sender<Result<Vec<SignatureInput>, String>> }`.
- `Command::Commit { message, trailers: Vec<Trailer>, reply }`, `Command::AmendCommit { message, author: Option<SignatureInput>, trailers: Vec<Trailer>, reply }`.
- `WorkerHandle::{get_file_sides(target, path, old_path), list_recent_authors(), commit(message, trailers), amend_commit(message, author, trailers)}`.

- [ ] **Step 1: Write the failing tests**

```rust
// base64.rs
#[cfg(test)]
mod tests {
    #[test]
    fn matches_the_rfc_4648_test_vectors() {
        for (input, expected) in [
            ("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(super::encode(input.as_bytes()), expected, "{input:?}");
        }
        assert_eq!(super::encode(&[0xff, 0xfe, 0xfd]), "//79");
    }
}
```

Worker tests: `get_file_sides` on an unstaged edit returns both sides; `commit` with a trailer lands it in HEAD's message; `amend_commit` with an author changes the author; `list_recent_authors` excludes the test user.

- [ ] **Step 2: Run to verify failure** — `cargo test -p repo-service` → FAIL.

- [ ] **Step 3: Implement**

`encode` (fixed body):

```rust
pub fn encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}
```

The handlers call `git_core::diff::file_sides(.., MAX_PREVIEW_BYTES)` and `git_core::commit::*`. Update `assert_held!(Commit { … })` / `AmendCommit { … }` literals with the new fields. Undo labels are unchanged (`quoted("Commit ", message)` uses the typed message).

- [ ] **Step 4: Run tests** — `cargo test -p repo-service` → PASS.

- [ ] **Step 5: Commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check
git add crates/repo-service
git commit -m "feat(repo-service): file sides, recent authors, commit trailers and amend author"
```

---

### Task B5: Tauri commands and sidecar methods

**Files:** `crates/tauri-app/src/commands/{mod,history,status}.rs` (wherever `commit`/`amend_commit` live), `main.rs`; `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs`.

- [ ] **Step 1: Write the failing Tauri DTO tests**

```rust
#[test]
fn file_sides_dto_encodes_base64_and_tags_each_side() {
    use git_core::diff::{FileSide, FileSides};
    let dto = serde_json::to_value(FileSidesDto::from(FileSides {
        old: FileSide::Missing,
        new: FileSide::Loaded { bytes: b"foo".to_vec() },
    }))
    .unwrap();
    assert_eq!(
        dto,
        serde_json::json!({ "old": { "kind": "Missing" }, "new": { "kind": "Loaded", "size": 3, "base64": "Zm9v" } })
    );
    let big = serde_json::to_value(FileSidesDto::from(FileSides {
        old: FileSide::TooLarge { size: 25_000_000 },
        new: FileSide::Missing,
    }))
    .unwrap();
    assert_eq!(big["old"], serde_json::json!({ "kind": "TooLarge", "size": 25_000_000 }));
}

#[test]
fn commit_inputs_and_settings_dtos_are_camel_case() {
    let target: FileSidesTargetDto = serde_json::from_value(serde_json::json!({ "kind": "Commit", "commitId": "abc" })).unwrap();
    assert!(matches!(repo_service::worker::FileSidesTarget::from(target), repo_service::worker::FileSidesTarget::Commit { commit_id } if commit_id == "abc"));
    let trailer: TrailerDto = serde_json::from_value(serde_json::json!({ "key": "Co-authored-by", "value": "A <a@x>" })).unwrap();
    assert_eq!(git_core::commit::Trailer::from(trailer).key, "Co-authored-by");
    let settings = serde_json::to_value(CommitMessageSettingsDto::from(config::CommitMessageSettings::default())).unwrap();
    assert_eq!(settings, serde_json::json!({ "summaryLimit": 50, "bodyRuler": 72, "showRuler": true }));
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p tauri-app` → FAIL.

- [ ] **Step 3: Implement** — DTOs (`FileSideDto` tagged `kind`, `Loaded { size, base64 }` via `repo_service::base64::encode`; `FileSidesTargetDto` tagged `kind` with camelCase fields; `TrailerDto`/`SignatureInputDto` `Deserialize` + `Serialize` for `list_recent_authors`; `CommitMessageSettingsDto` both ways). Commands: `get_file_sides(repo_path, target, path, old_path: Option<String>)`, `list_recent_authors(repo_path)`, `get_commit_message_settings()`, `set_commit_message_settings(settings)`. `commit` gains `trailers: Option<Vec<TrailerDto>>` (`None` → empty), and `amend_commit` gains `author: Option<SignatureInputDto>, trailers: Option<Vec<TrailerDto>>`. Register the new commands.

- [ ] **Step 4: Write the failing sidecar round-trip test** — open a repo with a committed `img.png` ("foo") and an unstaged edit ("foobar"); `get_file_sides` → `old.base64 == "Zm9v"`, `new.base64 == "Zm9vYmFy"`; `commit` with `trailers: [{ key: "Co-authored-by", value: "Ada <ada@x.org>" }]` after staging → `get_commit_message` of HEAD ends with that line; `get_commit_message_settings` → defaults (the sidecar test uses `ConfigDirGuard`).

- [ ] **Step 5: Implement the sidecar methods** — the same four new methods, and `#[serde(default)] trailers` / `author` on the commit/amend params. Private DTO copies.

- [ ] **Step 6: Run tests** — `cargo test -p tauri-app && cargo test -p vscode-sidecar` → PASS.

- [ ] **Step 7: Commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check
git add crates/tauri-app crates/vscode-sidecar
git commit -m "feat: expose file sides, recent authors, trailers, amend author and commit message settings"
```

---

### Task B6: `RepoClient` — both clients; VSCode CSP

**Files:** `frontend/src/ipc/{RepoClient,tauriRepoClient,vscodeRepoClient}.ts` (+ tests, fixtures); `extension/src/webviewHtml.ts`, `webviewHtml.test.ts`.

**Interfaces:**

```ts
export type FileSidesTarget = { kind: "Working"; staged: boolean } | { kind: "Commit"; commitId: string };
export type FileSide = { kind: "Missing" } | { kind: "TooLarge"; size: number } | { kind: "Loaded"; size: number; base64: string };
export interface FileSides { old: FileSide; new: FileSide; }
export interface Trailer { key: string; value: string; }
export interface SignatureInput { name: string; email: string; }
export interface AmendOptions { author: SignatureInput | null; trailers: Trailer[]; }
export interface CommitMessageSettings { summaryLimit: number; bodyRuler: number; showRuler: boolean; }
export const DEFAULT_COMMIT_MESSAGE_SETTINGS: CommitMessageSettings = { summaryLimit: 50, bodyRuler: 72, showRuler: true };

getFileSides(repoPath: string, target: FileSidesTarget, path: string, oldPath: string | null): Promise<FileSides>;
listRecentAuthors(repoPath: string): Promise<SignatureInput[]>;
getCommitMessageSettings(): Promise<CommitMessageSettings>;
setCommitMessageSettings(settings: CommitMessageSettings): Promise<void>;
commit(repoPath: string, message: string, trailers?: Trailer[]): Promise<void>;
amendCommit(repoPath: string, message: string, options?: AmendOptions): Promise<void>;
```

- [ ] **Step 1: Write the failing tests**

- `tauriRepoClient.test.ts`: `commit("/r", "m")` → `invoke("commit", { repoPath: "/r", message: "m", trailers: [] })`; `amendCommit("/r", "m")` → `{ repoPath, message, author: null, trailers: [] }`; with options passes them through; `getFileSides` → `get_file_sides` with `{ repoPath, target, path, oldPath }`; `listRecentAuthors`; get/set settings.
- `vscodeRepoClient.test.ts`: the same methods and params.
- `webviewHtml.test.ts`: the CSP contains `img-src vscode-webview://unit-test data:` (the test's `cspSource`) and still no `unsafe-eval`/inline script.

- [ ] **Step 2: Run to verify failure** — `cd frontend && pnpm test -- --run src/ipc` and `cd extension && pnpm test -- --run` → FAIL.

- [ ] **Step 3: Implement** — both clients. CSP: `"img-src " + webview.cspSource + " data:; "`. Update the existing `commit`/`amendCommit` expectations in the client tests to the new params.

- [ ] **Step 4: Fix fixtures** — `pnpm exec tsc --noEmit -p .`; add `getFileSides: async () => ({ old: { kind: "Missing" }, new: { kind: "Missing" } }), listRecentAuthors: async () => [], getCommitMessageSettings: async () => DEFAULT_COMMIT_MESSAGE_SETTINGS, setCommitMessageSettings: async () => {},` to every literal.

- [ ] **Step 5: Run tests and lint** — `cd frontend && pnpm test -- --run && pnpm lint`; `cd extension && pnpm test -- --run && pnpm run compile` → PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src extension/src
git commit -m "feat(frontend): RepoClient file sides, recent authors, trailers and commit message settings"
```

---

### Task B7: Image diff

**Files:**
- Create: `frontend/src/lib/imageTypes.ts` (+ test), `frontend/src/components/ImageDiff.tsx`, `ImageDiff.module.css`, `ImageDiff.test.tsx`
- Modify: `frontend/src/components/DiffPane.tsx` (`UncommittedFileSection` around the `DiffView` at ~line 371; `CommitFileSection` around ~line 1154) (+ `DiffPane.test.tsx`)

**Interfaces:**
- `imageMimeType(path: string): string | null`; `formatBytes(n: number): string` ("512 B", "12.3 KB", "24.1 MB", base 1024, one decimal above bytes).
- `ImageDiff({ client, repoPath, target: FileSidesTarget, path, oldPath: string | null, refreshKey?: number })`.

- [ ] **Step 1: Write the failing tests**

`imageTypes.test.ts`:

```ts
it("maps image extensions case-insensitively and formats sizes", () => {
  expect(imageMimeType("a/B.PNG")).toBe("image/png");
  expect(imageMimeType("x.jpeg")).toBe("image/jpeg");
  expect(imageMimeType("x.jpg")).toBe("image/jpeg");
  expect(imageMimeType("x.gif")).toBe("image/gif");
  expect(imageMimeType("x.webp")).toBe("image/webp");
  expect(imageMimeType("icon.svg")).toBe("image/svg+xml");
  expect(imageMimeType("x.bmp")).toBeNull();
  expect(imageMimeType("png")).toBeNull();
  expect(formatBytes(512)).toBe("512 B");
  expect(formatBytes(12_595)).toBe("12.3 KB");
  expect(formatBytes(25_270_000)).toBe("24.1 MB");
});
```

`ImageDiff.test.tsx`:

```tsx
const sides = (old: FileSide, next: FileSide) => fakeClient({ getFileSides: vi.fn(async () => ({ old, new: next })) });

it("shows before and after images side by side with size and loaded dimensions", async () => {
  const client = sides({ kind: "Loaded", size: 3, base64: "Zm9v" }, { kind: "Loaded", size: 2048, base64: "YmFy" });
  render(<ImageDiff client={client} repoPath="/r" target={{ kind: "Working", staged: false }} path="logo.png" oldPath={null} />);
  const before = await screen.findByRole("img", { name: "Before: logo.png" });
  expect(before).toHaveAttribute("src", "data:image/png;base64,Zm9v");
  const after = screen.getByRole("img", { name: "After: logo.png" });
  Object.defineProperty(after, "naturalWidth", { value: 64 });
  Object.defineProperty(after, "naturalHeight", { value: 32 });
  fireEvent.load(after);
  expect(await screen.findByText("64 × 32 px · 2.0 KB")).toBeInTheDocument();
  expect(screen.getByText("3 B")).toBeInTheDocument(); // before not loaded yet: size only
  expect(client.getFileSides).toHaveBeenCalledWith("/r", { kind: "Working", staged: false }, "logo.png", null);
});

it("labels added, deleted and too-large sides, and decode failures", async () => {
  render(<ImageDiff client={sides({ kind: "Missing" }, { kind: "TooLarge", size: 25_270_000 })} repoPath="/r" target={{ kind: "Commit", commitId: "c" }} path="a.png" oldPath={null} />);
  expect(await screen.findByText("Added")).toBeInTheDocument();
  expect(screen.getByText("Too large to preview (24.1 MB)")).toBeInTheDocument();

  render(<ImageDiff client={sides({ kind: "Loaded", size: 1, base64: "AA==" }, { kind: "Missing" })} repoPath="/r" target={{ kind: "Commit", commitId: "c" }} path="b.png" oldPath={null} />);
  expect(await screen.findByText("Deleted")).toBeInTheDocument();
  fireEvent.error(screen.getByRole("img", { name: "Before: b.png" }));
  expect(await screen.findByText("Can't display this image")).toBeInTheDocument();
});

it("renders SVG only through an img element", async () => {
  const { container } = render(
    <ImageDiff client={sides({ kind: "Missing" }, { kind: "Loaded", size: 10, base64: "PHN2Zy8+" })} repoPath="/r" target={{ kind: "Working", staged: true }} path="i.svg" oldPath={null} />,
  );
  const img = await screen.findByRole("img", { name: "After: i.svg" });
  expect(img.tagName).toBe("IMG");
  expect(img).toHaveAttribute("src", "data:image/svg+xml;base64,PHN2Zy8+");
  expect(container.querySelector("svg, object, iframe, embed")).toBeNull();
});
```

(`lucide-react` icons render `<svg>`. If `ImageDiff` uses none, the `svg` selector is valid. Otherwise scope the query to the image columns.)

`DiffPane.test.tsx`: selecting an unstaged `logo.png` row calls `getFileSides` and not `getWorkingDiff`. An `icon.svg` row shows the image with a `Show text diff` button that switches to the `DiffView` (and `getWorkingDiff` is called then). A committed `logo.png` in `CommitFileSection` passes `{ kind: "Commit", commitId }` and `oldPath`.

- [ ] **Step 2: Run to verify failure** — `cd frontend && pnpm test -- --run src/lib/imageTypes src/components/ImageDiff src/components/DiffPane` → FAIL.

- [ ] **Step 3: Implement**

- `ImageDiff` loads with a cancelled-flag effect keyed on `(repoPath, target kind/staged/commitId, path, oldPath, refreshKey)`. While loading it shows `Loading preview…` (`role="status"`); on error, `InlineError` with Retry. There are two columns (`<figure>` with `<figcaption>` `Before`/`After`). Each `Loaded` side is `<img alt=… src={\`data:${mime};base64,${base64}\`} onLoad={record naturalWidth/Height} onError={mark failed} />`, and the caption shows `${w} × ${h} px · ${formatBytes(size)}` once loaded (only `formatBytes(size)` before). The CSS has a checkerboard background (two `linear-gradient`s, theme tokens), `max-width: 100%`, `image-rendering: auto`, and columns stacked under 480px.
- `UncommittedFileSection`: `const mime = imageMimeType(entry.path); const [asText, setAsText] = useState(false);`. If `!isConflicted && mime !== null && !asText`, render `<ImageDiff target={{ kind: "Working", staged: entry.staged }} refreshKey={refreshGeneration} …/>` and skip `loadDiff`. For SVG only, render a `Show text diff` / `Show image` toggle button. `CommitFileSection` does the same with `{ kind: "Commit", commitId }` and `oldPath`.

- [ ] **Step 4: Run tests and lint** — `cd frontend && pnpm test -- --run && pnpm lint` → PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): side-by-side image preview for PNG, JPEG, GIF, WebP and SVG diffs"
```

---

### Task B8: Commit message settings — provider and Preferences section

**Files:**
- Create: `frontend/src/state/useCommitMessageSettings.tsx` (context + hook, modeled on `useDiffSettings.tsx`), `frontend/src/state/CommitMessageSettingsProvider.tsx` (+ test)
- Modify: `frontend/src/App.tsx` (wrap `AppContent` in the provider inside `DiffSettingsProvider`), 4f's Preferences dialog (+ test). If it does not exist (Task B0): create `frontend/src/components/PreferencesDialog.tsx` + test and a palette command `Preferences…` in `lib/commands.ts`.

- [ ] **Step 1: Write the failing tests**

- Hook: loads `getCommitMessageSettings` once and exposes `DEFAULT_COMMIT_MESSAGE_SETTINGS` until then. `update(partial)` is optimistic, then calls `setCommitMessageSettings` and keeps the new value. A failed load keeps the defaults.
- Preferences: a `Commit message` group with `Summary length limit` (spinbutton), `Ruler column` (spinbutton), `Show ruler` (checkbox). Changing them calls `setCommitMessageSettings` with the merged settings, and values outside 10–200 are clamped on blur.

```tsx
it("edits the commit message settings", async () => {
  const setCommitMessageSettings = vi.fn(async () => {});
  renderPreferences({ getCommitMessageSettings: async () => ({ summaryLimit: 50, bodyRuler: 72, showRuler: true }), setCommitMessageSettings });
  const group = await screen.findByRole("group", { name: "Commit message" });
  fireEvent.change(within(group).getByRole("spinbutton", { name: "Summary length limit" }), { target: { value: "60" } });
  fireEvent.blur(within(group).getByRole("spinbutton", { name: "Summary length limit" }));
  await waitFor(() => expect(setCommitMessageSettings).toHaveBeenLastCalledWith({ summaryLimit: 60, bodyRuler: 72, showRuler: true }));
  fireEvent.click(within(group).getByRole("checkbox", { name: "Show ruler" }));
  await waitFor(() => expect(setCommitMessageSettings).toHaveBeenLastCalledWith({ summaryLimit: 60, bodyRuler: 72, showRuler: false }));
  const ruler = within(group).getByRole("spinbutton", { name: "Ruler column" });
  fireEvent.change(ruler, { target: { value: "500" } });
  fireEvent.blur(ruler);
  await waitFor(() => expect(setCommitMessageSettings).toHaveBeenLastCalledWith({ summaryLimit: 60, bodyRuler: 200, showRuler: false }));
});
```

(`renderPreferences` renders the dialog inside `CommitMessageSettingsProvider` with a fake client, following 4f's own Preferences tests.)

- [ ] **Step 2–4:** run (FAIL), implement, run tests and lint (PASS).

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): commit message settings in Preferences"
```

---

### Task B9: CommitBox — summary counter and ruler

**Files:** `frontend/src/components/CommitBox.tsx`, `CommitBox.module.css`, `CommitBox.test.tsx`; `DiffPane.tsx` (pass `messageSettings` from `useCommitMessageSettingsContext()`).

**Interfaces:** `CommitBox` prop `messageSettings?: CommitMessageSettings` (default `DEFAULT_COMMIT_MESSAGE_SETTINGS`).

- [ ] **Step 1: Write the failing tests**

```tsx
it("counts summary characters and warns past the soft limit without blocking", () => {
  const onCommit = vi.fn();
  render(<CommitBox onCommit={onCommit} disabled={false} onAbortMerge={vi.fn()} messageSettings={{ summaryLimit: 10, bodyRuler: 72, showRuler: true }} />);
  const box = screen.getByRole("textbox", { name: "Commit message" });
  fireEvent.change(box, { target: { value: "0123456789\nbody that is long" } });
  const counter = screen.getByLabelText("Summary length");
  expect(counter).toHaveTextContent("Summary 10/10");
  expect(counter).not.toHaveAttribute("data-over");
  fireEvent.change(box, { target: { value: "0123456789é\n" } });
  expect(counter).toHaveTextContent("Summary 11/10");
  expect(counter).toHaveAttribute("data-over", "true");
  expect(counter).toHaveAttribute("title", "Many tools cut summaries longer than 10 characters.");
  fireEvent.click(screen.getByRole("button", { name: "Commit" }));
  expect(onCommit).toHaveBeenCalled();
});

it("counts code points, not UTF-16 units", () => {
  render(<CommitBox onCommit={vi.fn()} disabled={false} onAbortMerge={vi.fn()} />);
  fireEvent.change(screen.getByRole("textbox", { name: "Commit message" }), { target: { value: "🎉 ok" } });
  expect(screen.getByLabelText("Summary length")).toHaveTextContent("Summary 4/50");
});

it("draws the ruler at the configured column in a monospace field, or not at all", () => {
  const { rerender } = render(<CommitBox onCommit={vi.fn()} disabled={false} onAbortMerge={vi.fn()} messageSettings={{ summaryLimit: 50, bodyRuler: 80, showRuler: true }} />);
  const box = screen.getByRole("textbox", { name: "Commit message" });
  expect(box).toHaveAttribute("data-ruler", "true");
  expect(box.style.getPropertyValue("--ruler-column")).toBe("80");
  rerender(<CommitBox onCommit={vi.fn()} disabled={false} onAbortMerge={vi.fn()} messageSettings={{ summaryLimit: 50, bodyRuler: 80, showRuler: false }} />);
  expect(box).not.toHaveAttribute("data-ruler");
});
```

- [ ] **Step 2: Run to verify failure** → FAIL.

- [ ] **Step 3: Implement**

- `const summaryLength = [...message.split("\n", 1)[0]].length;` Below the textarea: `<span aria-label="Summary length" className={styles.counter} data-over={over ? "true" : undefined} title={over ? … : undefined}>Summary {summaryLength}/{limit}</span>`.
- Textarea: when `showRuler`, `data-ruler="true"` and `style={{ "--ruler-column": String(bodyRuler) } as CSSProperties}`.
- CSS:

```css
.textarea[data-ruler="true"] {
  font-family: var(--font-mono);
  background-image: linear-gradient(
    to right,
    transparent calc(var(--space-2) + var(--ruler-column) * 1ch),
    var(--color-border) calc(var(--space-2) + var(--ruler-column) * 1ch),
    var(--color-border) calc(var(--space-2) + var(--ruler-column) * 1ch + 1px),
    transparent calc(var(--space-2) + var(--ruler-column) * 1ch + 1px)
  );
  background-attachment: local;
}
.counter { font-size: var(--text-xs, 11px); color: var(--color-text-muted); }
.counter[data-over="true"] { color: var(--color-warning-text); }
```

  Use whichever muted/small tokens `tokens.css` defines (check the names).

- [ ] **Step 4: Run tests and lint** → PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): commit summary counter and body ruler"
```

---

### Task B10: CommitBox — co-authors and amend author; wiring

**Files:** `frontend/src/components/CommitBox.tsx`, `.module.css`, `.test.tsx`; `DiffPane.tsx` (+ test: client for `listRecentAuthors`, HEAD author from the graph commits); `state/useStagingActions.ts` (or wherever `commit` lives) and `state/useHistoryActions.ts` (`amendCommit(message, options)`); `App.tsx` (pass `commits` to `DiffPane`'s uncommitted branch if it is not there yet).

**Interfaces:**
- `CommitBox` props: `onCommit(message, trailers: Trailer[])`, `amend.onAmend(message, options: AmendOptions)`, `amend.headAuthor: SignatureInput | null`, `loadRecentAuthors?: () => Promise<SignatureInput[]>` (omit to hide "Add co-author").
- `CommitDraft.coAuthors?: SignatureInput[]`.
- Pure helpers in `CommitBox.tsx` (exported for tests): `formatPerson(p): string` → `Name <email>`, `parsePerson(text): SignatureInput | null` (`/^\s*(.+?)\s*<([^<>\s]+)>\s*$/`).

- [ ] **Step 1: Write the failing tests**

```tsx
it("adds co-authors from recent authors and commits them as trailers", async () => {
  const onCommit = vi.fn();
  render(
    <CommitBox
      onCommit={onCommit}
      disabled={false}
      onAbortMerge={vi.fn()}
      loadRecentAuthors={async () => [{ name: "Ada", email: "ada@x.org" }, { name: "Bob", email: "bob@x.org" }]}
    />,
  );
  fireEvent.change(screen.getByRole("textbox", { name: "Commit message" }), { target: { value: "Fix" } });
  fireEvent.click(screen.getByRole("button", { name: "Add co-author" }));
  const list = await screen.findByRole("listbox", { name: "Recent authors" });
  fireEvent.change(screen.getByRole("textbox", { name: "Filter authors" }), { target: { value: "bo" } });
  expect(within(list).queryByRole("option", { name: /Ada/ })).toBeNull();
  fireEvent.click(within(list).getByRole("option", { name: "Bob <bob@x.org>" }));
  expect(screen.getByText("Bob <bob@x.org>")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Commit" }));
  expect(onCommit).toHaveBeenCalledWith("Fix", [{ key: "Co-authored-by", value: "Bob <bob@x.org>" }]);
  expect(screen.queryByText("Bob <bob@x.org>")).toBeNull();
});

it("removes a co-author chip", async () => {
  render(<CommitBox onCommit={vi.fn()} disabled={false} onAbortMerge={vi.fn()} initialDraft={{ message: "x", lastSeeded: "", coAuthors: [{ name: "Ada", email: "ada@x.org" }] }} loadRecentAuthors={async () => []} />);
  fireEvent.click(screen.getByRole("button", { name: "Remove co-author Ada" }));
  expect(screen.queryByText("Ada <ada@x.org>")).toBeNull();
});

it("hides co-authors while a merge message is seeded", () => {
  render(<CommitBox onCommit={vi.fn()} disabled={false} onAbortMerge={vi.fn()} initialMessage="Merge branch 'x'" loadRecentAuthors={async () => []} />);
  expect(screen.queryByRole("button", { name: "Add co-author" })).toBeNull();
});

function amendBox(onAmend = vi.fn(), headAuthor: SignatureInput | null = { name: "Me", email: "me@x.org" }) {
  render(
    <CommitBox
      onCommit={vi.fn()}
      disabled={false}
      onAbortMerge={vi.fn()}
      amend={{ available: true, on: true, headMessage: "Old", pushed: false, onToggle: vi.fn(), onAmend, headAuthor }}
    />,
  );
  return onAmend;
}

it("amends with the author unchanged as null, and with an edited author parsed", async () => {
  const onAmend = amendBox();
  const author = await screen.findByRole("textbox", { name: "Author" });
  expect(author).toHaveValue("Me <me@x.org>");
  fireEvent.click(screen.getByRole("button", { name: "Amend" }));
  expect(onAmend).toHaveBeenLastCalledWith("Old", { author: null, trailers: [] });
  fireEvent.change(author, { target: { value: "Ada Lovelace <ada@x.org>" } });
  fireEvent.click(screen.getByRole("button", { name: "Amend" }));
  expect(onAmend).toHaveBeenLastCalledWith("Old", { author: { name: "Ada Lovelace", email: "ada@x.org" }, trailers: [] });
});

it("blocks amend on an unparseable author", async () => {
  const onAmend = amendBox();
  fireEvent.change(await screen.findByRole("textbox", { name: "Author" }), { target: { value: "Ada" } });
  fireEvent.click(screen.getByRole("button", { name: "Amend" }));
  expect(screen.getByText("Use the form Name <email>.")).toBeInTheDocument();
  expect(onAmend).not.toHaveBeenCalled();
});

it("leaves the author empty with a placeholder when HEAD's author is unknown", async () => {
  const onAmend = amendBox(vi.fn(), null);
  const author = await screen.findByRole("textbox", { name: "Author" });
  expect(author).toHaveValue("");
  expect(author).toHaveAttribute("placeholder", "Keep original author");
  fireEvent.click(screen.getByRole("button", { name: "Amend" }));
  expect(onAmend).toHaveBeenLastCalledWith("Old", { author: null, trailers: [] });
});
```

Update the existing CommitBox tests' `onCommit` expectations to `(message, [])` and `onAmend` to `(message, { author: null, trailers: [] })`. `DiffPane.test.tsx`: in amend mode, the Author field is prefilled from the `commits` entry whose `id === headInfo.headId`, and amending calls `onAmendCommit("…", { author: null, trailers: [] })`. `useAppState`/actions tests: `commit("m", trailers)` → `client.commit("/r", "m", trailers)`; `amendCommit("m", opts)` → `client.amendCommit("/r", "m", opts)`.

- [ ] **Step 2: Run to verify failure** → FAIL.

- [ ] **Step 3: Implement**

- Co-author state `coAuthors: SignatureInput[]` (initialized from `initialDraft?.coAuthors ?? []`, included in `onDraftChange`). "Add co-author" opens a popover (positioned under the button, closes on Esc/outside click) that loads `loadRecentAuthors()` once per open. It holds a filter text field and a `role="listbox"` of `role="option"` rows (`formatPerson`), excluding authors already chosen. Chips are `<span>` + × button. Hidden when `initialMessage !== undefined` (merge/pick pause). `commitIfReady` passes `coAuthors.map(p => ({ key: "Co-authored-by", value: formatPerson(p) }))` and clears `coAuthors` after `onCommit` (same timing as the message reset). Amend passes them too.
- Author field (amend only): local state seeded from `amend.headAuthor` when amend turns on. On Amend: `text.trim() === "" || text === formatPerson(headAuthor)` → `author: null`; else `parsePerson(text)` or show the error and return.
- `DiffPane` (uncommitted branch): `loadRecentAuthors={() => client.listRecentAuthors(repoPath)}`. `headAuthor` comes from `commits.find(c => c.id === headInfo.headId)` → `{ name: authorName, email: authorEmail }`, else `null`. `onCommit(message, trailers)` and `onAmendCommit(message, options)` pass through.
- Actions: `commit(message, trailers = [])` and `amendCommit(message, options?)` forward to the client.

- [ ] **Step 4: Run tests and lint** — `cd frontend && pnpm test -- --run && pnpm lint && pnpm build` → PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): co-author trailers from recent authors and author override on amend"
```

---

### Task B11: Docs and verification (Part B)

**Files:** `CHANGELOG.md`, `docs/USER_GUIDE.md`.

- [ ] **Step 1: CHANGELOG**

```markdown
### Image diff and commit message polish

- PNG, JPEG, GIF, WebP and SVG changes show the old and new image side by side with their
  dimensions and file size (up to 20 MB per side). SVGs can switch to the text diff.
- The commit message shows a summary length counter (soft limit 50) and a ruler at column 72
  for the body; both are configurable under **Preferences → Commit message**.
- **Add co-author** picks from the repository's recent authors and adds `Co-authored-by:`
  trailers. Amending shows an editable **Author** field (the committer stays you).
```

- [ ] **Step 2: USER_GUIDE**

- "Browsing history and diffs": a paragraph on image previews (formats, 20 MB cap, SVG text toggle, images never run scripts).
- "Staging and committing": the counter and ruler (and the Preferences settings), co-authors (recent authors from the last 1,000 commits, trailer format), and the amend Author field (blank or unchanged keeps the original; the original author date is kept).

- [ ] **Step 3: Full verification and commit**

```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && pnpm build && cd ..
cd extension && pnpm test -- --run && pnpm run compile && cd ..
```
Expected: all PASS. Run the Tauri E2E suite once (CLAUDE.md sequence) to confirm nothing regressed; there is no new spec in Part B.

```bash
git add CHANGELOG.md docs/USER_GUIDE.md
git commit -m "docs: image diff and commit message polish in user guide and changelog"
```

Open the 4g-2 PR (title "Image diff and commit message polish (batch 4g-2)", body referencing #96, the spec, and "closes #96").
