import type { RemoteInfo, RepoClient, UpstreamInfo } from "../ipc/RepoClient";
import type { TransferProgress } from "../ipc/RepoClient";
import { fakeRepoClient, graphPage } from "../test/fakeRepoClient";

export function unimplemented(): never {
  throw new Error("not implemented in this fake");
}

export const TEST_REPO_PATH = "/repo";

export const remote: RemoteInfo = { name: "origin", fetchUrl: "../origin.git", pushUrl: null, authMode: null, authUsername: null };
export const upstream: UpstreamInfo = { localBranch: "main", remoteName: "origin", remoteBranch: "main" };

/**
 * A `RepoClient` for `useAppState` tests: an empty repository on `main` that tracks
 * `origin/main` ({@link remote}, {@link upstream}), with `overrides` on top. Transfers that
 * report an operation id answer fixed ids. Other methods answer as `fakeRepoClient`'s `"empty"`
 * mode does, or fail when called.
 */
export function transferClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(
    {
      getStatus: async () => [],
      getCommitGraph: async () => graphPage([]),
      listBranches: async () => [],
      listStashes: async () => [],
      listRemotes: async () => [remote],
      getCurrentUpstream: async () => upstream,
      getRemoteUpstreams: async () => [upstream],
      fetchAllRemotes: async () => "fetch-all-1",
      backgroundFetch: async () => null,
      pushBranch: async () => "push-1",
      deleteRemoteBranch: async () => "delete-remote-1",
      deleteRemoteTag: async () => "delete-remote-tag-1",
      setGraphBranchSelection: async () => {},
      setGraphRemoteBranchSelection: async () => {},
      clearGraphBranchSelection: async () => {},
      clearGraphRemoteBranchSelection: async () => {},
      logFrontendError: async () => {},
      ...overrides,
    },
    "empty",
  );
}

export function progress(
  operationId: string,
  operation: TransferProgress["operation"],
  phase: TransferProgress["phase"],
  extra: Partial<TransferProgress> = {},
): TransferProgress {
  return { operationId, operation, phase, errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null, ...extra };
}
