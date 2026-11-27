import type { StatusEntry } from "../ipc/RepoClient";

/** An unstaged new file: git status lists it as untracked, unless it was added with `git add -N` (#1173). */
export const isUntracked = (entry: StatusEntry) => !entry.staged && entry.kind === "New" && entry.intentToAdd !== true;

/** A status letter and the kind it stands for, as a row shows them (#639); untracked is "?", as in `git status --short` (#1027). */
export function statusLetter(entry: StatusEntry): { letter: string; kind: string } {
  switch (entry.kind) {
    case "New":
      return isUntracked(entry) ? { letter: "?", kind: "Untracked" } : { letter: "A", kind: "Added" };
    case "Modified":
      return { letter: "M", kind: "Modified" };
    case "Deleted":
      return { letter: "D", kind: "Deleted" };
    case "Renamed":
      return { letter: "R", kind: "Renamed" };
    case "TypeChange":
      return { letter: "T", kind: "Type changed" };
    case "Conflicted":
      return { letter: "C", kind: "Conflicted" };
  }
}
