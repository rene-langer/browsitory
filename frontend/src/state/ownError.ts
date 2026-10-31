import { useMemo, useRef } from "react";

/**
 * The error message one read last set, so its next success clears that error and only that one:
 * a read never clears another action's error (#177). `take` returns it and forgets it.
 * `forgetUnless` forgets it once the error on screen is something else (dismissed, cleared by a
 * mutation starting, or replaced), so a later error with the same text isn't mistaken for it.
 */
export interface OwnError {
  set(message: string): void;
  take(): string | null;
  forgetUnless(current: string | null): void;
}

export function useOwnError(): OwnError {
  const ref = useRef<string | null>(null);
  return useMemo(
    () => ({
      set: (message: string) => {
        ref.current = message;
      },
      take: () => {
        const message = ref.current;
        ref.current = null;
        return message;
      },
      forgetUnless: (current: string | null) => {
        if (ref.current !== current) ref.current = null;
      },
    }),
    [],
  );
}

/** `error` after a successful read: cleared only when it is the message that read set. */
export function errorAfterOwnSuccess(error: string | null, own: string | null): string | null {
  return error !== null && error === own ? null : error;
}

/**
 * Wraps a state setter so each read's remembered error is forgotten as soon as `error` becomes
 * something else (dismissed, cleared by a mutation or transfer starting, or replaced). Done in
 * the update itself, not an effect: a mutation's start and its failure can land in one render.
 * Best effort: an updater React runs late may forget a read's error that is still showing; that
 * error then just stays until dismissed. It never makes a read clear another action's error.
 */
export function forgettingStaleOwnErrors<S extends { error: string | null }>(
  setState: (updater: (prev: S) => S) => void,
  owners: OwnError[],
): (updater: (prev: S) => S) => void {
  return (updater) =>
    setState((prev) => {
      const next = updater(prev);
      if (next.error !== prev.error) for (const owner of owners) owner.forgetUnless(next.error);
      return next;
    });
}
