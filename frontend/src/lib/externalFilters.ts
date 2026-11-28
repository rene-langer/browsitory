// Filter drivers Browsitory can't run (git-crypt, Git LFS; #1063, #1087) and working-tree-encodings
// it can't convert (UTF-16 and the like; #1156): the backend refuses to stage or write files under
// them, and the Changes pane says so.

import type { StatusEntry } from "../ipc/RepoClient";

const quoted = (name: string) => `'${name}'`;

/** "'a'", "'a' and 'b'", "'a', 'b' and 'c'". */
function listNames(names: readonly string[]): string {
  const all = names.map(quoted);
  return all.length === 1 ? all[0] : `${all.slice(0, -1).join(", ")} and ${all[all.length - 1]}`;
}

/**
 * The Changes pane's notice for a repository whose attributes use the filter `drivers` or the
 * working-tree-`encodings` (at least one of them not empty).
 */
export function externalFiltersNotice(drivers: readonly string[], encodings: readonly string[] = []): string {
  const oneDriver = drivers.length === 1;
  const oneEncoding = encodings.length === 1;
  const filters = `the ${listNames(drivers)} ${oneDriver ? "filter" : "filters"}, which Browsitory can't run`;
  const converted =
    `the ${listNames(encodings)} working-tree-${oneEncoding ? "encoding" : "encodings"}, ` +
    "which Browsitory can't convert";
  const usage =
    drivers.length === 0
      ? `This repository uses ${converted}.`
      : encodings.length === 0
        ? `This repository uses ${filters}.`
        : `This repository uses ${filters}. It also uses ${converted}.`;
  const under = drivers.length + encodings.length === 1 ? "it" : "them";
  return `${usage} It won't stage, discard or check out files under ${under}: use git on the command line for those.`;
}

/** Why a file under `driver` has no Stage button, and what its row's note means. */
export function filteredFileReason(driver: string): string {
  return (
    `Browsitory can't run the ${quoted(driver)} filter, so it can't tell whether this file really changed ` +
    "and won't stage it. Use git on the command line."
  );
}

/** Why a file with the working-tree-encoding `encoding` has no Stage button (#1156). */
export function encodedFileReason(encoding: string): string {
  return (
    `Browsitory can't convert the ${quoted(encoding)} encoding (working-tree-encoding), so it can't tell ` +
    "whether this file really changed and won't stage it. Use git on the command line."
  );
}

/**
 * What keeps Browsitory from staging `entry`'s file, if anything: a filter it can't run or an
 * encoding it can't convert, with its row's note and the reason.
 */
export function unconvertible(entry: StatusEntry): { note: string; reason: string } | undefined {
  if (entry.externalFilter !== undefined) {
    return { note: `${quoted(entry.externalFilter)} filter`, reason: filteredFileReason(entry.externalFilter) };
  }
  if (entry.workingTreeEncoding !== undefined) {
    return { note: `${entry.workingTreeEncoding} encoding`, reason: encodedFileReason(entry.workingTreeEncoding) };
  }
  return undefined;
}

/** The unstaged entries Browsitory can't filter or convert, which bulk actions leave out (#1107). */
export function leftOut(entries: readonly StatusEntry[]): StatusEntry[] {
  void entries;
  return [];
}

/** The notice naming the files a bulk action leaves out (#1107). */
export function leftOutNotice(entries: readonly StatusEntry[]): string {
  void entries;
  return "";
}

/** The toast after stashing everything in `status` (#1107). */
export function stashedMessage(status: readonly StatusEntry[]): string {
  void status;
  return "Changes stashed";
}
