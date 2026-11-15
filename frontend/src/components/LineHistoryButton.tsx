import type { RefObject } from "react";
import { lineRangeLabel } from "../lib/selectedLines";
import { useSelectedLines } from "../state/useSelectedLines";
import { ReasonButton } from "./primitives/ReasonButton";

/** Why "Line history" can't run yet (#1023). */
const NO_LINES = "Select lines first.";

/**
 * "History of lines N–M" (#447) for the lines selected inside `container` (rows carrying
 * `data-line-number`); "Line history" while none are, a `ReasonButton` that says why it can't run
 * (#1023). The numbers are `revision`'s version of `path`.
 */
export function LineHistoryButton({
  container,
  enabled = true,
  path,
  revision,
  onOpen,
}: {
  container: RefObject<HTMLElement | null>;
  /** Off while the content is hidden (a collapsed section). */
  enabled?: boolean;
  path: string;
  revision: string;
  onOpen: (path: string, revision: string, startLine: number, endLine: number) => void;
}) {
  const selected = useSelectedLines(container, enabled);
  const label = selected === null ? null : lineRangeLabel(selected.start, selected.end);
  return (
    <ReasonButton
      reason={selected === null ? NO_LINES : null}
      aria-label={label === null ? `History of selected lines of ${path}` : `History of ${label} of ${path}`}
      // Keep the text selection: a click must not clear it before it is read.
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => {
        if (selected !== null) onOpen(path, revision, selected.start, selected.end);
      }}
    >
      {label === null ? "Line history" : `History of ${label}`}
    </ReasonButton>
  );
}
