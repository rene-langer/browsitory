export type Theme = "light" | "dark";

const STORAGE_KEY = "browsitory-theme";

export function resolveTheme(stored: string | null, prefersDark: boolean): Theme {
  if (stored === "light" || stored === "dark") {
    return stored;
  }
  return prefersDark ? "dark" : "light";
}

export function loadStoredTheme(): string | null {
  return localStorage.getItem(STORAGE_KEY);
}

export function persistTheme(theme: Theme): void {
  localStorage.setItem(STORAGE_KEY, theme);
  document.documentElement.dataset.theme = theme;
}
