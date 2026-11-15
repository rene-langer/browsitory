import { useLayoutEffect, useRef, type KeyboardEvent, type RefObject } from "react";

/**
 * The one Escape rule for transient UI that isn't a dialog (#1001): the compare pane, the commit
 * search, the sidebar's inline forms. Escape anywhere inside the container closes the innermost
 * such UI, then focus goes back to what opened it: the element focused last outside the
 * container (the control that opened it, or one the user moved to since).
 *
 * Spread `onKeyDown` on the container and put `containerRef` on it. An Escape a control inside
 * already handled (`preventDefault`, e.g. a menu, a suggestion list or a nested form) is left
 * alone, so the innermost UI closes first. `initialFocus` takes the focus when it opens, after
 * the opener is recorded (in place of `autoFocus`, which would move it first).
 */
export function useEscapeToClose<T extends HTMLElement>({
  open = true,
  onClose,
  initialFocus,
}: {
  /** Whether the UI is shown; a component that stays mounted while closed passes it. */
  open?: boolean;
  onClose: () => void;
  initialFocus?: RefObject<HTMLElement | null>;
}) {
  const containerRef = useRef<T>(null);
  const opener = useRef<HTMLElement | null>(null);

  useLayoutEffect(() => {
    if (!open) return;
    const outside = (element: EventTarget | null): element is HTMLElement =>
      element instanceof HTMLElement && element !== document.body && !(containerRef.current?.contains(element) ?? false);
    opener.current = outside(document.activeElement) ? document.activeElement : null;
    initialFocus?.current?.focus();
    const onFocusIn = (event: FocusEvent) => {
      if (outside(event.target)) opener.current = event.target;
    };
    document.addEventListener("focusin", onFocusIn);
    return () => document.removeEventListener("focusin", onFocusIn);
  }, [open, initialFocus]);

  /** Closes, then focuses what opened the UI (if it is still on the page). */
  const close = () => {
    const back = opener.current;
    onClose();
    if (back?.isConnected) back.focus({ preventScroll: true });
  };

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Escape" || event.defaultPrevented) return;
    if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
    event.preventDefault();
    event.stopPropagation();
    close();
  };

  return { containerRef, onKeyDown, close };
}
