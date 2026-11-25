# Index integrity: plan

Design: `docs/superpowers/specs/2026-11-25-index-integrity-design.md`. Each task: a test-only
commit (red run quoted), then the implementation commit.

## Task 1: skip-worktree in status and staging (#1146)

Tests, `crates/git-core/tests/skip_worktree.rs` (git CLI sets up and checks):

- sparse checkout (`app/main.rs`, `docs/guide.md`, `vendor/lib/x.c`, `top.txt`, `git
  sparse-checkout set app`): `status` and `status_refreshing_index` are empty.
- one file marked `--skip-worktree` and removed: `status` is empty.
- `stage_file("docs/guide.md")` in the sparse checkout is `Err(StageError::SkipWorktree(_))`;
  `stage_paths` with it stages nothing; `git status --short` stays empty.

Code: `status::entries` takes the index; `StageError::SkipWorktree`.

## Task 2: intent-to-add in status and commits (#1164)

Tests, `crates/git-core/tests/intent_to_add.rs`:

- `new.txt` added with `git add -N`, `a.txt` staged: status is staged Modified `a.txt`,
  unstaged New `new.txt`, nothing else; with `new.txt` removed: unstaged Deleted `new.txt`.
- `commit` and `amend_head` in that state: `git ls-tree -r --name-only HEAD` has no `new.txt`,
  `git status --short` shows ` A new.txt`.

Code: `index_marks::committed_tree`, used by every commit from the repository's index.

## Task 3: Reset and Abort keep skip-worktree; Reset hard keeps local edits (#1145)

Tests, `crates/git-core/tests/skip_worktree.rs`:

- `reset_to(HEAD~1, Mixed)`: `git ls-files -v` keeps `S` on both marked files; status empty.
- `reset_to(HEAD~1, Hard)`, target has the same blob: file keeps its local content and `S`.
- `reset_to(HEAD~2, Hard)`, target changes an edited skip-worktree file: refused naming the file;
  HEAD, file and `S` unchanged.
- target changes an unedited skip-worktree file: target's content written, `S` kept.
- sparse checkout, target changes a file outside the cone: file stays missing, `S` kept.
- `start_merge` conflicted, `abort_merge`: `S` kept, local content kept.
- intent-to-add file, `reset_to(HEAD, Hard)`: content kept, untracked.

Code: `index_marks::reset_index_to_tree`, Reset hard's limited checkout.

## Task 4: no failed change left in the in-memory index (#1166)

Tests: git-core `stage_file` with `.git/index.lock` held fails; after the lock goes,
`repo.index().write()` (what the next command does) leaves `a.txt` unstaged. Same for
`reset_to(Mixed)`. Worker: failed `stage_file("a.txt")`, then `stage_file("b.txt")`: `git status
--short` shows ` M a.txt`, `M  b.txt`.

## Task 5: re-read the index before each worker command (#1167)

Test (worker): paused merge, `get_status`, terminal `git add` of the resolution and of `c.txt`,
app `stage_file("b.txt")`: `git status --short` is `M  a.txt`, `M  b.txt`, `M  c.txt`.

Code: `WorkerLoop::before` re-reads the index.

## Task 6: docs, changelog

`docs/guide/` (status, reset), `docs/ARCHITECTURE.md`, `changelog.d/1145-index-integrity.md`.
