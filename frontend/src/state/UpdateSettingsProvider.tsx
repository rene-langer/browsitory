import type { ReactNode } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import { UpdateSettingsContext, useUpdateSettings } from "./useUpdateSettings";

/** `enabled` is false where there is no updater (VSCode): the settings are then never read. */
export function UpdateSettingsProvider({
  client,
  enabled = true,
  children,
}: {
  client: RepoClient;
  enabled?: boolean;
  children: ReactNode;
}) {
  const api = useUpdateSettings(client, enabled);
  return <UpdateSettingsContext.Provider value={api}>{children}</UpdateSettingsContext.Provider>;
}
