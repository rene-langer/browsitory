# Index integrity: skip-worktree, intent-to-add, stale in-memory index (design)

Status: decided 2026-11-25. Bundle b73: #1145, #1146, #1164, #1166, #1167.

## Problem

Two per-entry marks of git's index, and the worker's one in-memory copy of the index, are
handled differently from git:

- **skip-worktree** (`git update-index --skip-worktree`, every file outside a sparse checkout's
  cone). libgit2's `git_index_read_tree` clears the mark on every entry, so Reset (mixed, hard)
  and Abort (merge, cherry-pick, revert; rebase with a case pair) drop it from the whole index
  (#1145). Reset hard's forced checkout also writes over a skip-worktree file's local edits, and
  Undo's snapshot (built from the status, which hides the file) can't bring them back. libgit2's
  status reports a skip-worktree file that is missing as deleted, so a sparse checkout lists
  every file outside its cone as deleted, and Stage all stages those deletions (#1146).
- **intent-to-add** (`git add -N`). libgit2's status reports the entry (the empty blob) as staged
  new, and `write_tree` commits it as an empty file (#1164).
- **The worker's in-memory index.** `repo.index()` is shared by every command of the worker. A
  stage that fails on the index write leaves its change in memory, and the next successful write
  saves it (#1166). While an operation is paused, no undo snapshot re-reads the index before a
  command, so a write saves the copy from the last status read over what a terminal staged
  since (#1167).

## What git does (checked with git 2.56 in throwaway tests)

| Case | git |
| --- | --- |
| `reset --mixed` | keeps `S` on every entry that stays; drops intent-to-add entries (file untracked) |
| `reset --hard`, skip-worktree entry, target has the same blob | file and `S` left alone (also when the file is missing) |
| `reset --hard`, target changes it, file edited | refuses: "Entry 'x' not uptodate. Cannot merge." |
| `reset --hard`, target changes it, file as in the index | writes the target's version, keeps `S` |
| `reset --hard`, target changes it, file missing | sparse checkout: stays missing, `S` kept; otherwise written |
| `reset --hard`, intent-to-add entry | entry dropped, file kept as untracked |
| `merge --abort` | keeps `S` |
| `status` | missing skip-worktree file not listed; intent-to-add file ` A` (or ` D` when missing) |
| `add <skip-worktree path>` | refuses ("outside of your sparse-checkout definition"); `add -A` skips it |
| `commit` | leaves intent-to-add entries out of the tree |

## Design

One module, `git_core::index_marks`, holds what the callers share:

- `reset_index_to_tree(index, tree)`: `read_tree`, then puts skip-worktree back on every stage-0
  path that had it and is still in the index; intent-to-add entries go, as with git. The
  write is followed, on failure, by `index.read(true)` so nothing of the failed change stays in
  memory (#1166). Used by `reset_to`, `apply_operation_reset` (Abort merge, cherry-pick,
  revert, the terminal sequence's abort), `restore_original_branch`'s case-pair path, and
  `set_case_pair_entries`; Undo's restore (which already refuses an index with these marks)
  only gets the re-read on a failed write.
- `committed_tree(repo, index)`: the index's tree without intent-to-add entries, written from an
  in-memory copy when there are any (else `write_tree` as before). Used for every commit made
  from the repository's index: commit, amend, fold, the paused pick/revert/rebase steps, split,
  patch apply.
- `edited_skip_worktree` moves here from `rebase.rs` (Reset hard needs it too).

Reset hard, when the index holds skip-worktree or intent-to-add entries: refuses (nothing
changed) when a skip-worktree file is edited and the target changes it, naming the file; else
checks out every path of the target, HEAD and the index except the skip-worktree paths whose
blob the target keeps, those missing in a sparse checkout, and intent-to-add paths the target
doesn't have (their file stays, untracked). Without such entries nothing changes.

Status (`status::entries`, which now gets the index): a `WT_DELETED` entry whose stage-0 entry
is skip-worktree is dropped; an intent-to-add entry is one unstaged entry, New (Deleted when its
file is missing). `stage_file` refuses a skip-worktree path with git's reason.

`stage_file` (and its untracked-folder form) re-read the index on any error (#1166).
`WorkerLoop::before` re-reads the index (`read(false)`, a stat call when nothing changed)
before every command (#1167).

## Decisions (safest choice; owner may revisit)

1. Reset hard of an edited skip-worktree file that the target changes is **refused**, like git,
   rather than overwritten.
2. Missing skip-worktree files a hard reset changes stay missing only with `core.sparseCheckout`
   on (git applies the cone there); otherwise they are written, as git does. Paths new in the
   target that lie outside the cone are written and not marked (Browsitory doesn't read sparse
   patterns).
3. An intent-to-add file is listed as an unstaged new file; staging it stages its content.
4. A stale in-memory index is re-read before every command rather than refused.
