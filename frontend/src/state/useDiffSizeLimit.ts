import { useCallback, useMemo, useState } from "react";
import { errorKind } from "../ipc/RepoClient";

/**
 * One file's diff size limit (#1066). The backend refuses a diff with a side over its limit
 * (kind `DiffTooLarge`, its message says how large); the viewer then shows that message, and
 * "Load diff anyway" fetches the file again with `noSizeLimit`. Both belong to the file
 * (`fileKey`): another file, or another diff source, asks with the limit again.
 */
export interface DiffSizeLimit {
  /** Spread into the fetch's `DiffOptions`: `{ noSizeLimit: true }` once loaded anyway, else `{}`. */
  options: { noSizeLimit?: true };
  /** Why the diff isn't loaded (the backend's message), or `null`. */
  tooLarge: string | null;
  /** Takes a failed fetch: `true` when it was `DiffTooLarge`, now shown as `tooLarge`. */
  refused: (err: unknown) => boolean;
  /** A fetch came back with hunks. */
  loaded: () => void;
  loadAnyway: () => void;
}

interface State {
  key: string;
  anyway: boolean;
  tooLarge: string | null;
}

export function useDiffSizeLimit(fileKey: string): DiffSizeLimit {
  const [state, setState] = useState<State>({ key: fileKey, anyway: false, tooLarge: null });
  // Another file resets it, during render (as `DiffView` resets its own per-file state).
  const current = state.key === fileKey ? state : { key: fileKey, anyway: false, tooLarge: null };
  if (current !== state) setState(current);

  const refused = useCallback((err: unknown) => {
    if (errorKind(err) !== "DiffTooLarge") return false;
    const message = err instanceof Error ? err.message : String(err);
    setState((prev) => ({ ...prev, tooLarge: message }));
    return true;
  }, []);
  const loaded = useCallback(() => setState((prev) => (prev.tooLarge === null ? prev : { ...prev, tooLarge: null })), []);
  const loadAnyway = useCallback(() => setState((prev) => ({ ...prev, anyway: true, tooLarge: null })), []);
  const options = useMemo(() => (current.anyway ? { noSizeLimit: true as const } : {}), [current.anyway]);

  return { options, tooLarge: current.tooLarge, refused, loaded, loadAnyway };
}
