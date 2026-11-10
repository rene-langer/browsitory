/**
 * Requests from the app to the active repository workspace (#647): the header's Commands button
 * and Help menu open the palette and the shortcut sheet, whose open state each `RepoWorkspace`
 * owns (`useWorkspaceShortcuts`), and patch files dropped on the window are applied to its
 * working tree (#649). Only the active workspace listens.
 */
export type WorkspaceRequest = "palette" | "shortcuts" | { kind: "applyPatches"; paths: string[] };

type Listener = (request: WorkspaceRequest) => void;

const listeners = new Set<Listener>();

/** Asks the active workspace to open `request`; does nothing while none is listening. */
export function requestWorkspace(request: WorkspaceRequest): void {
  for (const listener of [...listeners]) listener(request);
}

/** Calls `listener` for each request until the returned function is called. */
export function onWorkspaceRequest(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
