import { useEffect, type ReactNode } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import { applyAppearance } from "../lib/appearance";
import { AppearanceSettingsContext, useAppearanceSettings } from "./useAppearanceSettings";

/** Provides the appearance settings and applies them to the document whenever they change. */
export function AppearanceSettingsProvider({ client, children }: { client: RepoClient; children: ReactNode }) {
  const api = useAppearanceSettings(client);
  useEffect(() => {
    applyAppearance(api.settings);
  }, [api.settings]);
  return <AppearanceSettingsContext.Provider value={api}>{children}</AppearanceSettingsContext.Provider>;
}
