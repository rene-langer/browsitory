import { createContext, useCallback, useContext, useMemo, useState } from "react";
import { copyText } from "../lib/clipboard";
import { describeError } from "../lib/errorMessages";

/**
 * Copies `text` and reports the outcome (#614). `what` names the copied thing in the feedback
 * ("branch name" → "Copied branch name"; "Couldn't copy the branch name: …").
 */
export type CopyToClipboard = (text: string, what: string) => void;

/** Provided by `RepoWorkspace`, which owns the toast and the error banner. */
export const CopyToClipboardContext = createContext<CopyToClipboard | null>(null);

// Outside a workspace (a component rendered on its own) there is nowhere to report to: the text
// is still copied, with no feedback.
const copyWithoutFeedback: CopyToClipboard = (text) => {
  copyText(text).catch(() => {});
};

/** The workspace's copy action, for the context menus. */
export function useCopyToClipboard(): CopyToClipboard {
  return useContext(CopyToClipboardContext) ?? copyWithoutFeedback;
}

/**
 * The provider's side: a success shows a toast, a failure an error (shown by `RepoWorkspace` in
 * its error layer until dismissed or the next copy).
 */
export function useClipboardFeedback(pushToast: (message: string) => void) {
  const [error, setError] = useState<string | null>(null);
  const copy = useCallback<CopyToClipboard>(
    (text, what) => {
      setError(null);
      copyText(text).then(
        () => pushToast(`Copied ${what}`),
        (err: unknown) => setError(`Couldn't copy the ${what}: ${describeError(err).message}`),
      );
    },
    [pushToast],
  );
  const dismissError = useCallback(() => setError(null), []);
  return useMemo(() => ({ copy, error, dismissError }), [copy, error, dismissError]);
}
