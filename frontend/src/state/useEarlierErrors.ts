import { useCallback, useState } from "react";

/** How many replaced errors a banner keeps. */
export const EARLIER_ERRORS_KEPT = 5;

/**
 * The errors a banner's current one replaced (#663), newest first. An action clears the banner's
 * error when it starts (#177), so a second failure used to wipe the first without a trace: here
 * every error that goes away without being dismissed is kept (up to `EARLIER_ERRORS_KEPT`) and
 * shown under the next one. `dismiss` clears them with the current error.
 */
export function useEarlierErrors(error: string | null, onDismiss: () => void) {
  // `dismissed`: the error just dismissed, never kept even if it is still shown for a render.
  const [tracked, setTracked] = useState<{ current: string | null; earlier: readonly string[]; dismissed: string | null }>({
    current: error,
    earlier: [],
    dismissed: null,
  });
  // Adjusted during render, as React's "storing information from previous renders" pattern.
  let { earlier } = tracked;
  if (tracked.current !== error) {
    const gone = tracked.current;
    if (gone !== null && gone !== tracked.dismissed) {
      earlier = [gone, ...earlier.filter((text) => text !== gone)].slice(0, EARLIER_ERRORS_KEPT);
    }
    setTracked({ current: error, earlier, dismissed: tracked.dismissed === error ? tracked.dismissed : null });
  }
  const dismiss = useCallback(() => {
    setTracked((prev) => ({ current: prev.current, earlier: [], dismissed: prev.current }));
    onDismiss();
  }, [onDismiss]);
  return { earlier: earlier.filter((text) => text !== error), dismiss };
}
