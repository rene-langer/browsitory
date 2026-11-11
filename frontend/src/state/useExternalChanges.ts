import { useCallback, useEffect, useRef, useState } from "react";
import { useHostVisible } from "./useAutoFetch";

/** How often the visible tab checks for changes made outside the app, at the least. */
export const CHANGE_CHECK_INTERVAL_MS = 3_000;
/** The longest wait between two checks, however slow a check is. */
export const CHANGE_CHECK_MAX_INTERVAL_MS = 60_000;
/** A check may take at most this share of the time: a slow one stretches the interval. */
const CHECK_DUTY_FACTOR = 10;

export interface ExternalChangesOptions {
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
 * It checks every few seconds, and at once when the window gets the focus back, when the app
 * becomes visible again and when the tab is shown again. A slow check (a very large work tree)
 * stretches the interval, so checking never takes more than about a tenth of the time. At most
 * one check runs at a time; the checks of a hidden tab or a hidden app stop.
 */
export function useExternalChanges({ active, busy, checkForChanges }: ExternalChangesOptions): void {
  const visible = useHostVisible();
  const latest = useRef({ active, busy, visible, checkForChanges });
  useEffect(() => {
    latest.current = { active, busy, visible, checkForChanges };
  });
  const running = useRef(false);
  const lastDuration = useRef(0);
  // Counts finished checks, so the timer below starts over after each one.
  const [checks, setChecks] = useState(0);

  const check = useCallback(async () => {
    const wanted = () => latest.current.active && latest.current.visible && !latest.current.busy;
    if (running.current || !wanted()) return;
    running.current = true;
    const started = Date.now();
    try {
      await latest.current.checkForChanges(wanted);
    } finally {
      lastDuration.current = Date.now() - started;
      running.current = false;
      setChecks((n) => n + 1);
    }
  }, []);

  // Shown again (the tab, or the app), or idle again after an action of this tab.
  useEffect(() => {
    if (active && visible && !busy) void check();
  }, [active, visible, busy, check]);

  useEffect(() => {
    if (!active) return;
    const onFocus = () => void check();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [active, check]);

  useEffect(() => {
    if (!active || !visible || busy) return;
    const delay = Math.min(
      CHANGE_CHECK_MAX_INTERVAL_MS,
      Math.max(CHANGE_CHECK_INTERVAL_MS, lastDuration.current * CHECK_DUTY_FACTOR),
    );
    const timer = setTimeout(() => void check(), delay);
    return () => clearTimeout(timer);
  }, [active, visible, busy, checks, check]);
}
