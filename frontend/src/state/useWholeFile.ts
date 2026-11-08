import { useCallback, useState } from "react";
import { WHOLE_FILE_CONTEXT_LINES, WHOLE_FILE_LINE_LIMIT, type DiffHunk } from "../ipc/RepoClient";

export interface WholeFileState {
  /** The toggle as the user set it. */
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

/** One file section's "Show whole file" toggle (#444). */
export function useWholeFile(contextLines: number): WholeFileState {
  const [on, setOn] = useState(false);
  const [tooLongLines, setTooLongLines] = useState<number | null>(null);
  const onToggle = useCallback(() => {
    setOn((prev) => !prev);
    setTooLongLines(null);
  }, []);
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
