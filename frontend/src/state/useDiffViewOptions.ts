import { createContext, useCallback, useContext, useMemo, useState } from "react";

/**
 * The diff options that apply to every diff on screen but aren't diff settings (#640): Split view
 * and "Show whole file". The pane's one `DiffOptionsToolbar` sets them; each `DiffView` and file
 * section reads them. They persist in `localStorage`, per viewer, like the pane widths; the rest
 * of the toolbar persists in the diff settings.
 */
export interface DiffViewOptions {
  split: boolean;
  setSplit(split: boolean): void;
  wholeFile: boolean;
  toggleWholeFile(): void;
}

const SPLIT_KEY = "diff.split";
const WHOLE_FILE_KEY = "diff.wholeFile";

// Storage can be missing or throw (a blocked or private context): the options then last as long
// as the provider does.
function load(key: string): boolean {
  try {
    return localStorage.getItem(key) === "true";
  } catch {
    return false;
  }
}

function save(key: string, value: boolean): void {
  try {
    localStorage.setItem(key, String(value));
  } catch {
    // Not persisted; the in-memory value still applies.
  }
}

export const DiffViewOptionsContext = createContext<DiffViewOptions | null>(null);

/** A fresh set of options, loaded from storage; `DiffViewOptionsProvider` shares one. */
export function useOwnOptions(): DiffViewOptions {
  const [split, setSplitState] = useState(() => load(SPLIT_KEY));
  const [wholeFile, setWholeFile] = useState(() => load(WHOLE_FILE_KEY));
  const setSplit = useCallback((next: boolean) => {
    setSplitState(next);
    save(SPLIT_KEY, next);
  }, []);
  const toggleWholeFile = useCallback(() => {
    setWholeFile((prev) => {
      save(WHOLE_FILE_KEY, !prev);
      return !prev;
    });
  }, []);
  return useMemo(() => ({ split, setSplit, wholeFile, toggleWholeFile }), [split, setSplit, wholeFile, toggleWholeFile]);
}

/** The shared options; outside any provider, this component's own (not shared, not reset). */
export function useDiffViewOptions(): DiffViewOptions {
  const shared = useContext(DiffViewOptionsContext);
  const own = useOwnOptions();
  return shared ?? own;
}
