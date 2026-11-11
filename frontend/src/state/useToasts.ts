import { useCallback, useEffect, useRef, useState } from "react";

/** How a toast looks and how long it stays: errors stay until dismissed. */
export type ToastTone = "success" | "info" | "warning" | "error";

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface ToastOptions {
  /** Defaults to `"success"`. */
  tone?: ToastTone;
  /** One button next to the message; clicking it runs `run` and closes the toast. */
  action?: ToastAction;
  /** Two buttons, in this order (Push and Undo after a commit, #733); like `action` otherwise. */
  actions?: ToastAction[];
}

export interface Toast {
  id: number;
  message: string;
  tone: ToastTone;
  action?: ToastAction;
  actions?: ToastAction[];
}

/** A toast's buttons, in order: `actions`, else `action`, else none. */
export function toastActions(toast: Pick<Toast, "action" | "actions">): ToastAction[] {
  if (toast.actions !== undefined) return toast.actions;
  return toast.action === undefined ? [] : [toast.action];
}

/** What every toast producer takes: `pushToast("Committed 4c5beaf", { action: undo })`. */
export type PushToast = (message: string, options?: ToastOptions) => void;

const MIN_MS = 5000;
const MAX_MS = 15000;
const MS_PER_CHAR = 60;
const WITH_ACTION_MIN_MS = 10000;
const WARNING_MIN_MS = 8000;
const MAX_TOASTS = 3;

/**
 * How long a toast stays, in ms, or `null` for "until dismissed" (errors). Long enough to read
 * the message, and at least 10 s with an action so there is time to reach it (WCAG 2.2.1). The
 * clock stops while the pointer is over a toast or focus is in one (`setPaused`).
 */
export function toastDuration(toast: Pick<Toast, "message" | "tone" | "action" | "actions">): number | null {
  if (toast.tone === "error") return null;
  let ms = Math.min(MAX_MS, Math.max(MIN_MS, 2000 + toast.message.length * MS_PER_CHAR));
  if (toastActions(toast).length > 0) ms = Math.max(ms, WITH_ACTION_MIN_MS);
  if (toast.tone === "warning") ms = Math.max(ms, WARNING_MIN_MS);
  return ms;
}

interface Clock {
  /** Time left, in ms; `Infinity` for a toast that stays until dismissed. */
  remaining: number;
  /** Set while the clock runs. */
  timer?: ReturnType<typeof setTimeout>;
  startedAt?: number;
}

function stopClock(clock: Clock) {
  if (clock.timer === undefined) return;
  clearTimeout(clock.timer);
  clock.remaining -= Date.now() - (clock.startedAt ?? Date.now());
  clock.timer = undefined;
}

/**
 * The app's toast stack (AUD-2026-09-20-FB-003, #661): at most three, newest last, each closing
 * after `toastDuration` unless paused. `ToastRegion` renders it and pauses it on hover or focus.
 */
export function useToasts() {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(1);
  const clocks = useRef(new Map<number, Clock>());
  const paused = useRef(false);

  const dismiss = useCallback((id: number) => {
    const clock = clocks.current.get(id);
    if (clock !== undefined) clearTimeout(clock.timer);
    clocks.current.delete(id);
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
  }, []);

  const startClock = useCallback(
    (id: number, clock: Clock) => {
      clock.startedAt = Date.now();
      clock.timer = setTimeout(() => dismiss(id), Math.max(0, clock.remaining));
    },
    [dismiss],
  );

  const push = useCallback<PushToast>(
    (message, options = {}) => {
      const id = nextId.current++;
      const toast: Toast = { id, message, tone: options.tone ?? "success" };
      if (options.action !== undefined) toast.action = options.action;
      if (options.actions !== undefined) toast.actions = options.actions;
      // Ids only grow and a Map keeps insertion order, so the first clock is the oldest toast's.
      for (const [oldId, clock] of clocks.current) {
        if (clocks.current.size < MAX_TOASTS) break;
        clearTimeout(clock.timer);
        clocks.current.delete(oldId);
      }
      setToasts((prev) => [...prev.slice(-(MAX_TOASTS - 1)), toast]);
      const duration = toastDuration(toast);
      const clock: Clock = { remaining: duration ?? Infinity };
      clocks.current.set(id, clock);
      if (duration !== null && !paused.current) startClock(id, clock);
    },
    [startClock],
  );

  /** Stops every toast's clock (`true`), or restarts each with the time it had left (`false`). */
  const setPaused = useCallback(
    (next: boolean) => {
      if (paused.current === next) return;
      paused.current = next;
      clocks.current.forEach((clock, id) => {
        if (next) stopClock(clock);
        else if (Number.isFinite(clock.remaining)) startClock(id, clock);
      });
    },
    [startClock],
  );

  useEffect(() => {
    const active = clocks.current;
    return () => {
      active.forEach((clock) => clearTimeout(clock.timer));
      active.clear();
    };
  }, []);

  return { toasts, push, dismiss, setPaused };
}
