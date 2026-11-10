import { useContext, type ReactNode } from "react";
import { DiffViewOptionsContext, useOwnOptions } from "./useDiffViewOptions";

/**
 * Provides the shared options. Nested inside another provider it adds nothing, so a pane can
 * always wrap its content in one: under the app's provider it shares the app's options, alone (a
 * test, a pane rendered by itself) its toolbar and diffs still share one set.
 */
export function DiffViewOptionsProvider({ children }: { children: ReactNode }) {
  const parent = useContext(DiffViewOptionsContext);
  const own = useOwnOptions();
  return <DiffViewOptionsContext.Provider value={parent ?? own}>{children}</DiffViewOptionsContext.Provider>;
}
