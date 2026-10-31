import { useMemo, useRef } from "react";

/**
 * The error message one read last set, so its next success clears that error and only that one:
 * a read never clears another action's error (#177). `take` returns it and forgets it.
 */
export interface OwnError {
  set(message: string): void;
  take(): string | null;
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
    }),
    [],
  );
}

/** `error` after a successful read: cleared only when it is the message that read set. */
export function errorAfterOwnSuccess(error: string | null, own: string | null): string | null {
  return error !== null && error === own ? null : error;
}
