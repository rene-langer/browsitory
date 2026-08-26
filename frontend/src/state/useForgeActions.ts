import { useCallback } from "react";
import type { CreatePullRequest, ForgeProvider, PullRequestList, RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import { credentialFailureMessage, type RunMutation, type RunMutationWithOutcome } from "./useMutationRunner";

export interface ForgeActions {
  listPullRequests(remoteName: string, account: string): Promise<void>;
  saveForgeToken(provider: ForgeProvider, account: string, token: string): Promise<void>;
  forgetForgeToken(provider: ForgeProvider, account: string): Promise<void>;
  // `Promise<boolean>` (unlike the other forge actions above) so `PullRequestPanel` can tell a
  // failed creation from a successful one and only clear its form on success — `runMutation`
  // swallows the underlying error into `state.error` rather than rejecting, so the boolean is
  // the only success/failure signal available to the caller. Matches `renameRemote`/
  // `setRemoteAuthMode`'s existing `Promise<boolean>` pattern in `useRemoteActions`.
  createPullRequest(remoteName: string, account: string, pullRequest: CreatePullRequest): Promise<boolean>;
  openExternalUrl(url: string): Promise<void>;
}

export function useForgeActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runMutationWithOutcome: RunMutationWithOutcome,
  setState: (updater: (prev: AppState) => AppState) => void,
): ForgeActions {
  const listPullRequests = useCallback(
    async (remoteName: string, account: string) => {
      try {
        const result = await client.listPullRequests(repoPath, remoteName, account);
        setState((prev) => ({
          ...prev,
          pullRequests: { ...prev.pullRequests, [remoteName]: result },
          error: null,
        }));
      } catch (err) {
        // Drop this remote's entry (rather than leaving whatever was there before) so a failed
        // re-list can't leave stale, possibly-successful-looking rows on screen under this
        // remote's heading. Other remotes' entries are untouched — a failure listing remote B
        // must never affect what's shown for remote A.
        setState((prev) => {
          const rest = { ...prev.pullRequests };
          delete rest[remoteName];
          return { ...prev, pullRequests: rest, error: credentialFailureMessage(err) };
        });
      }
    },
    [client, repoPath, setState],
  );
  const saveForgeToken = useCallback(
    (provider: ForgeProvider, account: string, token: string) =>
      runMutation(() => client.saveForgeToken(repoPath, provider, account, token)),
    [client, runMutation, repoPath],
  );
  const forgetForgeToken = useCallback(
    (provider: ForgeProvider, account: string) =>
      runMutation(() => client.forgetForgeToken(repoPath, provider, account)),
    [client, runMutation, repoPath],
  );
  const createPullRequest = useCallback(
    (remoteName: string, account: string, pullRequest: CreatePullRequest): Promise<boolean> =>
      runMutationWithOutcome(async () => {
        const created = await client.createPullRequest(repoPath, remoteName, account, pullRequest);
        setState((prev) => {
          const existing = prev.pullRequests[remoteName];
          const updated: PullRequestList = {
            pullRequests: [created, ...(existing?.pullRequests ?? [])],
            truncated: existing?.truncated ?? false,
          };
          return {
            ...prev,
            pullRequests: { ...prev.pullRequests, [remoteName]: updated },
          };
        });
      }),
    [client, runMutationWithOutcome, repoPath, setState],
  );
  const openExternalUrl = useCallback(
    (url: string) => client.openExternalUrl(url),
    [client],
  );

  return { listPullRequests, saveForgeToken, forgetForgeToken, createPullRequest, openExternalUrl };
}
