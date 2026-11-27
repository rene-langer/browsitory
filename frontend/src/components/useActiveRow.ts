import { useCallback, useId, useState } from "react";

/**
 * The selected row of a listbox that keeps the DOM focus on itself and renders only the rows in
 * view (`useVirtualRows`), for screen readers (#1139): the row is the list's
 * `aria-activedescendant` while it is rendered. A selection moved to a row the window doesn't
 * hold yet (PageDown, End, a Go to) has no element to point at, so `announcement` names it for a
 * polite live region, once per move.
 */
export function useActiveRow({
  index,
  start,
  end,
  label,
}: {
  /** The selected row's index, `-1` for none. */
  index: number;
  /** The rendered rows, `[start, end)`. */
  start: number;
  end: number;
  /** What a live region says for a row: its commit and position. */
  label: (index: number) => string;
}): { rowId: (index: number) => string; activeDescendant: string | undefined; announcement: string } {
  const base = useId();
  const rowId = useCallback((row: number) => `${base}-row-${row}`, [base]);
  const rendered = index >= start && index < end;
  const [announced, setAnnounced] = useState({ index, text: "" });
  let announcement = announced.text;
  if (announced.index !== index) {
    announcement = index >= 0 && !rendered ? label(index) : "";
    setAnnounced({ index, text: announcement });
  }
  return { rowId, activeDescendant: index >= 0 && rendered ? rowId(index) : undefined, announcement };
}
