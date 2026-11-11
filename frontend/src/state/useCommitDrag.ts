import { createContext } from "react";

// Placeholder so the failing tests of #744's commit drag load; the implementation follows.

export const CommitDragContext = createContext<null>(null);

export function useCommitDragSession(_onRun: (run: unknown) => void): null {
  return null;
}
