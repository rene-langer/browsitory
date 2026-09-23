/**
 * Maps a raw thrown error to a plain-language `hint` while preserving the original message
 * verbatim, per `docs/CONTENT_GUIDELINES.md`: "keep raw backend text as the message and add a
 * plain-language hint." Every `InlineError` call site that surfaces a caught fetch/mutation error
 * (DiffPane's diff/blame/commit-files fetches, App's transport banner) should run its caught
 * value through this before rendering, instead of showing `String(err)` unexplained.
 */
export interface DescribedError {
  message: string;
  hint?: string;
}

const HINTS: Array<{ match: (message: string) => boolean; hint: string }> = [
  {
    match: (message) => message.includes("Transport failed") || message.includes("sidecar exited"),
    hint: "The connection to the backend was lost. Retry, or reopen the repository.",
  },
  {
    match: (message) => message.includes("failed to read working diff"),
    hint: "Could not read this file's changes. Retry, or check the file still exists.",
  },
  {
    match: (message) => message.includes("permission denied"),
    hint: "The operation was blocked by file permissions. Check the file isn't locked elsewhere.",
  },
];

export function describeError(err: unknown): DescribedError {
  const message = err instanceof Error ? err.message : String(err);
  const hint = HINTS.find((candidate) => candidate.match(message))?.hint;
  return hint === undefined ? { message } : { message, hint };
}
