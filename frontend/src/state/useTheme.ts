import { useEffect, useState } from "react";
import { applyTheme, loadStoredTheme, resolveTheme, type Theme } from "../lib/theme";

/** The app's colour theme: the stored one, else the system's; applied to the document. */
export function useTheme() {
  const [theme, setTheme] = useState<Theme>(() =>
    resolveTheme(
      loadStoredTheme(),
      window.matchMedia("(prefers-color-scheme: dark)").matches,
    ),
  );
  useEffect(() => {
    applyTheme(theme);
  }, [theme]);
  return [theme, setTheme] as const;
}
