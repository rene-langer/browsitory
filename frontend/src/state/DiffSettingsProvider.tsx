import type { ReactNode } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import { DiffSettingsContext, useDiffSettings } from "./useDiffSettings";

export function DiffSettingsProvider({ client, children }: { client: RepoClient; children: ReactNode }) {
  const api = useDiffSettings(client);
  return <DiffSettingsContext.Provider value={api}>{children}</DiffSettingsContext.Provider>;
}
