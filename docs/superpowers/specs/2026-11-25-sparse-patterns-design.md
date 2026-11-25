# Sparse patterns for Reset, the aborts and Undo (design)

Status: decided 2026-11-25. Bundle b79: #1172.

## Problem

#1145 made Reset, Abort and Undo keep the skip-worktree marks the index has. A path *new* to
the index gets no mark, because Browsitory doesn't read the sparse patterns
(`.git/info/sparse-checkout`). In a sparse checkout:

- **Reset mixed** to a commit that adds files outside the cone leaves them unmarked and
  missing: the status lists each as deleted, and Stage all would commit their deletion.
- **Reset hard** writes them to disk, unmarked.
- **Abort rebase** (`check_out_original`, a forced checkout of the whole branch tree) writes
  every missing file outside the cone and rewrites their entries without the mark.
- **Abort merge/cherry-pick/revert** puts a path the operation removed from the index back
  unmarked and writes its file.
- **Undo** adds paths back to the index unmarked (#1174's decision 4).

## What git does (git 2.56, checked in throwaway repositories)

| Case | git |
| --- | --- |
| `reset --mixed`, path with an index entry (any stage) | keeps its stage-0 mark; a conflicted path ends unmarked |
| `reset --mixed`, path without any index entry | marked when outside the sparse patterns (`builtin/reset.c`, `path_in_sparse_checkout`) |
| `reset --hard` | same marks; a marked path's file isn't written |
| `rebase --abort` | as `reset --hard` to the original branch |
| `merge --abort` | same marks (a conflict outside the cone: unmarked, HEAD's version written) |
| `core.sparseCheckout` on, no `info/sparse-checkout` file | nothing new is marked; `reset --hard` writes a missing marked file it changes |

Cone mode (`core.sparseCheckoutCone`) matches a path as `dir.c`'s `path_matches_pattern_list`:
in when the file is at the top level, its folder is a "parent" (`/a/` then `!/a/*/`), or a
folder above it is "recursive" (`/a/b/`); `/*` alone includes everything; folders compare
ignoring case under `core.ignorecase`. A pattern that doesn't fit the cone format turns cone
matching off (git warns "disabling cone pattern matching") and the patterns are read as
non-cone patterns. Linked worktrees have their own file (`.git/worktrees/<name>/info/
sparse-checkout`) and `core.sparseCheckout` in their `config.worktree`.

git's `reset --hard` and `rebase --abort` (unpack-trees) also re-apply the patterns to entries
already in the index: an unmarked file outside the cone is marked and removed, a marked one
inside is unmarked and written. That only matters when the marks don't match the patterns
(a manual `update-index --skip-worktree`, an operation that wrote files outside the cone);
it is not done here (decision 2).

## Design

`index_marks::Sparse` reads the patterns of the repository's work tree:
`Off` (`core.sparseCheckout` off, or the file missing or unreadable), `Cone` (the parent and
recursive folder sets, or everything), or `Patterns` (non-cone).

`index_marks::marks_after(repo, index, tree) -> Marks`, called before anything is written:

- `skip_worktree`: every stage-0 path marked now (as before), plus each path of `tree` with no
  index entry at any stage that lies outside the patterns. With non-cone patterns, "outside"
  is "its file is missing" (decision 1).
- `added`: those new marked paths; the checkouts leave them out.

Callers:

- `reset_to`: both modes; Reset hard's `left_alone` also holds `added`, so its checkout
  doesn't write them (and `left_alone` runs in a sparse checkout even when the index has no
  marked entry yet). Its "missing marked file in a sparse checkout" rule now needs readable
  patterns (`Sparse` not `Off`): without the file, `git reset --hard` applies no sparse
  checkout and writes such a file.
- `apply_operation_reset` (Abort merge, cherry-pick, revert, a terminal sequence's step):
  `added` is left out of the touched paths' checkout.
- `restore_original_branch` (Abort rebase, a failed start's rollback): checks out every path
  except Reset hard's `left_alone` (marked files the tree keeps, missing marked files in a
  sparse checkout, `added`) and the case pairs, then writes the index with the marks, as Reset
  hard does. An edited skip-worktree file the tree changes is left as it is (Abort must
  work; git would refuse).
- Undo's `restore`: `reset_index_marking` takes the marks from `marks_after`.

## Decisions (safest choice; owner may revisit)

1. **Non-cone patterns** aren't matched (gitignore-style wildcards, negation, last match
   wins): a path new to the index is marked when its file is missing, so nothing outside the
   sparse set is written or listed as deleted. Inside the patterns that differs from git,
   which writes (hard) or lists as deleted (mixed) such a file; `git sparse-checkout reapply`
   brings them in.
2. Entries already in the index keep their mark as they are; the patterns aren't re-applied
   to them (git's `reset --hard`/`rebase --abort` do).
3. Abort rebase leaves an edited skip-worktree file that the original branch changes as it
   is, marked, rather than refusing the abort.
4. A sparse index (`index.sparse`, directory entries) isn't supported by libgit2 at all;
   unchanged.
