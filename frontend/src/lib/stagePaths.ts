import type { StatusEntry } from "../ipc/RepoClient";
import { unconvertible } from "./externalFilters";
import { isUntracked } from "./statusLetter";

// `git-core::status` reports conflicted entries with `staged: false`, so they sit in the
// "Changes" group, but staging a conflicted path is what *marks the conflict resolved*, with
// whatever happens to be in the working tree. One bulk click would silently resolve every
// outstanding conflict, so the bulk actions skip them. The per-row Stage control on a conflicted
// file is left alone: that's a deliberate, one-file-at-a-time action.
//
// A file under a filter driver Browsitory can't run (`externalFilter`, #1087) or with a
// working-tree-encoding it can't convert (`workingTreeEncoding`, #1156) is refused by the backend;
// the bulk actions leave it out so they stage everything else.

const stageable = (entry: StatusEntry) =>
  !entry.staged && entry.kind !== "Conflicted" && unconvertible(entry) === undefined;

/**
 * The paths "Stage all" stages: every unstaged entry but conflicts and files Browsitory can't
 * filter or convert, untracked files included.
 */
export function stageAllPaths(status: StatusEntry[]): string[] {
  return status.filter(stageable).map((entry) => entry.path);
}

/**
 * The paths "Stage modified" stages (#621): the unstaged changes of tracked files (modified,
 * deleted, type changes, renames, and a file added with `git add -N`, as `git add -u` stages it,
 * #1173), leaving untracked files, conflicts and files Browsitory can't filter or convert alone.
 */
export function stageModifiedPaths(status: StatusEntry[]): string[] {
  return status.filter((entry) => stageable(entry) && !isUntracked(entry)).map((entry) => entry.path);
}

/**
 * The paths "Delete untracked…" deletes (#622): the untracked entries in the Changes group
 * (`isUntracked` in `statusLetter`), never tracked changes, conflicts or ignored files. Status
 * lists the files inside untracked folders one by one; a folder entry (a nested repository) is
 * refused by `discard_all_unstaged` and named in its error, as the per-file "Delete file" does.
 */
export function untrackedPaths(status: StatusEntry[]): string[] {
  return status.filter(isUntracked).map((entry) => entry.path);
}
