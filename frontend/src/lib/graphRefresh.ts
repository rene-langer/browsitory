import type { GraphCommit, GraphPage } from "../ipc/RepoClient";

/** The rows a refresh answer makes of the rows the client sent as `loaded` (#835). Stub. */
export function applyGraphRefresh(_loaded: GraphCommit[], page: GraphPage): GraphCommit[] {
  return page.commits;
}
