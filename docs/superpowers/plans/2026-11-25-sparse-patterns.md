# Sparse patterns for Reset, the aborts and Undo: plan

Design: `docs/superpowers/specs/2026-11-25-sparse-patterns-design.md`. A test-only commit (red
run quoted), then the implementation commit.

## Tests

`crates/git-core/tests/sparse_patterns.rs`. The git CLI builds every repository; where git has
the same operation, a twin repository built the same way runs it, and `git ls-files -v -s`,
`git status --short`, `git rev-parse HEAD^{tree}` and the work tree's files (paths and
contents) must be equal. History: `one` has `app/main.rs`, `docs/guide.md`, `lib/top.rs`,
`lib/core/c.rs`, `lib/other/o.rs`, `top.txt`; `two` adds `app/new.rs`, `docs/new.md`,
`docs/sub/s.md`, `lib/new.rs`, `lib/core/deep/d.rs`, `lib/other/n.rs`, `top2.txt` and changes
`docs/guide.md`. HEAD at `one`, `git sparse-checkout set app lib/core` (cone).

1. Reset mixed to `two` equals `git reset --mixed`; Browsitory's status lists nothing under
   `docs/` or `lib/other/`.
2. Reset hard to `two` equals `git reset --hard`.
3. With `core.ignorecase` on, paths `LIB/CORE/up.rs`, `LIB/up.rs`, `DOCS/up.md` added in
   `two`: Reset mixed equals git.
4. A linked worktree with its own `git sparse-checkout set app` (the main one not sparse):
   Reset mixed in the worktree equals git there.
5. `core.sparseCheckout` on and `info/sparse-checkout` deleted: Reset mixed and hard equal git
   (nothing new marked; hard also writes the missing marked `docs/guide.md` it changes, as git
   applies no sparse checkout without patterns).
6. Non-cone (`git sparse-checkout set --no-cone 'app/' '/top*.txt'`): Reset mixed to `two`
   gives `ls-files -v` with every new path `S` (all their files are missing), `git status
   --short` and Browsitory's status empty; the paths outside the patterns have git's marks.
7. Non-cone, Reset hard to `two`: no new file written, every new path `S`, status empty.
8. Cone mode on, a pattern git doesn't accept as cone (`/*.md`): read as non-cone, Reset mixed
   marks `app/new.rs` too.
9. Undo (`capture`/`restore`, `RefsAndIndex`) of a Reset mixed from `two` back to `one` in a
   sparse checkout at `two`: `ls-files -v -s` and `status --short` as before the reset.
10. Abort rebase: `main` has `mine1` (conflicts with `onto` in `app/main.rs`) then `mine2`
    (adds `docs/only.md`); sparse `app`. `start_rebase` onto `onto` pauses at `mine1`;
    `abort_rebase` equals `git rebase onto` + `git rebase --abort`.
11. Abort merge: `topic` changes `app/main.rs`, deletes `docs/guide.md`, adds `docs/new.md`
    and `docs/h.md`; `main` changes `app/main.rs` and adds `docs/h.md` differently; sparse
    `app`. `git merge topic` in both (libgit2's merge refuses to change the missing files), then
    `abort_merge` equals `git merge --abort`.

## Code

`index_marks::Sparse` (read, `contains`), `index_marks::marks_after`; callers `reset_to` /
`left_alone`, `merge::apply_operation_reset`, `rebase::restore_original_branch`,
`undo::restore`. Docs: `docs/ARCHITECTURE.md` (index marks, Undo), `docs/guide/
rewriting-commits.md`, `docs/guide/undo-redo.md`; changelog fragment `changelog.d/1172-*.md`.
