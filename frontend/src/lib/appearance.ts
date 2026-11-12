import {
  CODE_FONT_SIZES,
  DATE_FORMAT_VALUES,
  DEFAULT_APPEARANCE_SETTINGS,
  TAB_SIZES,
  UI_SIZE_VALUES,
  type AppearanceSettings,
} from "../ipc/RepoClient";

/** CSS generic family keywords, kept unquoted when typed as part of the code font. */
const GENERIC_FAMILIES = new Set([
  "serif",
  "sans-serif",
  "monospace",
  "cursive",
  "fantasy",
  "system-ui",
  "ui-serif",
  "ui-sans-serif",
  "ui-monospace",
  "ui-rounded",
  "math",
  "emoji",
  "fangsong",
]);

function clampNumber(value: unknown, range: { min: number; max: number }): number | null {
  if (typeof value !== "number" || !Number.isFinite(value)) return null;
  return Math.min(Math.max(Math.round(value), range.min), range.max);
}

/**
 * The settings as the views can use them, like the backend's `AppearanceSettings::clamped`:
 * a blank font is unset, sizes are whole numbers in range, and a missing or unknown value falls
 * back to the default.
 */
export function clampAppearance(settings: AppearanceSettings): AppearanceSettings {
  const font = typeof settings.codeFont === "string" ? settings.codeFont.trim() : "";
  return {
    codeFont: font === "" ? null : font,
    codeFontSize: clampNumber(settings.codeFontSize, CODE_FONT_SIZES),
    uiSize: UI_SIZE_VALUES.includes(settings.uiSize) ? settings.uiSize : DEFAULT_APPEARANCE_SETTINGS.uiSize,
    tabSize: clampNumber(settings.tabSize, TAB_SIZES) ?? DEFAULT_APPEARANCE_SETTINGS.tabSize,
    dateFormat: DATE_FORMAT_VALUES.includes(settings.dateFormat)
      ? settings.dateFormat
      : DEFAULT_APPEARANCE_SETTINGS.dateFormat,
  };
}

/** One family name as CSS: quoted (with `"` and `\` escaped) unless already quoted or generic. */
function cssFamily(name: string): string {
  if (/^"[^"\\]*"$|^'[^'\\]*'$/.test(name) || GENERIC_FAMILIES.has(name.toLowerCase())) return name;
  return `"${name.replace(/^["']|["']$/g, "").replace(/[\\"]/g, (c) => `\\${c}`)}"`;
}

/**
 * The `--font-mono` value for `codeFont`: its families first (a comma-separated list is allowed),
 * then the default monospace stack, so a font that isn't installed falls back to it.
 */
export function codeFontStack(codeFont: string | null): string {
  const families = (codeFont ?? "")
    .split(",")
    .map((name) => name.trim())
    .filter((name) => name !== "")
    .map(cssFamily);
  return [...families, "var(--font-mono-default)"].join(", ");
}

/**
 * Applies `settings` to the document at once, like the theme (`lib/theme.ts`): the code font,
 * code font size and tab size as custom properties on the root, the interface size as its
 * `data-ui-size` (see `styles/tokens.css`). Unset values remove their property, so each view
 * keeps its built-in size.
 */
export function applyAppearance(settings: AppearanceSettings, root: HTMLElement = document.documentElement): void {
  const { codeFont, codeFontSize, uiSize, tabSize } = clampAppearance(settings);
  if (codeFont === null) root.style.removeProperty("--font-mono");
  else root.style.setProperty("--font-mono", codeFontStack(codeFont));
  if (codeFontSize === null) root.style.removeProperty("--code-font-size");
  else root.style.setProperty("--code-font-size", `${codeFontSize}px`);
  root.style.setProperty("--tab-size", String(tabSize));
  if (uiSize === "Normal") delete root.dataset.uiSize;
  else root.dataset.uiSize = uiSize.toLowerCase();
}
