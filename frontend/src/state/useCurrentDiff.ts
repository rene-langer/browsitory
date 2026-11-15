import { createContext, useCallback, useContext, useEffect, useId, useMemo, useRef, useState } from "react";

/** What the palette can do to one diff view (#469): its own toolbar's per-diff toggles. */
export interface DiffViewControls {
  /** Flips this diff's "Wrap lines". */
  toggleWrap(): void;
  /** Flips "Show whole file"; `null` where the diff has no such toggle. */
  toggleWholeFile: (() => void) | null;
  /** Whether its lines wrap and it shows the whole file now, for the palette's names (#990). */
  wrapped: boolean;
  wholeFile: boolean;
}

/** The current diff, as the palette lists it. */
export interface CurrentDiffSummary {
  path: string | null;
  canShowWholeFile: boolean;
}

/**
 * One repo tab's diff views, so the palette's "Wrap lines" and "Show whole file" act on the
 * current one: the diff last clicked or focused, or the working tree's current file; otherwise the
 * first diff shown.
 */
export interface CurrentDiffRegistry {
  current: CurrentDiffSummary | null;
  /** Adds or updates a diff view; returns the removal. */
  register(id: string, entry: RegisteredDiff): () => void;
  /** Makes `id` the current diff. */
  claim(id: string): void;
  toggleWrap(): void;
  toggleWholeFile(): void;
  /** The current diff's state now (read at call time, like the toggles). */
  isWrapped(): boolean;
  isWholeFile(): boolean;
}

export interface RegisteredDiff {
  path: string | null;
  /** The latest controls (read at run time, so a toggle never acts on stale state). */
  controls: { current: DiffViewControls };
}

export function useCurrentDiffRegistry(): CurrentDiffRegistry {
  // Mount order, so the first diff shown is the fallback.
  const entries = useRef(new Map<string, RegisteredDiff>());
  const claimed = useRef<string | null>(null);
  const [current, setCurrent] = useState<CurrentDiffSummary | null>(null);

  const currentEntry = useCallback((): RegisteredDiff | null => {
    const map = entries.current;
    if (claimed.current !== null) {
      const entry = map.get(claimed.current);
      if (entry !== undefined) return entry;
    }
    return map.values().next().value ?? null;
  }, []);

  const recompute = useCallback(() => {
    const entry = currentEntry();
    const next =
      entry === null
        ? null
        : { path: entry.path, canShowWholeFile: entry.controls.current.toggleWholeFile !== null };
    setCurrent((prev) =>
      prev === next ||
      (prev !== null && next !== null && prev.path === next.path && prev.canShowWholeFile === next.canShowWholeFile)
        ? prev
        : next,
    );
  }, [currentEntry]);

  const register = useCallback(
    (id: string, entry: RegisteredDiff) => {
      entries.current.set(id, entry);
      recompute();
      return () => {
        // A claim outlives a re-registration (same id); a stale one falls back to the first diff.
        entries.current.delete(id);
        recompute();
      };
    },
    [recompute],
  );

  const claim = useCallback(
    (id: string) => {
      if (claimed.current === id) return;
      claimed.current = id;
      recompute();
    },
    [recompute],
  );

  const toggleWrap = useCallback(() => currentEntry()?.controls.current.toggleWrap(), [currentEntry]);
  const toggleWholeFile = useCallback(() => currentEntry()?.controls.current.toggleWholeFile?.(), [currentEntry]);
  const isWrapped = useCallback(() => currentEntry()?.controls.current.wrapped === true, [currentEntry]);
  const isWholeFile = useCallback(() => currentEntry()?.controls.current.wholeFile === true, [currentEntry]);

  return useMemo(
    () => ({ current, register, claim, toggleWrap, toggleWholeFile, isWrapped, isWholeFile }),
    [current, register, claim, toggleWrap, toggleWholeFile, isWrapped, isWholeFile],
  );
}

export const CurrentDiffContext = createContext<CurrentDiffRegistry | null>(null);

/**
 * Registers one diff view with its tab's registry (a no-op outside one) and returns `claim`, which
 * makes it the current diff. `current` claims it whenever it turns true.
 */
export function useRegisterDiffView(path: string | null, controls: DiffViewControls, current: boolean): () => void {
  const registry = useContext(CurrentDiffContext);
  const id = useId();
  const latest = useRef(controls);
  useEffect(() => {
    latest.current = controls;
  });
  const hasWholeFile = controls.toggleWholeFile !== null;
  const register = registry?.register;
  useEffect(() => {
    if (register === undefined) return;
    return register(id, { path, controls: latest });
    // `hasWholeFile` re-registers so the palette offers or drops "Show whole file".
  }, [register, id, path, hasWholeFile]);
  const claimFn = registry?.claim;
  useEffect(() => {
    if (current && claimFn !== undefined) claimFn(id);
  }, [current, claimFn, id]);
  return useCallback(() => claimFn?.(id), [claimFn, id]);
}
