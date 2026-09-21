import { useEffect } from "react";

/**
 * Remembers the element focused when the caller mounts and refocuses it on unmount, so closing a
 * modal `<dialog>` (which callers implement by unmounting) returns focus to the invoking control
 * instead of dropping it on `<body>`. Must run before the dialog moves focus into itself.
 */
export function useRestoreFocus(): void {
  useEffect(() => {
    const previous = document.activeElement;
    return () => {
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, []);
}
