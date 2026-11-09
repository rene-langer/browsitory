import { errorKind, type RepoErrorKind } from "../ipc/RepoClient";

/**
 * Maps a raw thrown error to a plain-language `hint` while preserving the original message
 * verbatim, per `docs/CONTENT_GUIDELINES.md`: "keep raw backend text as the message and add a
 * plain-language hint." Every `InlineError` call site that surfaces a caught fetch/mutation error
 * (DiffPane's diff/blame/commit-files fetches, App's transport banner) should run its caught
 * value through this before rendering, instead of showing `String(err)` unexplained. The hint
 * follows the error's kind (#496), never its text.
 */
export interface DescribedError {
  message: string;
  hint?: string;
}

const HINTS: Partial<Record<RepoErrorKind, string>> = {
  TransportLost: "The connection to the backend was lost. Retry, or reopen the repository.",
};

export function describeError(err: unknown): DescribedError {
  const message = err instanceof Error ? err.message : String(err);
  const hint = HINTS[errorKind(err)];
  return hint === undefined ? { message } : { message, hint };
}
