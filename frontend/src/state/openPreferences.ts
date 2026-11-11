import { createContext } from "react";
import type { PreferencesGroupId } from "../lib/preferencesGroup";

/**
 * Opens the Preferences dialog, on `group` when given, else on the group shown last (#784). The
 * app provides it; without a provider (a component rendered alone) there is nothing to open, and
 * links to a setting are left out.
 */
export const OpenPreferencesContext = createContext<((group?: PreferencesGroupId) => void) | null>(null);
