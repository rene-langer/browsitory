import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { isHostVisible, subscribeHostVisibility } from "../ipc/hostVisibility";
import type { RepoClient, TransferErrorKind, TransferProgress } from "../ipc/RepoClient";
import {
  firstNotice,
  lastAutoFetchAttempt,
  lastFetchAt,
  recordAutoFetchAttempt,
  recordFetch,
  subscribeLastFetch,
} from "../lib/autoFetch";
import type { ToastAction } from "./useToasts";

export interface AutoFetchOptions {
  client: RepoClient;
  repoPath: string;
  /** The effective "Fetch automatically" interval; 0 is off. */
  minutes: number;
  /** The repository's Prune choice, as a manual fetch sends it (`null`: git config decides). */
  prune: boolean | null;
  /** A mutation or a transfer of this tab is running: wait, and stop a background fetch. */
  busy: boolean;
  refresh: () => Promise<void>;
  /** A non-blocking notice (a toast); never a dialog, never focus. */
  notify: (message: string, action?: ToastAction) => void;
  /** Opens the remote's Credentials dialog, offered on a "no saved token" notice. */
  openCredentials: (remoteName: string) => void;
}

interface FailedRemote {
  remoteName: string;
  errorKind: TransferErrorKind | null;
}

interface Collected {
  failed: FailedRemote[];
  terminal: TransferProgress | null;
}

// `inFlight` while `backgroundFetch` has not answered yet: the id isn't known.
const STARTING = "\u0000starting";

function noticeText(remoteName: string, errorKind: TransferErrorKind | null): string | null {
  const skipped = `Fetch automatically skipped ${remoteName}`;
  if (errorKind === "MissingCredential") return `${skipped}: no HTTPS token is available without asking you. Fetch once yourself, or save a token.`;
  if (errorKind === "CredentialStoreFailure") return `${skipped}: the operating-system credential store is locked or unavailable.`;
  if (errorKind === "SshAgentFailure") return `${skipped}: the SSH agent has no key for it.`;
  return null;
}

/** Whether the user can see the app (window shown, VSCode panel in front). */
export function useHostVisible(): boolean {
  return useSyncExternalStore(subscribeHostVisibility, isHostVisible);
}

/**
 * "Fetch automatically" (#440) for the mounted (active) repository tab: every `minutes`, a quiet
 * Fetch all through `RepoClient.backgroundFetch`, then a refresh. It waits while `busy` or while
 * the app is hidden, and stops a running background fetch when `busy` turns on, so a user's
 * action never queues behind it. The backend skips the tick (`null`) while any other operation of
 * the repository is queued or running. A remote that needs a credential it can't get is reported
 * once per session as a notice; other failures stay quiet and the next tick tries again. The
 * backend gives up on a remote that doesn't answer and skips it for a while (#462), so a cancel
 * reaches even a fetch that is still connecting.
 */
export function useAutoFetch({
  client,
  repoPath,
  minutes,
  prune,
  busy,
  refresh,
  notify,
  openCredentials,
}: AutoFetchOptions): void {
  const visible = useHostVisible();
  const lastFetched = useSyncExternalStore(subscribeLastFetch, () => lastFetchAt(repoPath));
  const [schedule, setSchedule] = useState(0);
  const inFlight = useRef<string | null>(null);
  const collected = useRef(new Map<string, Collected>());
  const latest = useRef({ prune, minutes, busy, refresh, notify, openCredentials });
  useEffect(() => {
    latest.current = { prune, minutes, busy, refresh, notify, openCredentials };
  });

  const finish = useCallback(
    (operationId: string) => {
      const entry = collected.current.get(operationId);
      if (entry?.terminal == null) return;
      collected.current.clear();
      inFlight.current = null;
      const { refresh, notify, openCredentials } = latest.current;
      if (entry.terminal.phase === "Completed") {
        recordFetch(repoPath);
        void refresh();
      }
      for (const { remoteName, errorKind } of entry.failed) {
        const text = noticeText(remoteName, errorKind);
        if (text === null || !firstNotice(repoPath, remoteName)) continue;
        notify(
          text,
          errorKind === "MissingCredential"
            ? { label: "Credentials…", run: () => openCredentials(remoteName) }
            : undefined,
        );
      }
      setSchedule((n) => n + 1);
    },
    [repoPath],
  );

  // Background fetch events, by operation id: they can arrive before `backgroundFetch` answers.
  useEffect(
    () =>
      client.subscribeTransferProgress((progress) => {
        if (progress.operation !== "BackgroundFetch") return;
        if (progress.phase !== "RemoteFailed" && progress.phase !== "Completed" && progress.phase !== "Failed") return;
        const entry = collected.current.get(progress.operationId) ?? { failed: [], terminal: null };
        if (progress.phase === "RemoteFailed") {
          if (progress.remoteName != null) entry.failed.push({ remoteName: progress.remoteName, errorKind: progress.errorKind });
        } else {
          entry.terminal = progress;
        }
        collected.current.set(progress.operationId, entry);
        if (inFlight.current === progress.operationId) finish(progress.operationId);
      }),
    [client, finish],
  );

  const start = useCallback(async () => {
    if (latest.current.busy || inFlight.current !== null) return;
    recordAutoFetchAttempt(repoPath);
    inFlight.current = STARTING;
    let operationId: string | null = null;
    try {
      operationId = await client.backgroundFetch(repoPath, latest.current.prune, Math.max(1, Math.round(latest.current.minutes)));
    } catch {
      // The repository went away or the backend refused: quiet; the next tick tries again.
    }
    if (operationId === null) {
      inFlight.current = null;
      setSchedule((n) => n + 1);
      return;
    }
    inFlight.current = operationId;
    if (latest.current.busy) void client.cancelTransfer(repoPath, operationId).catch(() => {});
    finish(operationId);
  }, [client, finish, repoPath]);

  // A user action started: let it go first instead of queueing behind the background fetch.
  useEffect(() => {
    const operationId = inFlight.current;
    if (!busy || operationId === null || operationId === STARTING) return;
    void client.cancelTransfer(repoPath, operationId).catch(() => {});
  }, [busy, client, repoPath]);

  useEffect(() => {
    if (minutes <= 0) return;
    // The first interval counts from when the schedule first applied to this repository, busy
    // or hidden or not.
    if (lastAutoFetchAttempt(repoPath) === null) recordAutoFetchAttempt(repoPath);
    if (!visible || busy || inFlight.current !== null) return;
    const interval = minutes * 60_000;
    // A manual fetch also counts: no background fetch right after the user fetched.
    const base = Math.max(lastAutoFetchAttempt(repoPath) ?? 0, lastFetched ?? 0);
    const timer = setTimeout(() => void start(), Math.max(base + interval - Date.now(), 0));
    return () => clearTimeout(timer);
  }, [minutes, visible, busy, repoPath, lastFetched, schedule, start]);
}
