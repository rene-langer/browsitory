/**
 * The merge editor's undo history (#878): every change to the blocks' states (a take, a choice,
 * Reset, typing) is one step, except that keystrokes in one field, without a pause, make one.
 */
export interface History<T> {
  past: T[];
  present: T;
  future: T[];
  /** What the last step typed into (a field's key), so the next keystroke there can join it. */
  group: string | null;
  /** When the last step was recorded, in ms. */
  at: number;
}

/** A pause longer than this between keystrokes starts a new step. */
export const TYPING_PAUSE_MS = 1000;

/** The most steps kept; older ones are dropped. */
export const HISTORY_LIMIT = 500;

export function historyOf<T>(present: T): History<T> {
  return { past: [], present, future: [], group: null, at: 0 };
}

/** `next` as the new present; `group` names the field typed into, `null` for any other change. */
export function record<T>(history: History<T>, next: T, group: string | null, now: number): History<T> {
  if (next === history.present) return history;
  if (group !== null && group === history.group && now - history.at <= TYPING_PAUSE_MS) {
    return { ...history, present: next, future: [], at: now };
  }
  return {
    past: [...history.past, history.present].slice(-HISTORY_LIMIT),
    present: next,
    future: [],
    group,
    at: now,
  };
}

export function undo<T>(history: History<T>): History<T> {
  if (history.past.length === 0) return history;
  return {
    past: history.past.slice(0, -1),
    present: history.past[history.past.length - 1],
    future: [history.present, ...history.future],
    group: null,
    at: 0,
  };
}

export function redo<T>(history: History<T>): History<T> {
  if (history.future.length === 0) return history;
  const [next, ...rest] = history.future;
  return { past: [...history.past, history.present], present: next, future: rest, group: null, at: 0 };
}
