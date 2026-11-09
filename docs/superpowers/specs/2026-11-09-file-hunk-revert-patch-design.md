# Revert or apply a commit's file or hunk; save and apply patches (#448)

## Goal

Undo or bring over one file or one hunk of a commit without a full revert or cherry-pick, and
move changes around as patch files. Nothing here commits.

## Backend (`git_core::patch`)

- `apply_commit_change(commit, path, old_path, hunk, reverse, target)`: the file's diff against
  the first parent (or its reverse) with binary content, or the one hunk the commit view showed
  (found again with the view's `context_lines`, whitespace significant, as for staging) built
  with `line_patch` and trimmed to three lines of context on each side. `target` is the working
  tree, or the working tree and the index (`git apply --index`; not offered in the UI yet).
- `commits_patch(ids)`: `git format-patch` mails (`git2::Email::from_diff`, first parent,
  renames detected, binary included), `[PATCH n/m]` for several. `commit_file_patch`,
  `staged_patch`: plain diffs.
- `apply_patch(bytes)`: a plain diff or a mailbox. A mailbox is split at its `From <id> Mon Sep
  17 00:00:00 2001` lines and applied mail by mail; when one fails, the files the earlier ones
  touched are put back from a copy taken before the first (removed and rewritten, never written
  through a link), so the working tree is unchanged.
- Every apply: guard every path (relative, no `.git` component, no symlinked ancestor, no file
  in place of a folder, no link the patch treats as a file, no new name that only differs in
  case from an index entry where case folds), refuse a created file that already exists, move
  each text hunk to where its old lines are now, then libgit2's check pass, the hard-link
  separation (`hardlink::before_apply`), and the apply. libgit2 applies a hunk only at the exact
  line its header names (`apply.c`, no search), so without the move a commit's hunk would almost
  never apply to a file that changed above it. The move searches the nearest exact match to the
  expected line (no fuzz), simulating the earlier hunks of the file, and rewrites the header in
  the original patch text (libgit2's printer drops the headers of a parsed patch).

## Worker and transports

- Commands `ApplyCommitChange` and `ApplyPatch`: recorded with `Full` scope ("Revert hunk in
  a.txt from 1234567", "Apply patch"), refused while the state lock stays busy (like a discard),
  gate open (like staging and discards). `GetPatch` is a read.
- IPC: `apply_commit_change`, `get_patch`, `apply_patch` on both transports. Patch bytes travel
  as base64 (`PatchContent`), since a patch may hold any bytes.
- Host methods: `save_patch_file(suggestedName, content)` and `open_patch_file()` show the save
  and open dialogs and do the file I/O (Tauri's dialog plugin; VSCode's `showSaveDialog`,
  `showOpenDialog` and `workspace.fs` in the extension host). The webview never names a path.

## Frontend

- Commit file menu: Revert this file, Apply this file to the working tree, Save file as patch….
  Hunk headers in a commit's diff: Revert hunk, Apply hunk (disabled under Ignore whitespace and
  while `operationBlockers` blocks).
- Commit graph: Save as patch… (one commit), Save N commits as patch… (selection, oldest first).
- Staged heading: Save as patch…. Palette: Apply patch…, Save staged changes as patch….
- Results show as a toast (`patchOutcome`); failures in the error banner.
