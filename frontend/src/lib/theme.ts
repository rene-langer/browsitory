/** The colour theme on screen. */
export type Theme = "light" | "dark";

/** Preferences > Appearance > Theme (#691): `system` follows `prefers-color-scheme`, live. */
export type ThemePreference = "system" | Theme;

export const THEME_PREFERENCES: readonly ThemePreference[] = ["system", "light", "dark"];

const STORAGE_KEY = "browsitory-theme";

export const DARK_SCHEME_QUERY = "(prefers-color-scheme: dark)";

/** The stored preference; anything else (nothing stored, an unknown value) is `system`. */
export function parseThemePreference(stored: string | null): ThemePreference {
  return stored === "light" || stored === "dark" ? stored : "system";
}

export function resolveTheme(preference: ThemePreference, prefersDark: boolean): Theme {
  if (preference === "system") return prefersDark ? "dark" : "light";
  return preference;
}

export function loadThemePreference(): ThemePreference {
  try {
    return parseThemePreference(localStorage.getItem(STORAGE_KEY));
  } catch {
    return "system";
  }
}

/**
 * Applies `preference` to the document. `system` removes `data-theme`, so `styles/tokens.css`
 * follows `prefers-color-scheme` by itself, also when the system changes later; `light` and
 * `dark` pin the palette with `data-theme`.
 */
export function applyThemePreference(preference: ThemePreference, root: HTMLElement = document.documentElement): void {
  if (preference === "system") delete root.dataset.theme;
  else root.dataset.theme = preference;
}

/** Stores `preference` (`system` clears the stored choice) and applies it. */
export function persistThemePreference(preference: ThemePreference): void {
  try {
    if (preference === "system") localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, preference);
  } catch {
    // Storage blocked: the choice still applies for this session.
  }
  applyThemePreference(preference);
}
