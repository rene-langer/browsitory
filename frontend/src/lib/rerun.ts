import type { Rerun } from "../ipc/RepoClient";

/**
 * The workspace actions Run again starts (#890): the same ones the toolbar and menus use, so a
 * rerun shows its progress, errors and toasts like the first run, and is a new log entry.
 */
export interface RerunActions {
  fetchRemote(remoteName: string): unknown;
  fetchAllRemotes(): unknown;
  pullCurrentUpstream(): unknown;
  pushCurrentBranch(remoteName: string, noVerify: boolean): unknown;
  pushBranch(remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean, noVerify: boolean): unknown;
  pushTags(remoteName: string, names: string[], noVerify: boolean): unknown;
}

/**
 * Runs a logged fetch, pull or push again with the parameters it was given. A fetch's prune
 * follows the repository's current setting, as every fetch the app starts does.
 */
export function runRerun(rerun: Rerun, actions: RerunActions): void {
  switch (rerun.kind) {
    case "Fetch":
      void actions.fetchRemote(rerun.remoteName);
      return;
    case "FetchAll":
      void actions.fetchAllRemotes();
      return;
    case "Pull":
      void actions.pullCurrentUpstream();
      return;
    case "PushCurrentBranch":
      void actions.pushCurrentBranch(rerun.remoteName, rerun.noVerify);
      return;
    case "PushBranch":
      void actions.pushBranch(rerun.remoteName, rerun.localBranch, rerun.remoteBranch, rerun.setUpstream, rerun.noVerify);
      return;
    case "PushTags":
      void actions.pushTags(rerun.remoteName, rerun.names, rerun.noVerify);
      return;
  }
}

/**
 * Why `rerun` can't run in the repository as it is now (`null`: it can): the remote or local
 * branch it named is gone. Anything else that changed (the upstream, the tags) the operation
 * itself reports, as a new failed entry.
 */
export function rerunBlockedReason(rerun: Rerun, state: { remotes: string[]; branches: string[] }): string | null {
  if (rerun.kind === "FetchAll") {
    return state.remotes.length === 0 ? "The repository has no remotes any more." : null;
  }
  if (rerun.kind === "Pull") return null;
  if (!state.remotes.includes(rerun.remoteName)) return `The remote ${rerun.remoteName} no longer exists.`;
  if (rerun.kind === "PushBranch" && !state.branches.includes(rerun.localBranch)) {
    return `The branch ${rerun.localBranch} no longer exists.`;
  }
  return null;
}
