import { useEffect, useRef } from "react";

/**
 * Remembers the element focused when the caller mounts and refocuses it on unmount, so closing a
 * modal `<dialog>` (which callers implement by unmounting) returns focus to the invoking control
 * instead of dropping it on `<body>`. Must run before the dialog moves focus into itself.
 * `returnFocusTo`, when given, is focused instead: for a dialog opened from a control that is
 * gone by then (the command palette).
 */
export function useRestoreFocus(returnFocusTo?: HTMLElement | null): void {
  const target = useRef(returnFocusTo);
  useEffect(() => {
    const previous = target.current ?? document.activeElement;
    return () => {
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, []);
}
