import { useCallback, useEffect, useState } from "react";
import {
  applyThemePreference,
  DARK_SCHEME_QUERY,
  loadThemePreference,
  persistThemePreference,
  resolveTheme,
  type Theme,
  type ThemePreference,
} from "../lib/theme";

function darkSchemeQuery(): MediaQueryList | null {
  return typeof window.matchMedia === "function" ? window.matchMedia(DARK_SCHEME_QUERY) : null;
}

/** Whether the system prefers a dark scheme, kept current as it changes. */
function usePrefersDark(): boolean {
  const [prefersDark, setPrefersDark] = useState(() => darkSchemeQuery()?.matches ?? false);
  useEffect(() => {
    const query = darkSchemeQuery();
    if (query === null || typeof query.addEventListener !== "function") return;
    const onChange = (event: MediaQueryListEvent) => setPrefersDark(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);
  return prefersDark;
}

/**
 * The app's colour theme (#691): the stored preference (System, Light or Dark; System when
 * nothing is stored), applied to the document, and the theme it resolves to now. `enabled: false`
 * (the VSCode webview, where the editor's theme decides, #676) applies nothing.
 */
export function useTheme(enabled = true) {
  const [preference, setPreferenceState] = useState<ThemePreference>(loadThemePreference);
  const prefersDark = usePrefersDark();
  useEffect(() => {
    if (enabled) applyThemePreference(preference);
  }, [enabled, preference]);
  const setPreference = useCallback((next: ThemePreference) => {
    persistThemePreference(next);
    setPreferenceState(next);
  }, []);
  const theme: Theme = resolveTheme(preference, prefersDark);
  return { preference, theme, setPreference };
}
