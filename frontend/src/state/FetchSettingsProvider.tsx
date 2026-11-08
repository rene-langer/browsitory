import type { ReactNode } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import { FetchSettingsContext, useFetchSettings } from "./useFetchSettings";

export function FetchSettingsProvider({ client, children }: { client: RepoClient; children: ReactNode }) {
  const api = useFetchSettings(client);
  return <FetchSettingsContext.Provider value={api}>{children}</FetchSettingsContext.Provider>;
}
