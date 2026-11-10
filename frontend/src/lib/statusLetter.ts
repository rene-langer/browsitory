import type { StatusEntry } from "../ipc/RepoClient";

const isUntracked = (entry: StatusEntry) => !entry.staged && entry.kind === "New";

/** A status letter and the kind it stands for, as a row shows them (#639). */
export function statusLetter(entry: StatusEntry): { letter: string; kind: string } {
  switch (entry.kind) {
    case "New":
      return isUntracked(entry) ? { letter: "U", kind: "Untracked" } : { letter: "A", kind: "Added" };
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
