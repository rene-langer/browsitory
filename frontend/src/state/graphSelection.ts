import type { GraphSelection } from "../ipc/RepoClient";

/**
 * The graph's saved branch selection (#498): `local` `null` = no filter saved (every local
 * branch), `remote` `null` = no remote selection saved (no remote branch walked, every remote
 * badge shown). The client takes a `GraphSelection` instead, where the remote list is never null.
 */
export interface SavedGraphSelection {
  local: string[] | null;
  remote: string[] | null;
}

/**
 * What the backend walks for a saved selection: a never-saved remote selection walks none.
 * `collapseMerges` (#617) adds `firstParent`, sent only when on.
 */
export function toGraphSelection(saved: SavedGraphSelection, collapseMerges = false): GraphSelection {
  const selection: GraphSelection = { local: saved.local, remote: saved.remote ?? [] };
  return collapseMerges ? { ...selection, firstParent: true } : selection;
}
