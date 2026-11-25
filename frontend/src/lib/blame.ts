import { errorKind, type Blame } from "../ipc/RepoClient";
import type { DescribedError } from "./errorMessages";

/** The blame shown before one has loaded. */
export const EMPTY_BLAME: Blame = { commits: [], lines: [] };

/** What a blame view shows when `getBlame` fails. */
export interface BlameFailure extends DescribedError {
  /** Asking again can help: not for a file blame refuses (too large, binary). */
  retry: boolean;
}

/**
 * A failed `getBlame`: a file blame refuses (`BlameUnavailable`, #1158) says why, with no
 * Retry; anything else is the general message.
 */
export function blameFailure(error: unknown): BlameFailure {
  if (errorKind(error) === "BlameUnavailable") {
    return { message: error instanceof Error ? error.message : String(error), retry: false };
  }
  return { message: "No blame available for this file at this revision.", retry: true };
}

/** Whether `error` is a file blame refuses: shown as a note, not an error with Retry. */
export function isBlameRefusal(error: DescribedError | BlameFailure | null): error is BlameFailure {
  return error !== null && "retry" in error && !error.retry;
}
