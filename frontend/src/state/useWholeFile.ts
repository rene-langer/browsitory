import { useCallback, useState } from "react";
import { WHOLE_FILE_CONTEXT_LINES, WHOLE_FILE_LINE_LIMIT, type DiffHunk } from "../ipc/RepoClient";
import { useDiffViewOptions } from "./useDiffViewOptions";

export interface WholeFileState {
  /** The toggle as the user set it: the pane's, shared by every file (#640). */
  on: boolean;
  onToggle: () => void;
  /** Lines of the whole-file diff that came back too long to show; `null` otherwise. */
  tooLongLines: number | null;
  /** The context to fetch with: the whole file while on and not too long, else `contextLines`. */
  contextLines: number;
  /**
   * Checks hunks fetched with `requestedContext`. Returns `false` (and falls back to the normal
   * context, which refetches) when a whole-file diff is over `WHOLE_FILE_LINE_LIMIT` lines; the
   * caller must then not show them.
   */
  accept: (hunks: DiffHunk[], requestedContext: number) => boolean;
}

/**
 * One file's "Show whole file" (#444). The toggle is the pane's (`useDiffViewOptions`); the
 * fallback to the normal context when this file is too long is the file's own.
 */
export function useWholeFile(contextLines: number): WholeFileState {
  const { wholeFile: on, toggleWholeFile } = useDiffViewOptions();
  const [tooLongLines, setTooLongLines] = useState<number | null>(null);
  // Turning it off and on again retries a file that was too long (it may have shrunk).
  const [previousOn, setPreviousOn] = useState(on);
  if (previousOn !== on) {
    setPreviousOn(on);
    setTooLongLines(null);
  }
  const onToggle = useCallback(() => {
    toggleWholeFile();
    setTooLongLines(null);
  }, [toggleWholeFile]);
  const accept = useCallback((hunks: DiffHunk[], requestedContext: number) => {
    if (requestedContext !== WHOLE_FILE_CONTEXT_LINES) return true;
    const lines = hunks.reduce((n, hunk) => n + hunk.lines.length, 0);
    if (lines <= WHOLE_FILE_LINE_LIMIT) return true;
    setTooLongLines(lines);
    return false;
  }, []);
  return {
    on,
    onToggle,
    tooLongLines,
    contextLines: on && tooLongLines === null ? WHOLE_FILE_CONTEXT_LINES : contextLines,
    accept,
  };
}
