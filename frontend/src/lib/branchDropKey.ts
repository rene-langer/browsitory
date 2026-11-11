import type { RebaseTarget } from "./actions/branchActions";

/** A row's drop key: `local:<name>` or `remote:<remote>:<branch>`, as its selection key. */
export function branchDropKey(ref: RebaseTarget): string {
  return ref.kind === "local" ? `local:${ref.name}` : `remote:${ref.remoteName}:${ref.branchName}`;
}

/** The branch a drop key names; `null` for a folder header's key. */
export function parseBranchDropKey(key: string): RebaseTarget | null {
  if (key.startsWith("local:")) return { kind: "local", name: key.slice("local:".length) };
  if (key.startsWith("remote:")) {
    // A branch name has no ":" (git refuses it in ref names), so the last one splits.
    const rest = key.slice("remote:".length);
    const split = rest.lastIndexOf(":");
    if (split < 0) return null;
    return { kind: "remote", remoteName: rest.slice(0, split), branchName: rest.slice(split + 1) };
  }
  return null;
}
