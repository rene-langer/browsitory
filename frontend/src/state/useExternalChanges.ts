import { useCallback, useEffect, useRef, useState } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import { useHostVisible } from "./useAutoFetch";

/** How often the visible tab checks for changes made outside the app, at the least. */
export const CHANGE_CHECK_INTERVAL_MS = 3_000;
/** The longest wait between two checks, however slow a check is. */
export const CHANGE_CHECK_MAX_INTERVAL_MS = 60_000;
/** A check may take at most this share of the time: a slow one stretches the interval. */
const CHECK_DUTY_FACTOR = 10;
/**
 * How often the visible tab checks while a watcher reports its changes (#756): a fallback for the
 * changes a watcher misses (some network file systems report none).
 */
export const WATCHED_CHECK_INTERVAL_MS = 30_000;

export interface ExternalChangesOptions {
  client: RepoClient;
  repoPath: string;
  /** The workspace's tab is the one shown. */
  active: boolean;
  /** An action or a transfer of this tab is running: no checks, its own refresh follows. */
  busy: boolean;
  /** `useAppState`'s `checkForChanges`. */
  checkForChanges: (wanted: () => boolean) => Promise<void>;
}

/**
 * Shows changes made outside the app (#650): a terminal, an editor, another Git tool. While the
 * tab is shown, the app is visible and nothing of this tab is running, it compares the
 * repository's change stamp with the one its last refresh read, and refreshes when they differ.
 * It checks every few seconds, and at once when the window gets the focus back (unless a watcher
 * runs, see below), when the app becomes visible again and when the tab is shown again. A slow check (a very large work tree)
 * stretches the interval, so checking never takes more than about a tenth of the time. At most
 * one check runs at a time; the checks of a hidden tab or a hidden app stop.
 *
 * While the workspace is mounted, the backend watches the repository (#756): a change it reports
 * starts a check at once, and the timer only checks every `WATCHED_CHECK_INTERVAL_MS` as a
 * fallback. A report for a hidden tab is dropped; showing the tab checks anyway. When the watcher
 * can't start or stops, the checks go back to every few seconds.
 */
export function useExternalChanges({ client, repoPath, active, busy, checkForChanges }: ExternalChangesOptions): void {
  const visible = useHostVisible();
  const latest = useRef({ active, busy, visible, checkForChanges });
  useEffect(() => {
    latest.current = { active, busy, visible, checkForChanges };
  });
  const running = useRef(false);
  const lastDuration = useRef(0);
  // Counts finished checks, so the timer below starts over after each one.
  const [checks, setChecks] = useState(0);

  // A watcher's report that came while a check ran: that check may have read the stamp before
  // the change, so another one follows.
  const again = useRef(false);
  const [reruns, setReruns] = useState(0);

  const check = useCallback(async (reported = false) => {
    const wanted = () => latest.current.active && latest.current.visible && !latest.current.busy;
    if (!wanted()) return;
    if (running.current) {
      again.current ||= reported;
      return;
    }
    running.current = true;
    const started = Date.now();
    try {
      await latest.current.checkForChanges(wanted);
    } finally {
      lastDuration.current = Date.now() - started;
      running.current = false;
      setChecks((n) => n + 1);
      if (again.current) {
        again.current = false;
        setReruns((n) => n + 1);
      }
    }
  }, []);
  const watching = useRepoWatch(client, repoPath, () => check(true));

  useEffect(() => {
    if (reruns > 0) void check(true);
  }, [reruns, check]);

  // Shown again (the tab, or the app), or idle again after an action of this tab.
  useEffect(() => {
    if (active && visible && !busy) void check();
  }, [active, visible, busy, check]);

  // Not while watched (#841): the watcher reports changes made meanwhile, and the slow poll
  // covers the file systems it misses.
  useEffect(() => {
    if (!active || watching) return;
    const onFocus = () => void check();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [active, check, watching]);

  useEffect(() => {
    if (!active || !visible || busy) return;
    const polled = Math.min(
      CHANGE_CHECK_MAX_INTERVAL_MS,
      Math.max(CHANGE_CHECK_INTERVAL_MS, lastDuration.current * CHECK_DUTY_FACTOR),
    );
    const delay = watching ? Math.max(WATCHED_CHECK_INTERVAL_MS, polled) : polled;
    const timer = setTimeout(() => void check(), delay);
    return () => clearTimeout(timer);
  }, [active, visible, busy, checks, check, watching]);
}

/**
 * Watches `repoPath` while mounted (#756) and calls `onChange` for each change the watcher
 * reports. Returns whether the watcher runs: `false` until `watchRepo` answers, when it can't
 * start, and after it reports `Stopped`.
 */
function useRepoWatch(client: RepoClient, repoPath: string, onChange: () => Promise<void>): boolean {
  const [watching, setWatching] = useState(false);
  const latestOnChange = useRef(onChange);
  useEffect(() => {
    latestOnChange.current = onChange;
  });

  useEffect(() => {
    let mounted = true;
    const stop = client.subscribeRepoWatch((event) => {
      if (!mounted || event.repoPath !== repoPath) return;
      if (event.kind === "Stopped") setWatching(false);
      else void latestOnChange.current();
    });
    client.watchRepo(repoPath).then(
      (started) => {
        if (mounted) setWatching(started);
      },
      // Polling, as without a watcher; the transport has logged the failure.
      () => {},
    );
    return () => {
      mounted = false;
      stop();
      setWatching(false);
      void client.unwatchRepo(repoPath).catch(() => {});
    };
  }, [client, repoPath]);

  return watching;
}
