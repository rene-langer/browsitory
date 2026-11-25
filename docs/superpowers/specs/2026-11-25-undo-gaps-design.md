# Undo gaps: paused operations, branch settings, sparse checkouts, big files (design)

Status: decided 2026-11-25. Bundle b77 (milestone 0.17.0): #1147, #1151, #1154, #1104, #1089,
#1174. Builds on #1176 (`index_marks`, `reset_index_to_tree`) and #1106 (untracked-file limit,
`undo.json` version 2).

## Problems

- **#1147 (high).** The commands that destroy work-tree content (policy `busy=Discard`) are meant
  to be recorded or refused. While an operation is paused (`UndoRecorder::begin` returns early)
  or the index has conflicts (`capture` fails with `ConflictedIndex`, only logged) they run
  unrecorded, and the confirmation still says Undo can bring the content back.
- **#1151.** A snapshot holds HEAD, `refs/heads/*`, `refs/tags/*`, index and work tree, but no
  configuration. Undo of Rename branch or Delete branch puts the ref back without its
  `branch.<name>.*` section (upstream, rebase, description…); a rename leaves the section
  behind under the new name.
- **#1154.** `CreateBranch { checkout: true }` is recorded with `RefsAndIndex`; when the start
  point isn't HEAD's commit the switch rewrites the work tree, and Undo leaves those files there.
- **#1104.** `FullUncapped` stores every untracked file, so discarding one hunk next to a big
  un-ignored folder reads and writes all of it twice.
- **#1089.** Undo/Redo of a discard read a big file whole twice: `verify` hashes it from a
  whole read, `write_worktree_file` loads the whole blob.
- **#1174.** `restore` refuses an index holding any skip-worktree or intent-to-add entry with
  "the repository changed since this operation", so in a sparse checkout every Undo or Redo that
  changes the index is refused, without saying why.

## Design

### Work-tree entries (#1147)

A `busy=Discard` command that runs while an operation is paused (a marker, or a pending entry)
or while the index has conflicts gets an entry of its own, a **work-tree entry**:

- Its snapshots are taken by `undo::capture_worktree`: HEAD, refs and the work tree as for
  `FullUncapped` (narrowed as below), and **no index** (`RepoSnapshot::index_tree` is `None`);
  a conflicted index doesn't stop it.
- It goes straight onto the undo stack (never into the pending entry), and Undo and Redo may
  run it while an operation is paused: `undo::restore_worktree` restores only the work-tree
  files, after the usual checks of each file (still as the entry left it, else
  `RepositoryChanged`). It writes raw bytes (no checkout from the index: there is no recorded
  index), and never touches HEAD, refs or the index. Any other entry still answers "operation
  in progress" during the pause.
- Only commands that change nothing but the work tree qualify (`UndoPlan::worktree_only`):
  discard hunk, lines, file, all unstaged, restore a file version, apply a commit's change
  without staging it, apply a patch to the work tree. The others (`Reset hard`, apply and
  stage a patch or a commit's change, apply a patch as commits) are **refused** in that state,
  with a message that says why (nothing could take them back): "Undo couldn't take this back
  while a merge, rebase, cherry-pick or revert is paused. Finish or abort it first." or "…while
  the index has conflicts. Resolve them first."
- A `busy=Discard` command whose snapshot can't be taken at all (an I/O error) is refused too
  ("Couldn't save the files for undo, so nothing was changed: …"), instead of running unrecorded.

Ordering: a work-tree entry recorded during a pause sits below the operation's entry once that
finishes. Undoing the operation first restores the state before it; the work-tree entry then
refuses if its files no longer are as it left them. An aborted operation leaves its work-tree
entries, which still undo (their files are checked as always).

### Branch settings (#1151)

`RepoSnapshot::branch_config`: each local branch's `branch.<name>.*` entries of the
repository's own config file (`ConfigLevel::Local`), in file order, keyed by branch name (the
subsection, split from the variable at the last dot). `changed_refs_only` keeps them only for
branches whose ref the entry changes. `restore` handles a branch's settings only when the
restore creates or deletes that branch (its ref exists on one side only) and the two sides'
settings differ: it checks that the current settings are the expected side's (else
`RepositoryChanged`, like a ref) and writes the target's (all of the branch's entries removed,
then the target's added). So Undo of a rename moves the settings back, Undo of a delete brings
them back, Redo does the reverse; settings changed by unrecorded commands (Set upstream) are
never touched by an entry that doesn't create or delete that branch.

### Create and switch (#1154)

`CreateBranch` is recorded with `Full` when `checkout` is true (like `SwitchBranch`),
`RefsAndIndex` otherwise.

### Discards store whole only what they touch (#1104)

`UndoPlan::whole`: the paths a `FullUncapped` command can change (`None`: every file). The
recorder snapshots with `undo::capture_keeping(repo, paths)`: those files, and everything below
those folders, are stored whatever their size or number; every other file under `Full`'s limits
(50 MB, 10,000 untracked). Paths are matched ignoring case, so a spelling difference can only
store more. Per command: Discard hunk/lines/file and Restore file version: the path; Discard
all: the confirmed paths; Apply a commit's change: its path and old path; Apply patch (every
form): the patch's old and new paths (`patch::touched_paths`; `None` when it can't be parsed);
Reset hard: every file.

### Streaming Undo of big files (#1089)

- `verify`, `already_restored` and `holds_blob` compare a work-tree file with a blob without
  hashing a whole read: different sizes (from the opened handle's stat) differ at once,
  otherwise the file is compared with the blob in pieces (the blob through `Odb::reader`, a
  stream, falling back to the loaded blob where the odb can't stream it, e.g. a packed object).
- `write_worktree_file` streams the blob into `replace_beneath`'s temporary file (new
  `Content::Stream`), with the same fallback.

### Sparse checkouts and intent-to-add (#1174)

- `restore` no longer refuses skip-worktree entries: it writes the index with
  `index_marks::reset_index_to_tree`, which keeps the skip-worktree mark on every path that
  keeps an entry (as git's reset does, #1145).
- `RepoSnapshot::intent_to_add`: the index's intent-to-add paths. `restore` checks them with the
  index tree (the current index must be one side's tree *and* marks) and puts the mark back on
  the target's paths after writing the index.

### `undo.json` version 3

The snapshot gains `branchConfig` and `intentToAdd` (optional) and `indexTree` may be absent
(work-tree entries). A version 2 build would misread such a file (restore intent-to-add entries
as empty files, fail to parse a work-tree entry and overwrite the file), so the format becomes
version 3: this build reads 1 to 3 and writes 3; older builds see an unknown version, start
empty and never overwrite it (#1093).

## Decisions (safest choice; owner may revisit)

1. While an operation is paused or the index has conflicts, Reset hard and the staging forms of
   apply patch / apply a commit's change are **refused** rather than recorded with an index
   that can't be restored. After a conflicted stash pop, Reset hard needs the conflicts marked
   resolved first.
2. A `busy=Discard` command whose snapshot fails is refused, not run unrecorded.
3. Branch settings restored: every `branch.<name>.*` entry of the repository's own config file,
   only for branches the restore creates or deletes. Settings in global or worktree config
   files are left alone.
4. Undo/Redo in a sparse checkout keep the current skip-worktree marks (paths the restore adds
   back to the index aren't marked, as with Reset, #1172); intent-to-add marks are recorded and
   restored.
5. Discard snapshots keep whole only the paths the command names; a Discard all keeps the
   confirmed list. An unparseable patch keeps every file.
