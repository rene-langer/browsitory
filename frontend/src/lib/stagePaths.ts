import type { StatusEntry } from "../ipc/RepoClient";

// `git-core::status` reports conflicted entries with `staged: false`, so they sit in the
// "Changes" group, but staging a conflicted path is what *marks the conflict resolved*, with
// whatever happens to be in the working tree. One bulk click would silently resolve every
// outstanding conflict, so the bulk actions skip them. The per-row Stage control on a conflicted
// file is left alone: that's a deliberate, one-file-at-a-time action.

/** The paths "Stage all" stages: every unstaged entry but conflicts, untracked files included. */
export function stageAllPaths(status: StatusEntry[]): string[] {
  return status.filter((entry) => !entry.staged && entry.kind !== "Conflicted").map((entry) => entry.path);
}

/**
 * The paths "Stage modified" stages (#621): the unstaged changes of tracked files (modified,
 * deleted, type changes, renames), leaving untracked files and conflicts alone.
 */
export function stageModifiedPaths(status: StatusEntry[]): string[] {
  return status
    .filter((entry) => !entry.staged && entry.kind !== "Conflicted" && entry.kind !== "New")
    .map((entry) => entry.path);
}

/**
 * The paths "Delete untracked…" deletes (#622): the untracked entries in the Changes group
 * (`isUntracked` in `FileActionsMenu`), never tracked changes, conflicts or ignored files. Status
 * lists the files inside untracked folders one by one; a folder entry (a nested repository) is
 * refused by `discard_all_unstaged` and named in its error, as the per-file "Delete file" does.
 */
export function untrackedPaths(status: StatusEntry[]): string[] {
  return status.filter((entry) => !entry.staged && entry.kind === "New").map((entry) => entry.path);
}
