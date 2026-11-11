import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { BlockChoice } from "../lib/conflictMerge";

/** What the palette can do in the merge editor (#764). */
export interface MergeEditorControls {
  previous(): void;
  next(): void;
  /** Takes `choice` for the current conflict (the first, before one is current) and focuses it. */
  choose(choice: BlockChoice): void;
}

/** The open merge editor, as the palette lists it. */
export interface MergeEditorSummary {
  path: string;
  canGoPrevious: boolean;
  canGoNext: boolean;
}

/** One repo tab's merge editor (there is at most one), so the palette's commands act on it. */
export interface MergeEditorRegistry {
  current: (MergeEditorSummary & MergeEditorControls) | null;
  /** Shows `summary` (null: nothing to offer); returns the removal. */
  register(summary: MergeEditorSummary | null, controls: { current: MergeEditorControls }): () => void;
}

export function useMergeEditorRegistry(): MergeEditorRegistry {
  const latest = useRef<{ current: MergeEditorControls } | null>(null);
  const [summary, setSummary] = useState<MergeEditorSummary | null>(null);

  const register = useCallback((next: MergeEditorSummary | null, controls: { current: MergeEditorControls }) => {
    latest.current = controls;
    setSummary(next);
    return () => {
      if (latest.current !== controls) return;
      latest.current = null;
      setSummary(null);
    };
  }, []);

  const current = useMemo(
    () =>
      summary === null
        ? null
        : {
            ...summary,
            // Read at run time, so a command never acts on stale state.
            previous: () => latest.current?.current.previous(),
            next: () => latest.current?.current.next(),
            choose: (choice: BlockChoice) => latest.current?.current.choose(choice),
          },
    [summary],
  );

  return useMemo(() => ({ current, register }), [current, register]);
}

export const MergeEditorContext = createContext<MergeEditorRegistry | null>(null);

/** Registers the merge editor with its tab's registry (a no-op outside one). */
export function useRegisterMergeEditor(summary: MergeEditorSummary | null, controls: MergeEditorControls): void {
  const registry = useContext(MergeEditorContext);
  const latest = useRef(controls);
  useEffect(() => {
    latest.current = controls;
  });
  const register = registry?.register;
  const path = summary?.path;
  const canGoPrevious = summary?.canGoPrevious;
  const canGoNext = summary?.canGoNext;
  useEffect(() => {
    if (register === undefined) return;
    const next =
      path === undefined || canGoPrevious === undefined || canGoNext === undefined
        ? null
        : { path, canGoPrevious, canGoNext };
    return register(next, latest);
  }, [register, path, canGoPrevious, canGoNext]);
}
