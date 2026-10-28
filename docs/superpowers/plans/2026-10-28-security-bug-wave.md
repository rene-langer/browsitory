# Security and Bug Fix Wave Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close #114 (forced checkouts writing through symlinks), #138 (race in the work-tree image read), #110 (signing without timeout/cancel), and fix #116 (per-file diff includes `d/…`), #117 (conflicted file in working-tree compare) and #137 (staged rename has no old path). Both the Tauri app and the VSCode extension.

**Architecture:** One new `git-core` module, `worktree_guard.rs`, owns every "never through a symlink" rule: hazard scan + link removal before each forced checkout (generalizing batch 4c's path-limited stash fix), `remove_beneath`/`write_beneath` for Browsitory's own writes, and `open_beneath` (an `openat` walk with `O_NOFOLLOW` per component on unix; reparse-point open + final-path check on Windows) for the image preview and the conflicted compare. Signing moves onto a split-output variant of 4e's `process::run_captured`. The diff fixes stay inside `git-core::diff`/`stage`/`status`, with small DTO/`RepoClient` additions for `oldPath`.

**Tech Stack:** Rust (`git2` 0.21, `libc` on unix, `thiserror`), React + TypeScript, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-28-security-bug-wave-design.md` (probe results, design, product decisions D1–D5). This plan implements the **recommended** option of every decision; if the user picks another, the affected tasks are listed under "Decision hooks".

---

## Global Constraints

- **No new dependency** (D4-A). `libc` is already a unix dependency of `git-core`; extend its `docs/LICENSE_COMPLIANCE.md` usage text (`openat`, `O_DIRECTORY`, `O_CLOEXEC`) in Task Z1 — no new row. If D4-B is chosen instead, add the `windows-sys` row (verify with `cargo info windows-sys`) in the same commit as the dependency.
- **Branch:** `fix/security-bug-wave` (already pushed). One PR at the end.
- **Windows-safe tests:**
  - Everything creating a symlink or FIFO, or using `libc`, is `#[cfg(unix)]` (test *and* helper).
  - Repo helpers pin `core.autocrlf=false`, `core.eol=lf`, `core.safecrlf=false`, `commit.gpgSign=false`, `core.hooksPath=<gitdir>/hooks` (as `tests/common/mod.rs::init_repo` does). `tests/undo.rs` has its own `init_repo` that pins only `core.autocrlf`: Task S4 extends it.
  - Never rely on `init.defaultBranch`: read the branch name from `repo.head()` or `repo.set_head("refs/heads/main")` before the first commit.
  - The "outside" target is always a second `tempfile::TempDir`, never a fixed path.
- **Tests:** `git-core`/`repo-service` use real temp-dir repos; frontend mocks `RepoClient` only.
- **Never** touch the real `~/.local/share/com.browsitory.app` or the real config dir; nothing in this wave needs them.
- **Error copy, exactly:**
  - `The signing program did not finish within {seconds} s. If it is waiting for a passphrase, unlock your key and try again.` (`SignError::TimedOut`)
  - `Signing was cancelled.` (`SignError::Cancelled`)
  - `Signing failed: the signature was larger than {limit} bytes.` (`SignError::SignerFailed`, stdout overflow)
- **UI copy, exactly:** `Waiting for the signing program…` (CommitBox status line while signing; same Cancel button as hooks); `Renamed from {oldPath}` (existing copy, now also for staged renames).
- **CHANGELOG** entries land in Task Z1 under `## [Unreleased]` → `### Fixed` (and a `### Security` subsection). The pre-push hook blocks a push touching `crates/`/`frontend/src/` without it, so push only after Z1.

## Review Focus

1. **Nothing outside the repo is ever written, deleted or read through a link.** `tests/symlink_write_through.rs` (Task S9) is the matrix of every worktree-writing operation × {leaf, ancestor}; each asserts the outside files are byte-identical and present.
2. **Hazard removal never removes a target** — only the link itself (`remove_file`/`remove_dir` on the link, never `remove_dir_all`), and only links that a source path goes through (Task S1 `untracked_links_no_source_path_uses_are_left_alone`).
3. **Stash all still records the symlink** before it is replaced (Task S7 `stash_all_with_a_typechanged_symlink_stashes_the_link_and_restores_it_on_pop`).
4. **`open_beneath` fails closed**: any `ELOOP`/`ENOTDIR`/`ENOENT` → `None`, never a fallback open by path (Task S8).
5. **A signature is never truncated** (stdout overflow is an error, Task P1/P2) and a timed-out signer leaves no process behind (Task P1 reuses 4e's process-group kill).
6. **`StatusEntry.path` semantics changed for renames** (now the new path). Every consumer of a `Renamed` entry is checked in Task D4/D5 (stage, unstage, discard, diff, preview, diff tool, file actions menu).

---

## File Structure

| File | Tasks | Responsibility |
|---|---|---|
| `crates/git-core/src/worktree_guard.rs` (new), `lib.rs` | S1, S8 | `Hazard`, `GuardSource`, `scan`, `clear_hazards`, `has_symlinked_ancestor`, `remove_beneath`, `write_beneath`, `open_beneath` (+ inline unit tests) |
| `crates/git-core/src/reset.rs` (+ `tests/reset.rs`) | S2 | guard before hard reset |
| `crates/git-core/src/stage.rs` (+ `tests/discard_ignore.rs`, `tests/stage_commit.rs`, `tests/stage_lines.rs`) | S3, D1, D3 | discard leaf guard; `find_hunk` path filter; `unstage_file` for renames |
| `crates/git-core/src/undo.rs` (+ `tests/undo.rs`) | S4 | guard in `checkout_from_index`; ancestor-safe raw writes/deletes |
| `crates/git-core/src/merge.rs` (+ `tests/merge.rs`) | S5 | guard in `abort_merge`; `remove_beneath`/`write_beneath` in abort and conflict resolution |
| `crates/git-core/src/rebase.rs` (+ `tests/rebase.rs`) | S6 | guard in `restore_original_branch` |
| `crates/git-core/src/stash.rs` (+ `tests/stash.rs`) | S7 | hazard routing for stash all; shared helpers |
| `crates/git-core/src/diff.rs` (+ `tests/diff.rs`, `tests/compare.rs`) | S8, D1, D2, D3 | `workdir_side` via `open_beneath`; `hunks_from_diff` filter; conflicted compare; `staged_rename_diff` |
| `crates/git-core/tests/symlink_write_through.rs` (new), `tests/common/mod.rs` | S9 | regression matrix; `outside_dir`/`link` helpers |
| `crates/git-core/src/process.rs` | P1 | `run_captured_split` |
| `crates/git-core/src/signing.rs` (+ new `tests/signing_timeout.rs`) | P2 | `SignOptions`, timeouts, new errors |
| `crates/git-core/src/commit.rs` (+ `tests/stage_commit.rs`) | P3 | pass cancel, report `signing` |
| `frontend/src/lib/hookFailure.ts` (+ test), `components/CommitBox.test.tsx` | P3 | `Waiting for the signing program…` |
| `crates/git-core/src/status.rs` (+ `tests/status.rs`), `tools.rs` | D3 | `old_path`, new-path semantics, no worktree renames; `DiffSource::Staged { old_path }` |
| `crates/repo-service/src/worker/{status,mod}.rs`, `crates/tauri-app/src/commands/{status,mod}.rs`, `crates/vscode-sidecar/src/dispatch.rs` (+ `tests/protocol_roundtrip.rs`) | D4 | DTOs, `get_working_diff` `old_path` |
| `frontend/src/ipc/{RepoClient,tauriRepoClient,vscodeRepoClient}.ts` (+ tests, fixtures), `components/DiffPane.tsx` (+ test) | D5 | `oldPath` end to end |
| `CHANGELOG.md`, `docs/ARCHITECTURE.md`, `docs/LICENSE_COMPLIANCE.md` | Z1 | docs |

Workstreams for parallel execution (disjoint files): **S** (S1→S2…S7→S8→S9), **P** (P1→P2→P3), **D** (D1→D2→D3→D4→D5). D2 uses `open_beneath` from S8 — start D2 after S8, or stub with the existing `workdir_side` reader and switch in S8. P3 and D5 both touch the frontend but different files.

---

## Task S0: Pre-flight

- [ ] **Step 1:** `git status` clean on `fix/security-bug-wave`; `git log -1` contains the spec+plan commit.
- [ ] **Step 2:** Pre-push hook installed: `ln -sf "$(git rev-parse --show-toplevel)/scripts/hooks/pre-push" "$(git rev-parse --git-path hooks)/pre-push"`.
- [ ] **Step 3:** Baseline: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cd frontend && pnpm test -- --run && pnpm lint`. Note any pre-existing failure in the PR description; do not fix unrelated failures here.
- [ ] **Step 4:** Re-run the audit grep and compare with the spec's §1.1 table; a new forced checkout since 0632de19 gets a row in Task S9 and a guard:
  `grep -rn -E "checkout_tree|checkout_head|checkout_index|CheckoutBuilder|\.force\(\)|stash_save|\.reset\(|fs::write|remove_file|remove_dir" crates/git-core/src`

---

## Task S1: `worktree_guard` module

**Files:**
- Create: `crates/git-core/src/worktree_guard.rs`
- Modify: `crates/git-core/src/lib.rs` (`pub(crate) mod worktree_guard;`), `crates/git-core/src/stash.rs` (delete `has_symlinked_ancestor`, call the moved one), `crates/git-core/src/stage.rs` (`ensure_no_symlinked_parent` delegates to `worktree_guard::has_symlinked_ancestor`, keeps its `StageError` mapping)

**Interfaces (produces):** see spec §1.2: `GuardSource::{Tree, Index}`, `Hazard::{Leaf(String), Ancestor(String)}`, `scan`, `clear_hazards`, `has_symlinked_ancestor`, `remove_beneath`, `write_beneath`.

- [ ] **Step 1: Write the failing unit tests** (inline `#[cfg(test)] mod tests` in `worktree_guard.rs`, `#[cfg(unix)]` where links are made). Helper inside the test module: `fn repo() -> (TempDir, Repository)` with the pinned config from Global Constraints, `tc`=`plain\n`, `d/f`=`inner\n`, `a.txt`=`base\n` committed on `refs/heads/main`; `fn outside() -> TempDir` with `target.txt` and `f` = `OUTSIDE\n`.

```rust
#[test]
#[cfg(unix)]
fn scan_finds_a_leaf_typechange_and_a_symlinked_directory() {
    let (dir, repo) = repo();
    let out = outside();
    std::fs::remove_file(dir.path().join("tc")).unwrap();
    std::os::unix::fs::symlink(out.path().join("target.txt"), dir.path().join("tc")).unwrap();
    std::fs::remove_dir_all(dir.path().join("d")).unwrap();
    std::os::unix::fs::symlink(out.path(), dir.path().join("d")).unwrap();
    let tree = repo.head().unwrap().peel_to_tree().unwrap();

    let mut hazards = scan(dir.path(), &GuardSource::Tree(&tree), None).unwrap();
    hazards.sort_by_key(|h| format!("{h:?}"));
    assert_eq!(hazards, vec![Hazard::Ancestor("d".into()), Hazard::Leaf("tc".into())]);
}

#[test]
#[cfg(unix)]
fn a_link_the_source_also_has_as_a_link_is_not_a_hazard() { /* commit `l` -> "a.txt" as a symlink, keep it; scan is empty */ }

#[test]
#[cfg(unix)]
fn untracked_links_no_source_path_uses_are_left_alone() {
    // `extra` -> outside, untracked; scan(None) is empty and clear_hazards(&[]) leaves it.
}

#[test]
#[cfg(unix)]
fn scan_limited_to_paths_ignores_other_hazards() { /* only Leaf("tc") when paths = ["tc"] although d is linked */ }

#[test]
#[cfg(unix)]
fn clear_hazards_removes_only_the_links() {
    // after clear: tc and d absent; out/target.txt and out/f still "OUTSIDE\n"; out dir still exists
}

#[test]
#[cfg(unix)]
fn remove_beneath_does_not_follow_a_symlinked_ancestor() {
    // d -> outside (holding f); remove_beneath(dir, "d/f") == Ok(false); out/f still exists
}

#[test]
#[cfg(unix)]
fn write_beneath_replaces_a_leaf_link_and_refuses_a_linked_ancestor() {
    // tc -> out/target.txt: write_beneath(dir, "tc", b"new") == Ok(true); tc is a regular file "new"; target untouched
    // d -> outside: write_beneath(dir, "d/f", b"x") == Ok(false); out/f untouched
}

#[test]
fn scan_on_a_clean_worktree_is_empty() { /* runs on every platform */ }
```

- [ ] **Step 2:** `cargo test -p git-core worktree_guard` → fails to compile (module missing).
- [ ] **Step 3: Implement.**
  - `scan`: collect source paths (`Tree::walk` PreOrder over blobs, or `Index::iter` stage 0), skip mode `0o160000`; apply the literal `paths` filter (exact match, or `p` starts with `"{path}/"` — a directory path limits to everything below it, matching `CheckoutBuilder::path` + `disable_pathspec_match`). For each path: walk ancestors with a `HashMap<String, bool>` cache of "is symlink" (`symlink_metadata(...).file_type().is_symlink()`; `NotFound` → not a link and stop descending); the first linked ancestor yields `Ancestor(dir)` once (dedupe via `BTreeSet`) and skips the leaf check. Leaf: mode != `0o120000` and the worktree entry is a symlink → `Leaf(path)`.
  - `clear_hazards`: for each hazard path, `remove_file`; on Windows, if that fails and the link is a directory link, `remove_dir`. `NotFound` is fine (already gone). Never `remove_dir_all`.
  - `remove_beneath`: `has_symlinked_ancestor` → `Ok(false)`; leaf via `symlink_metadata` (never `is_file()`): directory → `Ok(false)`; else `remove_file` (`NotFound` → `Ok(true)`).
  - `write_beneath`: ancestor check → `Ok(false)`; leaf symlink → `remove_file`; `create_dir_all(parent)`; `std::fs::write`.
  - Move `has_symlinked_ancestor` from `stash.rs` verbatim.
- [ ] **Step 4:** `cargo test -p git-core` (whole crate: stash/stage callers unchanged in behaviour) passes; clippy clean.
- [ ] **Step 5:** Commit `git-core: worktree_guard module for symlink-safe worktree writes (#114)`.

---

## Task S2: Hard reset never writes through a link

**Files:** Modify `crates/git-core/src/reset.rs`; Test `crates/git-core/tests/reset.rs`; Modify `crates/git-core/tests/common/mod.rs`

- [ ] **Step 1: Shared test helpers** in `tests/common/mod.rs` (all `#[allow(dead_code)]`, unix ones `#[cfg(unix)]`):

```rust
/// A directory outside any repository holding `target.txt` and `f`, both "OUTSIDE\n".
#[allow(dead_code)]
pub fn outside_dir() -> TempDir {
    let dir = TempDir::new().expect("outside dir");
    std::fs::write(dir.path().join("target.txt"), "OUTSIDE\n").unwrap();
    std::fs::write(dir.path().join("f"), "OUTSIDE\n").unwrap();
    dir
}

#[allow(dead_code)]
pub fn assert_outside_untouched(outside: &Path) {
    assert_eq!(std::fs::read_to_string(outside.join("target.txt")).unwrap(), "OUTSIDE\n");
    assert_eq!(std::fs::read_to_string(outside.join("f")).unwrap(), "OUTSIDE\n");
}

/// Replaces the worktree entry `rel` (file or directory) with a symlink to `target`.
#[cfg(unix)]
#[allow(dead_code)]
pub fn replace_with_link(workdir: &Path, rel: &str, target: &Path) {
    let p = workdir.join(rel);
    match std::fs::symlink_metadata(&p) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(&p).unwrap(),
        Ok(_) => std::fs::remove_file(&p).unwrap(),
        Err(_) => {}
    }
    std::os::unix::fs::symlink(target, &p).unwrap();
}

#[allow(dead_code)]
pub fn is_link(workdir: &Path, rel: &str) -> bool {
    std::fs::symlink_metadata(workdir.join(rel)).is_ok_and(|m| m.file_type().is_symlink())
}
```

- [ ] **Step 2: Failing tests** in `tests/reset.rs`:

```rust
#[test]
#[cfg(unix)]
fn hard_reset_replaces_a_typechanged_symlink_without_writing_through_it() {
    let out = common::outside_dir();
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "tc", "plain\n");
    common::commit_all(&repo, "base");
    common::replace_with_link(dir.path(), "tc", &out.path().join("target.txt"));
    let head = repo.head().unwrap().target().unwrap().to_string();

    reset_to(&repo, &head, ResetMode::Hard).unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "tc"));
    assert_eq!(std::fs::read_to_string(dir.path().join("tc")).unwrap(), "plain\n");
}

#[test]
#[cfg(unix)]
fn hard_reset_replaces_a_symlinked_directory_without_writing_through_it() {
    // d/f committed; d -> outside; reset --hard; outside untouched; d is a real dir; d/f == "inner\n"
}

#[test]
#[cfg(unix)]
fn hard_reset_to_another_commit_does_not_write_through_a_link() {
    // base tc="plain\n"; second commit tc="second\n"; link tc; reset --hard base; tc == "plain\n"
}
```

- [ ] **Step 3:** `cargo test -p git-core --test reset` → the three fail (outside file becomes `plain\n` / `inner\n`).
- [ ] **Step 4: Implement** in `reset_to`, `ResetMode::Hard` only, before `repo.reset`:

```rust
if mode == ResetMode::Hard {
    if let Some(workdir) = repo.workdir() {
        let tree = target.tree()?;
        let hazards = crate::worktree_guard::scan(workdir, &crate::worktree_guard::GuardSource::Tree(&tree), None)?;
        crate::worktree_guard::clear_hazards(workdir, &hazards)?;
    }
}
```

- [ ] **Step 5:** `cargo test -p git-core --test reset` passes. Commit `git-core: hard reset never writes through a symlink (#114)`.

---

## Task S3: Discard never writes through a link

**Files:** Modify `crates/git-core/src/stage.rs`; Test `crates/git-core/tests/discard_ignore.rs`

- [ ] **Step 1: Failing tests:**
  - `discard_file_of_a_typechanged_symlink_restores_the_file_and_leaves_the_target_alone` (unix): `tc` → outside link; `discard_file(&repo, "tc")`; outside untouched; `tc` regular `plain\n`.
  - `discard_all_unstaged_of_a_typechanged_symlink_does_not_write_through` (unix): same through `discard_all_unstaged(&repo, &["tc".into()])`.
  - `discard_file_still_refuses_a_path_beyond_a_symlinked_directory` (unix): `d` → outside; `discard_file(&repo, "d/f")` is `Err(StageError::InvalidPath)`; outside untouched (pins existing behaviour).
- [ ] **Step 2:** Run `cargo test -p git-core --test discard_ignore` → first two fail.
- [ ] **Step 3: Implement** in `discard_file`, tracked branch, before the checkout: if `full` is a symlink and the stage-0 index entry's mode is not `0o120000`, `std::fs::remove_file(&full)?`. (Equivalent to `scan(Index, Some(&[path]))` + `clear_hazards`; use the guard functions for one rule set.)
- [ ] **Step 4:** Tests pass. Commit `git-core: discard never writes through a symlink (#114)`.

---

## Task S4: Undo/redo restore never writes or deletes through a link

**Files:** Modify `crates/git-core/src/undo.rs`; Test `crates/git-core/tests/undo.rs`

- [ ] **Step 1:** Extend `tests/undo.rs`'s `init_repo` to pin `core.eol=lf`, `core.safecrlf=false`, `commit.gpgSign=false`, `core.hooksPath=<gitdir>/hooks`, and `repo.set_head("refs/heads/main")` before the seed commit (only if no test there depends on the default branch name — grep first).
- [ ] **Step 2: Failing tests** (unix; an outside `TempDir` per test):
  - `restore_over_a_typechanged_symlink_does_not_write_through_it`: `clean = capture(Full)`; link `tc` → outside; `dirty = capture(Full)`; `restore(&repo, &dirty, &clean, "undo")` is `Ok`; outside untouched; `tc` regular `plain\n`.
  - `restore_over_a_symlinked_directory_does_not_write_through_it`: same with `d` → outside dir; `d/f` == `inner\n` in a real `d/`.
  - `restore_never_deletes_through_a_symlinked_directory`: unit-level check of the deletion path: build a `WorktreeChange` scenario where the target lacks `d/x.txt` (snapshot pair: with `d/x.txt` untracked → without), then replace `d` with a link to an outside dir holding `x.txt` **between** `capture` of the expected snapshot and `restore` *and* make verification pass (if `verify` refuses — the probe saw `RepositoryChanged` — assert that refusal and that outside `x.txt` still exists; the test pins "never deleted" either way).
- [ ] **Step 3:** Run → first two fail (probe rows `undo_restore_leaf`, `undo_restore_ancestor`).
- [ ] **Step 4: Implement:**
  - `checkout_from_index`: before `checkout_index`, `scan(workdir, &GuardSource::Index(&repo.index()?), Some(&from_index_vec))` → `clear_hazards`.
  - `apply`: deletions call `worktree_guard::remove_beneath(workdir, path)` (skip when it returns `false`); `write_worktree_file` first clears symlinked ancestors of that path (`scan` restricted to the path, or `has_symlinked_ancestor` → remove the linked ancestor dir with `clear_hazards(&[Hazard::Ancestor(..)])`), then `remove_existing` + write.
  - `prune_empty_parents`: stop at a symlink (`symlink_metadata` is_symlink → break) and never call `remove_dir` on it.
- [ ] **Step 5:** `cargo test -p git-core --test undo` passes; `cargo test -p repo-service` (undo worker tests) passes. Commit `git-core: undo/redo never writes through a symlink (#114)`.

---

## Task S5: Abort merge and conflict resolution never write through a link

**Files:** Modify `crates/git-core/src/merge.rs`; Test `crates/git-core/tests/merge.rs`

- [ ] **Step 1: Failing tests** (unix; fixture: `a.txt` conflicting between `main` and `other`, read `main`'s name from `HEAD`):
  - `abort_merge_restores_a_conflicted_path_replaced_by_a_symlink` — link `a.txt` after `start_merge` → `Conflicted`; `abort_merge`; outside untouched; `a.txt` regular, HEAD content.
  - `abort_merge_restores_a_merged_path_replaced_by_a_symlink` — `other` also changes `tc`; link `tc` after the merge stopped.
  - `abort_merge_restores_a_directory_replaced_by_a_symlink` — `other` also changes `d/f`; link `d`.
  - `abort_merge_never_deletes_an_added_file_through_a_symlinked_directory` — `other` adds `d/new.txt`; after the merge stopped, link `d` to an outside dir that holds its own `new.txt`; `abort_merge` is `Ok`; outside `new.txt` still exists.
  - `resolve_conflict_replaces_a_symlink_instead_of_writing_through_it` — link `a.txt`; `resolve_conflict(&repo, "a.txt", "resolved\n")`; outside untouched; `a.txt` regular `resolved\n`, staged.
  - `resolve_add_delete_conflict_replaces_a_symlink` — `other` deletes `a.txt`, `main` modifies; link `a.txt`; `resolve_add_delete_conflict(.., FileConflictChoice::Ours)`; outside untouched.
- [ ] **Step 2:** Run `cargo test -p git-core --test merge` → all six fail.
- [ ] **Step 3: Implement:**
  - `abort_merge`: after `index.read_tree(&head_tree)`, `scan(Tree(&head_tree), Some(&to_checkout))` → `clear_hazards` before `checkout_head`; the "added by the operation" branch uses `worktree_guard::remove_beneath(workdir, &rel)` instead of `full.is_file()` + `remove_file`.
  - `resolve_conflict` / `resolve_add_delete_conflict`: `worktree_guard::write_beneath(workdir, path, bytes)`; `Ok(false)` → `MergeError::SymlinkedFolder(path)` (cannot happen after `ensure_inside_workdir`, but fail closed). The `Delete` branch uses `remove_beneath`.
- [ ] **Step 4:** Tests pass, plus `cargo test -p git-core --test pick_sequence` (sequence abort goes through `abort_merge`). Commit `git-core: abort merge and conflict resolution never write through a symlink (#114)`.

---

## Task S6: Abort rebase never writes through a link

**Files:** Modify `crates/git-core/src/rebase.rs`; Test `crates/git-core/tests/rebase.rs`

- [ ] **Step 1: Failing tests** (unix): a rebase paused on a conflict (`feature` and `main` both change `a.txt`), then link `tc` (resp. `d`) to outside; `abort_rebase(&repo, state)` is `Ok`; outside untouched; `tc` regular `plain\n` (resp. real `d/f`). Names: `abort_rebase_replaces_a_typechanged_symlink`, `abort_rebase_replaces_a_symlinked_directory`.
- [ ] **Step 2:** Run → fail.
- [ ] **Step 3: Implement** in `restore_original_branch`, after `set_head`: `let tree = repo.head()?.peel_to_tree()?;` `scan(Tree(&tree), None)` → `clear_hazards` → existing forced `checkout_head`. Error type: add `RebaseError::Io(#[from] std::io::Error)` if not present.
- [ ] **Step 4:** Pass. Commit `git-core: abort rebase never writes through a symlink (#114)`.

---

## Task S7: Stash all never writes through a link

**Files:** Modify `crates/git-core/src/stash.rs`; Test `crates/git-core/tests/stash.rs`

- [ ] **Step 1: Failing tests** (unix):

```rust
#[test]
#[cfg(unix)]
fn stash_all_with_a_typechanged_symlink_stashes_the_link_and_restores_it_on_pop() {
    let out = common_outside(); // or common::outside_dir() if stash.rs uses `mod common`
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tc", "plain\n");
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "edited\n");
    std::fs::remove_file(dir.path().join("tc")).unwrap();
    std::os::unix::fs::symlink(out.path().join("target.txt"), dir.path().join("tc")).unwrap();

    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

    assert_eq!(std::fs::read_to_string(out.path().join("target.txt")).unwrap(), "OUTSIDE\n");
    assert_eq!(read(dir.path(), "tc"), "plain\n");
    assert_eq!(read(dir.path(), "a.txt"), "a\n");
    assert_eq!(git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap(), StashApplyOutcome::Applied);
    assert!(std::fs::symlink_metadata(dir.path().join("tc")).unwrap().file_type().is_symlink());
    assert_eq!(read(dir.path(), "a.txt"), "edited\n");
}
```

  - `stash_all_keep_index_with_a_typechanged_symlink_does_not_write_through` (keep_index on).
  - `stash_all_tracked_only_with_a_typechanged_symlink_does_not_write_through` (`include_untracked: false`).
  - `stash_all_refuses_when_a_directory_was_replaced_by_a_symlink` — `d` → outside dir: `Err(StashError::BeyondSymlink(_))`, outside untouched, nothing stashed (`list_stashes` empty), `d` still the link (D2-A).
  - `stash_all_without_hazards_still_uses_libgit2_messages` — pins that the default message is libgit2's `WIP on main: …` (guards against accidentally routing every stash).
- [ ] **Step 2:** Run `cargo test -p git-core --test stash` → the hazard tests fail (probe rows `stash_all_*`).
- [ ] **Step 3: Implement** in `save_stash`, before `stash_save2` when `opts.paths` is empty:

```rust
let workdir = repo.workdir().ok_or_else(|| git2::Error::from_str("cannot stash in a bare repository"))?.to_path_buf();
let head_tree = repo.head()?.peel_to_tree()?;
let index = repo.index()?;
use crate::worktree_guard::{scan, GuardSource};
let hazardous = !scan(&workdir, &GuardSource::Tree(&head_tree), None)?.is_empty()
    || !scan(&workdir, &GuardSource::Index(&index), None)?.is_empty();
if hazardous {
    let paths = changed_paths(repo, opts.include_untracked)?; // status, literal, no dirs, no ignored
    let routed = StashOptions { paths, ..opts.clone() };
    return save_path_limited_stash(repo, &routed, message);
}
```

  `changed_paths`: same `StatusOptions` as `save_path_limited_stash` without pathspecs; `NothingToStash` when empty. The path-limited implementation already refuses ancestors (`BeyondSymlink`) and replaces leaf links (step 2), now via `worktree_guard`.
- [ ] **Step 4:** `cargo test -p git-core --test stash` passes (all existing 4c tests too). Commit `git-core: stash all never writes through a symlink (#114)`.

---

## Task S8: Race-free work-tree read (`open_beneath`) (#138)

**Files:** Modify `crates/git-core/src/worktree_guard.rs`, `crates/git-core/src/diff.rs`; Test inline in `worktree_guard.rs` and `crates/git-core/tests/diff.rs`

**Interfaces (produces):** `pub(crate) fn open_beneath(workdir: &Path, rel: &Path) -> std::io::Result<Option<std::fs::File>>` — `rel` must already be `is_safe_relative`.

- [ ] **Step 1: Failing unit tests** (inline):
  - every platform: `open_beneath_opens_a_regular_file_in_a_subdirectory`, `open_beneath_returns_none_for_a_missing_path_or_a_directory`, `open_beneath_returns_none_when_a_component_is_a_file` (`a.txt/x`).
  - `#[cfg(unix)]`: `open_beneath_refuses_a_leaf_symlink`, `open_beneath_refuses_a_symlinked_intermediate_directory` (`rd` → outside dir holding `secret.png`; `open_beneath(dir, "rd/secret.png")` is `None`), `open_beneath_does_not_block_on_a_fifo` (`libc::mkfifo`, returns `None` promptly — move/keep the existing FIFO test from `diff.rs`).
  - `#[cfg(unix)] #[ignore = "stress: run manually"]` `directory_swap_race_never_reads_outside` — a thread toggles `rd` between a real dir (with `secret.png` = `inside`) and a symlink to an outside dir (`secret.png` = `OUTSIDE`) in a loop; the main thread runs `file_sides(Unstaged, "rd/secret.png")` 200_000 times; assert no `Loaded { bytes }` equals `b"OUTSIDE"`.
- [ ] **Step 2:** Run `cargo test -p git-core worktree_guard` → fails (fn missing).
- [ ] **Step 3: Implement (unix):**

```rust
#[cfg(unix)]
pub(crate) fn open_beneath(workdir: &Path, rel: &Path) -> std::io::Result<Option<std::fs::File>> {
    use std::ffi::CString;
    use std::os::fd::{FromRawFd, OwnedFd, AsRawFd};
    use std::os::unix::ffi::OsStrExt;
    let gone = |e: &std::io::Error| matches!(e.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR | libc::ENOENT));
    let open_at = |dir: libc::c_int, name: &std::ffi::OsStr, flags: libc::c_int| -> std::io::Result<OwnedFd> {
        let c = CString::new(name.as_bytes()).map_err(|_| std::io::ErrorKind::InvalidInput)?;
        let fd = unsafe { libc::openat(dir, c.as_ptr(), flags | libc::O_CLOEXEC) };
        if fd < 0 { Err(std::io::Error::last_os_error()) } else { Ok(unsafe { OwnedFd::from_raw_fd(fd) }) }
    };
    let root = std::fs::File::open(workdir)?; // may follow links in the repo path itself (by design)
    let mut dir: OwnedFd = root.into();
    let parts: Vec<_> = rel.components().map(|c| c.as_os_str().to_owned()).collect();
    let Some((last, parents)) = parts.split_last() else { return Ok(None) };
    for part in parents {
        match open_at(dir.as_raw_fd(), part, libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW) {
            Ok(next) => dir = next,
            Err(e) if gone(&e) => return Ok(None),
            Err(e) => return Err(e),
        }
    }
    let file = match open_at(dir.as_raw_fd(), last, libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK) {
        Ok(fd) => std::fs::File::from(fd),
        Err(e) if gone(&e) => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(file.metadata()?.is_file().then_some(file))
}
```

  (`O_DIRECTORY` on an intermediate symlink fails with `ELOOP` because of `O_NOFOLLOW`; on a file with `ENOTDIR`.)
- [ ] **Step 4: Implement (Windows, D4-A):** open `workdir.join(rel)` with `.read(true).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT /* 0x0020_0000 */)` (no `FILE_FLAG_BACKUP_SEMANTICS`: only files are opened, so a directory fails and maps to `None`); if `metadata().file_type().is_symlink()` or not `is_file()` → `None`; then `final_path(&file)` via

```rust
#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetFinalPathNameByHandleW(h: *mut core::ffi::c_void, buf: *mut u16, len: u32, flags: u32) -> u32;
}
```

  (`flags = 0` = `FILE_NAME_NORMALIZED | VOLUME_NAME_DOS`; grow the buffer when the return value exceeds its length), and require `PathBuf::from(OsString::from_wide(..)).starts_with(std::fs::canonicalize(workdir)?)`; otherwise `None`. `NotFound`/`PermissionDenied`-on-directory → `None`. Comment why (handle-based, so a swapped junction is detected). CI on Windows compiles and runs the non-unix tests; add `open_beneath_opens_a_regular_file_in_a_subdirectory` as the Windows smoke test.
- [ ] **Step 5: Use it** in `diff::workdir_side`: replace `parent_stays_inside` + `symlink_metadata` + `open_regular_file` + `same_file` with `open_beneath(workdir, rel)?` → `None` ⇒ `Missing`; `let len = file.metadata()?.len();` `> cap` ⇒ `TooLarge { size: len }`; bounded read as today (the grown-file `TooLarge` branch uses `file.metadata()` instead of a path `metadata`). Delete `open_regular_file`/`same_file` if unused.
- [ ] **Step 6:** `cargo test -p git-core` passes (existing `a_symlinked_working_file_is_missing`, `a_working_file_behind_a_symlinked_folder_is_missing`, FIFO test). Run the ignored stress test once locally: `cargo test -p git-core directory_swap_race -- --ignored` → 0 leaks; note the run in the PR.
- [ ] **Step 7:** Commit `git-core: race-free work-tree read for the image preview (#138)`.

---

## Task S9: Regression matrix (#114)

**Files:** Create `crates/git-core/tests/symlink_write_through.rs` (whole file `#![cfg(unix)]`)

- [ ] **Step 1:** One test per row of the spec's §1.1 table, each with a fresh repo + `common::outside_dir()`, ending in `common::assert_outside_untouched(out.path())` (and, for the abort-merge added-path row, `assert!(out.path().join("new.txt").exists())`). Rows that are *refusals* also assert the specific error (`branch::switch_branch` → `BranchError::Git` with `ErrorCode::Conflict`, stash apply → `StashError::WouldOverwrite`, `discard_file("d/f")` → `StageError::InvalidPath`, path-limited stash → `BeyondSymlink`, stash-all ancestor → `BeyondSymlink`). Names: `<operation>_<leaf|ancestor>_<never_writes_through|is_refused>`, e.g. `reset_hard_ancestor_never_writes_through`, `switch_branch_leaf_is_refused`, `abort_sequence_leaf_is_refused`.
- [ ] **Step 2:** `cargo test -p git-core --test symlink_write_through` passes (all fixes from S2–S7 in place). Temporarily revert one guard (e.g. comment out S2's `clear_hazards`) and confirm the matching rows fail; restore.
- [ ] **Step 3:** Commit `git-core: symlink write-through regression matrix (#114)`.

---

## Task P1: `process::run_captured_split`

**Files:** Modify `crates/git-core/src/process.rs` (inline tests)

**Interfaces (produces):**

```rust
pub(crate) struct SplitCaptured { pub success: bool, pub timed_out: bool, pub stdout: Vec<u8>, pub stdout_overflowed: bool, pub stderr: String }
pub(crate) fn run_captured_split(command: Command, stdin: Option<&[u8]>, cancel: &dyn Fn() -> bool, timeout: Duration, stdout_limit: usize, stderr_limit: usize) -> Result<SplitCaptured, RunError>;
```

- [ ] **Step 1: Failing tests** (`#[cfg(unix)]`, using `/bin/sh -c`):
  - `split_keeps_stdout_and_stderr_apart` — `printf out; printf err >&2` → `stdout == b"out"`, `stderr == "err"`.
  - `split_feeds_stdin` — `cat` echoes the input.
  - `split_times_out_and_kills_the_group` — `sleep 30 & sleep 30; wait` with 300 ms → `timed_out`, returns in < 5 s (reuse the existing grandchild test's shape).
  - `split_cancel_stops_the_program` — cancel flag flips after 100 ms → `Err(RunError::Cancelled)`.
  - `split_reports_stdout_overflow` — `head -c 2000 /dev/zero` with `stdout_limit = 1000` → `stdout_overflowed`.
  - `split_spawn_failure_is_spawn_error` — nonexistent program → `RunError::Spawn` with `NotFound` (every platform).
- [ ] **Step 2:** Run `cargo test -p git-core process::` → fail.
- [ ] **Step 3: Implement:** extract the existing poll loop (`try_wait` / `cancel()` / deadline / `kill_and_reap`) from `run_captured` into `fn wait_bounded(child: &mut Child, cancel, timeout) -> Result<(ExitStatus, bool /*timed_out*/), RunError>`; `run_captured` keeps its behaviour on top of it. `run_captured_split`: `Stdio::piped()` for stdout and stderr, one drain thread each (stdout: keep the first `stdout_limit` bytes and set overflow when more arrive, keep draining; stderr: `TailBuffer`), stdin writer thread as today, `process_group(0)` / `CREATE_NO_WINDOW` as today, join drains with `DRAIN_GRACE`.
- [ ] **Step 4:** `cargo test -p git-core` passes (hooks tests prove `run_captured` unchanged). Commit `git-core: run_captured_split for programs with separate stdout/stderr (#110)`.

---

## Task P2: Signing with timeout and cancel

**Files:** Modify `crates/git-core/src/signing.rs`; Create `crates/git-core/tests/signing_timeout.rs`

**Interfaces (produces):** `pub const SIGN_TIMEOUT: Duration = 120 s; pub const VERIFY_TIMEOUT: Duration = 30 s;` `pub struct SignOptions<'a> { pub cancel: &'a dyn Fn() -> bool, pub timeout: Duration }` + `Default`; `sign_buffer_with(config, committer, buffer, &SignOptions)`; `create_commit_with(repo, head_update, author, committer, message, tree, parents, &SignOptions)`; `SignError::TimedOut { seconds: u64 }`, `SignError::Cancelled`.

- [ ] **Step 1: Failing tests** (`tests/signing_timeout.rs`, `#![cfg(unix)]`, `mod common;`): a fake signer script in a `TempDir` (`#!/bin/sh\nsleep 30\n`, chmod 755) configured as `gpg.program` with `commit.gpgSign=true` on a repo from `common::init_repo()` (then override `commit.gpgSign` to true).
  - `a_hung_signer_times_out` — `create_commit_with(.., &SignOptions { timeout: Duration::from_millis(500), ..Default::default() })` → `Err(SignError::TimedOut { seconds: 0 })` within 5 s; HEAD unchanged.
  - `a_cancelled_signer_stops` — cancel returns true after 200 ms → `Err(SignError::Cancelled)`; HEAD unchanged.
  - `a_signer_printing_too_much_fails_instead_of_truncating` — script `head -c 2000000 /dev/zero; echo '[GNUPG:] SIG_CREATED ' >&2` → `SignerFailed` with the overflow copy.
  - `a_missing_signer_is_still_signer_not_found` — `gpg.program=browsitory-no-such-gpg` → `SignerNotFound`.
  - `a_hung_verifier_reports_an_error_status` — `verify_commit` on a signed commit (create one with a fake signer that prints a fixed armored block and the `SIG_CREATED` status line) with a hung verifier script and a test-only short timeout → `Ok(CommitSignature)` with error status mentioning `timed out`. Expose `verify_commit_with(repo, id, timeout)` (`pub` with doc "tests and callers needing a shorter bound").
- [ ] **Step 2:** Run `cargo test -p git-core --test signing_timeout` → fail.
- [ ] **Step 3: Implement:** `run_program(program, args, stdin, &SignOptions)` builds the `Command` (args, `CREATE_NO_WINDOW` handled by the runner) and calls `run_captured_split(.., stdout_limit = 1 << 20, stderr_limit = 64 << 10)`; map `RunError::Spawn(NotFound)` → `SignerNotFound`, other spawn/wait errors → `Io`, `Cancelled` → `SignError::Cancelled`, `timed_out` → `TimedOut { seconds: timeout.as_secs() }`, `stdout_overflowed` → `SignerFailed("the signature was larger than 1048576 bytes.")` (Display yields the exact copy). `sign_buffer`/`create_commit` call the `_with` variants with `SignOptions::default()`. Verification uses `VERIFY_TIMEOUT` and turns `TimedOut`/`Cancelled` into the existing error-status path.
- [ ] **Step 4:** `cargo test -p git-core --test signing_timeout --test signing_gpg --test signing_ssh` pass (gpg/ssh tests skip without the programs, as today). Commit `git-core: signing runs with a timeout and cancel (#110)`.

---

## Task P3: Cancel signing from the commit box

**Files:** Modify `crates/git-core/src/commit.rs` (+ `tests/stage_commit.rs`), `frontend/src/lib/hookFailure.ts` (+ its test), `frontend/src/components/CommitBox.test.tsx`

- [ ] **Step 1: Failing tests:**
  - `crates/git-core/tests/stage_commit.rs` (`#[cfg(unix)]`): `commit_reports_signing_to_the_hook_reporter_and_honours_cancel` — fake hung signer; a recording `HookReporter`; `HookOptions { cancel: Some(&|| started.elapsed() > 200ms), reporter: Some(&mut rec), .. }`; `commit::commit` → error whose text is `Signing was cancelled.`; `rec.started == ["signing"]` (plus any installed hooks — none here) and `rec.finished` contains `"signing"`.
  - `frontend/src/lib/hookFailure.test.ts`: `runningHookLabel("signing") === "Waiting for the signing program…"`; `runningHookLabel("pre-commit") === "Running pre-commit hook…"` (unchanged).
  - `CommitBox.test.tsx`: with `hooks.running = { operationId: "op1", hook: "signing" }` the status shows `Waiting for the signing program…` and the Cancel button calls `onCancel`.
- [ ] **Step 2:** Run → fail.
- [ ] **Step 3: Implement:**
  - `commit::commit` and `amend_head` (wherever `HookOptions` is in scope): if `signing::signing_config(repo)?` is `Some`, call `reporter.started("signing")`, then `create_commit_with(.., &SignOptions { cancel: hooks.cancel.unwrap_or(&|| false), timeout: SIGN_TIMEOUT })`, then `reporter.finished("signing", &Ok(HookOutcome::NotInstalled))` (an outcome the worker's `log_hook_result` ignores; comment that "signing" is a pseudo-hook for the status line only). Make sure the frontend never treats a `signing` finish as a hook failure (it only clears `hookRun` — verify in `useHookActions.ts`).
  - `runningHookLabel`: `hook === "signing" ? "Waiting for the signing program…" : \`Running ${hook} hook…\``.
- [ ] **Step 4:** `cargo test -p git-core --test stage_commit`, `cargo test -p repo-service`, `cd frontend && pnpm test -- --run` pass. Commit `Cancel a waiting signer from the commit box (#110)`.

---

## Task D1: Per-file diffs keep only the requested path (#116)

**Files:** Modify `crates/git-core/src/diff.rs`, `crates/git-core/src/stage.rs`; Test `crates/git-core/tests/diff.rs`, `tests/compare.rs`, `tests/stage_lines.rs`

- [ ] **Step 1: Failing tests:**
  - `tests/compare.rs`: `range_diff_of_a_file_that_became_a_directory_shows_only_that_file` — commit A: `d` = `file d\n`; commit B: `d` removed, `d/inner.txt` = `inner\n`. `range_diff_files(A, B)` lists `d` and `d/inner.txt`; `range_diff(A, B, "d", None)` has exactly one hunk whose lines are `[Remove "file d\n"]`; `range_diff(A, B, "d/inner.txt", None)` is `[Add "inner\n"]`.
  - `tests/compare.rs`: `working_tree_compare_of_a_file_that_became_a_directory_shows_only_that_file` — same against the working tree (`compare_working_tree(A, "d", None)`).
  - `tests/diff.rs`: `commit_diff_of_a_file_that_became_a_directory_shows_only_that_file`; `working_diff_unstaged_of_a_file_replaced_by_a_directory_shows_only_that_file` (tracked `d`, worktree `d/inner.txt` untracked: `working_diff("d", false)` = only the removal).
  - `tests/stage_lines.rs`: `discard_hunk_of_d_never_uses_a_hunk_of_d_slash_inner` — same worktree shape; take `(old_start, new_start)` of `d/inner.txt`'s hunk and call `discard_hunk(&repo, "d", ..)` → `Err(StageError::HunkNotFound)`; worktree unchanged.
- [ ] **Step 2:** Run the three test files → new tests fail.
- [ ] **Step 3: Implement:** `pub(crate) fn hunks_from_diff(diff: &git2::Diff, wanted: &[&str])`: in the file callback compute `keep = [old_file, new_file].iter().any(|f| f.path().is_some_and(|p| wanted.iter().any(|w| p == Path::new(w))))` into a `Cell<bool>`; hunk and line callbacks return early when `!keep`. Update all five callers (`working_diff` `[path]`, `commit_diff` `[path]`, `commit_rename_diff` `[old, new]`, `range_diff`/`compare_working_tree` `[path] + old_path`). `stage::find_hunk(diff, path, old_start, new_start)` gets the same delta filter (a matching hunk in another delta is ignored).
- [ ] **Step 4:** `cargo test -p git-core` passes. Commit `git-core: per-file diffs keep only the requested path (#116)`.

---

## Task D2: Working-tree compare shows hunks for conflicted files (#117)

**Files:** Modify `crates/git-core/src/diff.rs`; Test `crates/git-core/tests/compare.rs`
**Depends on:** S8 (`open_beneath`) for the fallback read.

- [ ] **Step 1: Failing tests:**
  - `a_conflicted_file_compares_against_its_working_tree_content` — `c.txt` conflicting merge (`git_core::merge::start_merge` → `Conflicted`); `compare_working_tree(base_commit, "c.txt", None)` is non-empty and its added lines include `<<<<<<< ours`-style marker lines (assert one `Add` line starts with `<<<<<<<`); `compare_working_tree_files` still lists `c.txt`.
  - `#[cfg(unix)] a_conflicted_path_replaced_by_a_symlink_is_not_read_through` — after the conflict, link `c.txt` to an outside file; the result contains no line `OUTSIDE\n`.
- [ ] **Step 2:** Run → fail (0 hunks).
- [ ] **Step 3: Implement** in `compare_working_tree`: after the diff, if a delta whose path equals `path` has `status() == Delta::Conflicted`, rebuild for that path with `repo.diff_tree_to_workdir(Some(&tree), Some(&mut literal_path_options(path, old_path, opts) + include_untracked_content))` and return `hunks_from_diff(&diff2, &[path])`. If that still yields a `Conflicted` delta with no hunks (confirm in Step 2's run), fall back to: old blob = `tree.get_path(old_path.unwrap_or(path))` (regular blob only), new bytes = `worktree_guard::open_beneath` read (missing/link ⇒ empty → all-removed), `git2::Patch::from_blob_and_buffer(old.as_ref(), Some(path), &bytes, Some(path), Some(&mut opts))` → new `hunks_from_patch(&patch)` helper (iterate `num_hunks`/`hunk`/`line_in_hunk`). Note in a comment that the fallback applies no clean filters.
- [ ] **Step 4:** Pass. Commit `git-core: working-tree compare shows conflicted files (#117)`.

---

## Task D3: Staged renames carry their old path (#137) — git-core

**Files:** Modify `crates/git-core/src/status.rs`, `tools.rs`, `diff.rs`, `stage.rs`; Test `tests/status.rs`, `tests/diff.rs`, `tests/tools.rs`, `tests/stage_commit.rs`

**Interfaces (produces):** `StatusEntry { path, old_path: Option<String>, staged, kind }`; `DiffSource::Staged { old_path: Option<String> }`; `diff::staged_rename_diff(repo, old_path, new_path, opts) -> Result<Vec<DiffHunk>, DiffError>`.

- [ ] **Step 1: Failing tests:**
  - `tests/status.rs`: extend `reports_a_staged_rename_as_staged_renamed` to assert `entries[0].path == "new.txt"` and `entries[0].old_path.as_deref() == Some("old.txt")`; new `a_moved_but_unstaged_file_is_deleted_plus_untracked` (rename on disk only → `old.txt` Deleted unstaged + `new.txt` New unstaged, both `old_path: None`); every other existing test gets `old_path: None` where it builds entries.
  - `tests/diff.rs`: `file_sides_of_a_staged_rename_compares_old_and_new` — `old.png` committed, renamed and staged; `file_sides(Staged { old_path: Some("old.png") }, "new.png")` = `(Loaded, Loaded)` with equal bytes; `Staged { old_path: None }` keeps today's `(Missing, Loaded)`. `staged_rename_diff_shows_only_the_edit` — rename + one-line edit staged → one hunk with that line.
  - `tests/tools.rs`: the diff-tool sides for `Staged { old_path: Some(old) }` read the HEAD side from `old` (mirror the existing `Commit { old_path }` test).
  - `tests/stage_commit.rs`: `unstaging_a_staged_rename_unstages_both_paths` — after `unstage_file(&repo, "new.txt")`, `status()` shows no staged entries (`old.txt` unstaged Deleted, `new.txt` untracked New).
- [ ] **Step 2:** Run → fail (path is `old.txt`; field/variant missing).
- [ ] **Step 3: Implement:**
  - `status.rs`: `renames_index_to_workdir(false)`; per entry: staged side path/old from `entry.head_to_index()` (`new_file().path()`, `old_file().path()` when `Renamed`), unstaged side from `entry.index_to_workdir()`; fall back to `entry.path()` when the delta is absent (conflicts). A staged rename with unstaged edits yields two entries with the new path (old_path only on the staged one).
  - `tools.rs`: `Staged { old_path }`; `prepare_diff_tool_with` uses `old_path.as_deref().unwrap_or(path)` for the HEAD side, like `Commit`.
  - `diff.rs`: `file_sides` reads the old side of `Staged` from `old_rel`; add `staged_rename_diff` (HEAD tree → index, `literal_path_options(new, Some(old))`, `detect_renames`, `hunks_from_diff(&diff, &[old, new])`).
  - `stage.rs::unstage_file`: before `reset_default`, look up a staged rename whose new path is `path` (`statuses` with `renames_head_to_index(true)` and pathspec-free iteration, or a `diff_tree_to_index` + `find_similar` restricted to the two candidates) and reset `[old, new]` together.
  - Fix all compile errors across the workspace from the struct/enum change (`repo-service`, `tauri-app`, `vscode-sidecar` construct or match these) with `old_path: None` / `Staged { old_path: None }` — wire changes come in D4.
- [ ] **Step 4:** `cargo test --workspace` passes. Commit `git-core: staged renames report their old path (#137)`.

---

## Task D4: `oldPath` on the wire (#137)

**Files:** Modify `crates/repo-service/src/worker/status.rs`, `worker/mod.rs`; `crates/tauri-app/src/commands/status.rs`, `commands/mod.rs`; `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs`

- [ ] **Step 1: Failing tests:**
  - `tauri-app` inline DTO tests: `StatusEntryDto` serializes `oldPath` (`null` and `"old.txt"`); `DiffSourceDto` parses `{"kind":"Staged"}` → `Staged { old_path: None }` and `{"kind":"Staged","oldPath":"a.png"}` → `Some`.
  - sidecar `tests/protocol_roundtrip.rs`: `get_status` returns `oldPath` for a staged rename; `get_working_diff` with `oldPath` returns the rename diff (one edit hunk).
  - `repo-service` worker test: `get_working_diff("new.txt", staged=true, Some("old.txt"))` → the edit hunk only.
- [ ] **Step 2:** Run → fail.
- [ ] **Step 3: Implement:** `StatusEntryDto { path, old_path: Option<String> /* camelCase oldPath, always present */, staged, kind }` in both Tauri and sidecar; `DiffSourceDto::Staged { #[serde(default)] old_path }` (`#[serde(rename_all = "camelCase")]`) in both; `Command::GetWorkingDiff { .., old_path: Option<String> }` → `staged && old_path.is_some()` ⇒ `diff::staged_rename_diff`, else `working_diff`; `WorkerHandle::get_working_diff(path, staged, opts, old_path)`; Tauri `get_working_diff(.., old_path: Option<String>)`; sidecar params `#[serde(default)] old_path`.
- [ ] **Step 4:** `cargo test --workspace` passes. Commit `Wire oldPath for staged renames (#137)`.

---

## Task D5: Frontend — staged renames (#137)

**Files:** Modify `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts` (+ their tests and any `StatusEntry` fixtures), `frontend/src/components/DiffPane.tsx` (+ `DiffPane.test.tsx`); check `FileActionsMenu.tsx`, `state/useStagingActions.ts`, `state/useAppState.ts` for `Renamed` handling

- [ ] **Step 1: Failing tests** (mock `RepoClient`):
  - `DiffPane.test.tsx`: a staged entry `{ path: "new.png", oldPath: "old.png", staged: true, kind: "Renamed" }` → `getFileSides` called with `{ kind: "Staged", oldPath: "old.png" }`; `getWorkingDiff` called with `oldPath "old.png"`; header shows `Renamed from old.png`; "Open in diff tool" passes `{ kind: "Staged", oldPath: "old.png" }`.
  - Client tests: `getWorkingDiff` sends `oldPath` (default `null`) to `get_working_diff` in both clients.
- [ ] **Step 2:** `pnpm test -- --run` → fail.
- [ ] **Step 3: Implement:** `StatusEntry.oldPath: string | null`; `DiffSource` `{ kind: "Staged"; oldPath: string | null }` (update every construction site — `grep -rn 'kind: "Staged"' frontend/src`); `getWorkingDiff(repoPath, path, staged, opts, oldPath: string | null = null)`; `DiffPane` passes `entry.oldPath` and renders the existing `Renamed from` element for staged renames. Update fixtures (`oldPath: null`). Selection keys stay `path`+`staged`.
- [ ] **Step 4:** `pnpm test -- --run`, `pnpm lint`, `pnpm build`; `cd extension && pnpm test -- --run && pnpm run compile`. Commit `Frontend: staged renames preview old vs new (#137)`.

---

## Task Z1: Docs and changelog

**Files:** `CHANGELOG.md`, `docs/ARCHITECTURE.md`, `docs/LICENSE_COMPLIANCE.md`

- [ ] **Step 1: CHANGELOG** under `## [Unreleased]`, new `### Security` above `### Fixed`:
  - `Discarding changes, hard reset, undo/redo, aborting a merge, cherry-pick, revert or rebase, resolving a conflict, and "Stash all" no longer write (or delete) through a symbolic link that replaced a tracked file or folder; the link is removed and the file restored, like git. Previously a file outside the repository could be overwritten (#114).`
  - `The image preview reads working-tree files without following a folder that is swapped for a symbolic link while it reads (#138).`
  
  `### Fixed` additions:
  - `Committing with a signing program that never finishes (for example a passphrase prompt that never appears) times out after 2 minutes, and the commit box shows "Waiting for the signing program…" with Cancel (#110).`
  - `The diff of a file that became a folder (or the reverse) no longer shows the folder's files (#116).`
  - `Comparing with the working tree shows the changes of a conflicted file (#117).`
  - `A staged rename shows its new name, previews old against new, and unstages as a whole; a file moved but not staged shows as deleted plus new, like git status (#137).`
- [ ] **Step 2: ARCHITECTURE.md:** a short "Worktree writes and symlinks" subsection: every forced checkout calls `worktree_guard::scan` + `clear_hazards` first; raw writes use `write_beneath`/`remove_beneath`; work-tree reads use `open_beneath`; the regression matrix lives in `tests/symlink_write_through.rs`; residual race documented (spec §1.3). Note the signing timeouts and `run_captured_split`.
- [ ] **Step 3: LICENSE_COMPLIANCE.md:** extend the `libc` row's usage text with `openat`/`O_DIRECTORY`/`O_CLOEXEC` for `open_beneath`; no new row (D4-A).
- [ ] **Step 4:** Commit `Docs and changelog for the security and bug fix wave`.

## Task Z2: Verification and PR

- [ ] `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
- [ ] `cd frontend && pnpm lint && pnpm test -- --run && pnpm build`; `cd extension && pnpm test -- --run && pnpm run compile`.
- [ ] Manual (Linux): repro from #114 in `cargo tauri dev` against a scratch repo with an outside target in a temp dir — Stash all, Discard, Reset hard, Undo: target untouched. Staged rename of a PNG previews Before/After.
- [ ] Push (pre-push hook passes with the changelog), open the PR against `main` listing the D1–D5 answers used, the stress-test result, and "Closes #114, #138, #110, #116, #117, #137".

---

## Decision hooks

| Decision | If the user picks… | Change |
|---|---|---|
| D1-B (refuse) | refuse instead of removing | S1 adds `HazardError { path }`; S2–S7 return it instead of `clear_hazards`; S9 rows become `_is_refused`; changelog wording |
| D1-C | refuse only ancestors | as D1-B for `Hazard::Ancestor` only |
| D2-B | always own stash | S7 routes every stash-all; add parity tests for submodules / intent-to-add / ignored |
| D2-C | refuse stash all | S7 returns a new `StashError::SymlinkInWorktree(path)` |
| D3-B | 600 s, no UI | P2 constants; drop P3's frontend part (keep cancel plumbing) |
| D3-C | configurable | new `config` setting + Preferences field (new task after P3) |
| D4-B | `windows-sys` | S8 Step 4 uses it; Z1 adds the license row (`cargo info windows-sys`) in the dependency commit |
| D4-C | document only | drop S8 Step 4; ARCHITECTURE notes the Windows limitation |
| D5-B | keep unstaged renames | D3 keeps `renames_index_to_workdir(true)`; `discard_file`/`stage_file` act on both paths; extra tests |
