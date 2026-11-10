import type { ReactNode } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import { DiffSettingsContext, useDiffSettings } from "./useDiffSettings";
import { DiffViewOptionsProvider } from "./DiffViewOptionsProvider";

/** The diff settings, plus the pane-wide view options (Split, whole file, #640) every tab shares. */
export function DiffSettingsProvider({ client, children }: { client: RepoClient; children: ReactNode }) {
  const api = useDiffSettings(client);
  return (
    <DiffSettingsContext.Provider value={api}>
      <DiffViewOptionsProvider>{children}</DiffViewOptionsProvider>
    </DiffSettingsContext.Provider>
  );
}
