import type { DateFormat } from "../ipc/RepoClient";
import { useAppearanceSettingsContext } from "./useAppearanceSettings";

/** Preferences → Appearance → Date format (#675), for `formatAbsoluteDate`/`formatShortDate`. */
export function useDateFormat(): DateFormat {
  return useAppearanceSettingsContext().settings.dateFormat;
}
