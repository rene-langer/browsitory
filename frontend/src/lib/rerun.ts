import type { Rerun } from "../ipc/RepoClient";

/** The workspace actions Run again starts (#890). Not built yet. */
export interface RerunActions {
  fetchRemote(remoteName: string): unknown;
  fetchAllRemotes(): unknown;
  pullCurrentUpstream(): unknown;
  pushCurrentBranch(remoteName: string, noVerify: boolean): unknown;
  pushBranch(remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean, noVerify: boolean): unknown;
  pushTags(remoteName: string, names: string[], noVerify: boolean): unknown;
}

export function runRerun(_rerun: Rerun, _actions: RerunActions): void {}

export function rerunBlockedReason(_rerun: Rerun, _state: { remotes: string[]; branches: string[] }): string | null {
  return null;
}
