import { useCallback } from "react";
import type { CreatePullRequest, ForgeProvider, PullRequestList, RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import { errorAfterOwnSuccess, type OwnError } from "./ownError";
import { failureError, type RunMutationWithMessage } from "./useMutationRunner";

export interface ForgeActions {
  listPullRequests(remoteName: string, account: string): Promise<void>;
  // Resolves to `null` once saved, else why not: the Connect dialog shows it (#1012).
  saveForgeToken(provider: ForgeProvider, account: string, token: string): Promise<string | null>;
  // Resolves to `null` once forgotten, else the failure (also in `state.error`): the card then
  // stays connected, since the token is still saved.
  forgetForgeToken(provider: ForgeProvider, account: string): Promise<string | null>;
  // Resolves to `null` once created, else the failure, which the New pull request dialog shows
  // with the typed fields kept (#1012); the failure also goes to `state.error`, as before.
  createPullRequest(remoteName: string, account: string, pullRequest: CreatePullRequest): Promise<string | null>;
  openExternalUrl(url: string): Promise<void>;
}

export function useForgeActions(
  client: RepoClient,
  repoPath: string,
  runMutationWithMessage: RunMutationWithMessage,
  setState: (updater: (prev: AppState) => AppState) => void,
  // The error a failed pull request list set: a later successful list clears that one only, never
  // another action's (#177).
  ownError: OwnError,
): ForgeActions {
  const listPullRequests = useCallback(
    async (remoteName: string, account: string) => {
      try {
        const result = await client.listPullRequests(repoPath, remoteName, account);
        const own = ownError.take();
        setState((prev) => ({
          ...prev,
          pullRequests: { ...prev.pullRequests, [remoteName]: result },
          error: errorAfterOwnSuccess(prev.error, own),
        }));
      } catch (err) {
        // Drop this remote's entry (rather than leaving whatever was there before) so a failed
        // re-list can't leave stale, possibly-successful-looking rows on screen under this
        // remote's heading. Other remotes' entries are untouched — a failure listing remote B
        // must never affect what's shown for remote A.
        const failure = failureError(err);
        ownError.set(failure.error);
        setState((prev) => {
          const rest = { ...prev.pullRequests };
          delete rest[remoteName];
          return { ...prev, pullRequests: rest, ...failure };
        });
      }
    },
    [client, repoPath, setState, ownError],
  );
  const saveForgeToken = useCallback(
    (provider: ForgeProvider, account: string, token: string) =>
      runMutationWithMessage(() => client.saveForgeToken(repoPath, provider, account, token)),
    [client, runMutationWithMessage, repoPath],
  );
  const forgetForgeToken = useCallback(
    (provider: ForgeProvider, account: string) =>
      runMutationWithMessage(() => client.forgetForgeToken(repoPath, provider, account)),
    [client, runMutationWithMessage, repoPath],
  );
  const createPullRequest = useCallback(
    (remoteName: string, account: string, pullRequest: CreatePullRequest): Promise<string | null> =>
      runMutationWithMessage(async () => {
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
    [client, runMutationWithMessage, repoPath, setState],
  );
  const openExternalUrl = useCallback(
    (url: string) => client.openExternalUrl(url),
    [client],
  );

  return { listPullRequests, saveForgeToken, forgetForgeToken, createPullRequest, openExternalUrl };
}
