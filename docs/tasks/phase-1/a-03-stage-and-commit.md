# Task 1.A.03: `git-core::stage` and `git-core::commit`

## Goal

Add whole-file `stage_file`/`unstage_file` (`crates/git-core/src/stage.rs`) and `commit`
(`crates/git-core/src/commit.rs`). No hunk/line-level staging, no amend — message + commit only,
matching the design spec's Phase 1 scope. These two modules are small and their natural test
(stage a change, then commit it, then assert it landed) spans both, so they're one task.

## Depends on

None — new modules, only touch `crates/git-core`. (Not on 1.A.01/1.A.02 — independent
functionality; can be worked in either order relative to those.)

## Interfaces produced

`crates/git-core/src/stage.rs`:
```rust
use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StageError {
    #[error("failed to update the index: {0}")]
    Index(#[from] git2::Error),
    #[error("repository has no working directory (bare repository)")]
    NoWorkdir,
}

pub fn stage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    // ...
}

pub fn unstage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    // ...
}
```

`crates/git-core/src/commit.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommitError {
    #[error("failed to create commit: {0}")]
    Write(#[from] git2::Error),
}

/// Commits the current index. Author/committer come from `repo.signature()` (reads
/// `user.name`/`user.email` from git config — returns a clear `git2::Error` if unset).
/// Parent is the current `HEAD` commit if one exists, or no parents for a repo's first commit.
/// Returns the new commit's full hex OID.
pub fn commit(repo: &git2::Repository, message: &str) -> Result<String, CommitError> {
    // ...
}
```

`crates/git-core/src/lib.rs` gains `pub mod stage;` and `pub mod commit;`.

## Implementation notes

**`stage_file`** must handle both "add/update" (new or modified file) and "remove" (a tracked
file deleted from the worktree — staging that deletion means removing it from the index, and
`Index::add_path` errors if the file doesn't exist on disk). Branch on whether the file exists:
```rust
pub fn stage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
    let mut index = repo.index()?;
    if workdir.join(path).exists() {
        index.add_path(Path::new(path))?;
    } else {
        index.remove_path(Path::new(path))?;
    }
    index.write()?;
    Ok(())
}
```

**`unstage_file`** is simpler — `git2::Repository::reset_default` already does "restore this
path's index entry from the target commit, or remove it from the index if the target has no
entry for that path" in one call, which is exactly whole-file unstage semantics for every case
(modified, newly-added, or a staged deletion):
```rust
pub fn unstage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let target = head_commit.as_ref().map(|c| c.as_object());
    repo.reset_default(target, [path])?;
    Ok(())
}
```
On a repo with no commits yet (`head_commit` is `None`), `target` is `None` too — `reset_default`
with `target: None` simply removes the path from the index, which is correct: unstaging a newly
added file in a brand-new repo should make it untracked again.

**`commit`** mirrors `crates/git-core/tests/common/mod.rs`'s existing `commit_all` test helper
exactly (same pattern, now as production code):
```rust
pub fn commit(repo: &git2::Repository, message: &str) -> Result<String, CommitError> {
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let signature = repo.signature()?;

    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();

    let oid = repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &parents)?;
    Ok(oid.to_string())
}
```

## TDD requirement

`crates/git-core/tests/stage_commit.rs` (new file, `mod common;` + existing
`common::{init_repo, write_file}` — do not use `commit_all` here, since these tests exercise
`stage`/`commit` themselves):

- `stage_file_adds_a_new_file_to_the_index`: `write_file` an untracked file, call
  `stage_file(&repo, "new.txt")`, then call `git_core::status::status(&repo)` and assert the
  one entry is `staged: true, kind: StatusKind::New` (reuses Task 1's `status` module as the
  observable assertion — no need to inspect the index directly).
- `stage_file_stages_a_deletion`: `write_file` + `stage_file` + a real commit via
  `git_core::commit::commit(&repo, "add file")` (this test *does* use the module under test to
  create history, rather than `commit_all`, so it also exercises `commit` on a fresh repo).
  Then `std::fs::remove_file` the file, call `stage_file(&repo, "tracked.txt")`, assert
  `status()` shows one entry `staged: true, kind: StatusKind::Deleted`.
- `unstage_file_restores_the_index_entry_from_head`: commit a file via `git_core::commit::commit`,
  modify it, stage it, then `unstage_file(&repo, "tracked.txt")`; assert `status()` shows the
  path unstaged (`staged: false, kind: StatusKind::Modified`) — the modification is still in
  the worktree, just no longer staged.
- `unstage_file_on_a_newly_staged_file_makes_it_untracked_again`: `write_file` + `stage_file` on
  a repo with no commits yet, then `unstage_file`, assert `status()` shows
  `staged: false, kind: StatusKind::New`.
- `commit_creates_a_commit_with_the_given_message_and_staged_content`: `write_file` + `stage_file`,
  call `let oid = commit(&repo, "add greeting").unwrap()`, assert `status(&repo).unwrap().is_empty()`
  (nothing left uncommitted), then `repo.find_commit(git2::Oid::from_str(&oid).unwrap()).unwrap()`
  and assert `.message().unwrap() == "add greeting"`.
- `commit_on_a_fresh_repo_creates_a_parentless_first_commit`: `write_file` + `stage_file` on a
  brand-new repo (no prior commits), call `commit`, then assert the resulting commit's
  `.parent_count() == 0`.
- `commit_without_a_configured_identity_returns_an_error`: build a repo *without* `init_repo()`'s
  `user.name`/`user.email` config (use `tempfile::TempDir` + `git2::Repository::init` directly,
  skip the config step), `write_file` + `stage_file`, assert `commit(&repo, "msg").is_err()`.

Write these seven tests first (they'll fail to compile — `stage`/`commit` modules don't exist),
run `cargo test -p git-core --test stage_commit`, confirm compile failure, then implement
`stage.rs` and `commit.rs` and re-run until green.

## Acceptance criteria

- [ ] `cargo test -p git-core --test stage_commit` passes (all 7 tests).
- [ ] `cargo test --workspace` still passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo fmt --all -- --check` clean.
- [ ] Commit: `git add crates/git-core/src/lib.rs crates/git-core/src/stage.rs crates/git-core/src/commit.rs crates/git-core/tests/stage_commit.rs && git commit -m "feat(git-core): add stage_file, unstage_file, commit"`.

## Out of scope

Hunk/line-level staging. Amend-last-commit. Sign-off / GPG signing. Commit message templates or
validation beyond "non-empty" (not even that is enforced here — an empty-message commit is
allowed by `git2` and by this function; the UI task (`1.E.04`) disables the Commit button when
the message is empty, so this is a UI-layer concern, not a `git-core` one).
