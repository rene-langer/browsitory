import { describe, expect, it } from "vitest";
import tokensCss from "./tokens.css?raw";
import { contrastRatio, parseColor, resolveColor, tokenBlock, tokenContrast, type TokenMap } from "./contrast";

// WCAG 2.2 AA (#666): 1.4.3 text, 1.4.11 UI parts and graphics, 2.4.13 the focus ring.
const TEXT = 4.5;
const UI = 3;

const t = (name: string) => `var(--color-${name})`;

/** Every surface text sits on. */
const TEXT_SURFACES = ["bg", "bg-subtle", "surface", "chrome", "control", "control-hover", "selected-bg", "hover-bg"];

interface Pair {
  fg: string;
  bg: string;
  min: number;
  /** The opaque colour a translucent `bg` is drawn over (the page background otherwise). */
  base?: string;
}

const each = (fg: string, bgs: string[], min: number): Pair[] => bgs.map((bg) => ({ fg: t(fg), bg: t(bg), min }));

const SHIKI = ["constant", "string", "comment", "keyword", "parameter", "function", "string-expression", "punctuation", "link"];
const CODE_SURFACES = ["surface", "bg", "bg-subtle", "diff-add-bg", "diff-remove-bg", "selected-bg"];

const PAIRS: Pair[] = [
  ...each("text", [...TEXT_SURFACES, "input-bg"], TEXT),
  ...each("text-muted", TEXT_SURFACES, TEXT),
  ...each("control-text", ["control", "control-hover"], TEXT),
  ...each("input-text", ["input-bg"], TEXT),
  ...each("placeholder", ["input-bg"], TEXT),
  ...each("selected-text", ["selected-bg"], TEXT),
  // "main (current)", links, "Copy", pressed toggle buttons, branch badges.
  ...each("accent-fg", [...TEXT_SURFACES, "accent-tint"], TEXT),
  { fg: t("accent-fg"), bg: t("accent-tint"), base: t("surface"), min: TEXT },
  // The Commit button and the HEAD badge.
  ...each("accent-text", ["accent", "accent-hover"], TEXT),
  ...each("danger-text", ["bg", "surface", "bg-subtle", "danger-bg"], TEXT),
  ...each("success-text", ["bg", "surface", "bg-subtle", "success-bg"], TEXT),
  ...each("warning-text", ["bg", "surface", "bg-subtle", "warning-bg"], TEXT),
  ...each("info-text", ["bg", "surface", "bg-subtle", "info-bg"], TEXT),
  ...each("diff-add-text", ["diff-add-bg", "bg", "surface"], TEXT),
  ...each("diff-remove-text", ["diff-remove-bg", "bg", "surface"], TEXT),
  ...each("diff-context-text", ["bg", "surface"], TEXT),
  // The armed "Discard" button: page colour on the danger colour.
  { fg: t("bg"), bg: t("danger-text"), min: TEXT },
  ...SHIKI.flatMap((token) =>
    CODE_SURFACES.map((bg) => ({ fg: `var(--shiki-token-${token})`, bg: t(bg), min: TEXT })),
  ),

  ...each("focus-ring", [...TEXT_SURFACES, "input-bg"], UI),
  ...each("accent", ["bg", "bg-subtle", "surface", "chrome"], UI),
  ...each("input-border", ["input-bg", "bg", "surface"], UI),
  ...each("selected-marker", ["selected-bg", "bg", "surface"], UI),
  ...[1, 2, 3, 4, 5, 6].flatMap((lane) => each(`lane-${lane}`, ["surface", "bg", "selected-bg"], UI)),
];

const LIGHT = tokenBlock(tokensCss, ":root");
const DARK = tokenBlock(tokensCss, ':root[data-theme="dark"]');
const SYSTEM_DARK = tokenBlock(tokensCss, ':root:not([data-theme="light"])');

function failures(tokens: TokenMap): string[] {
  return PAIRS.flatMap(({ fg, bg, min, base }) => {
    const ratio = tokenContrast(tokens, fg, bg, base);
    return ratio + 1e-9 < min ? [`${fg} on ${bg}${base ? ` over ${base}` : ""}: ${ratio.toFixed(2)} < ${min}`] : [];
  });
}

describe("colour token contrast (WCAG 2.2 AA)", () => {
  it("passes every pair in the light theme", () => {
    expect(failures(LIGHT)).toEqual([]);
  });

  it("passes every pair in the dark theme", () => {
    expect(failures({ ...LIGHT, ...DARK })).toEqual([]);
  });

  it("keeps the system dark block identical to the explicit dark theme", () => {
    expect(SYSTEM_DARK).toEqual(DARK);
  });
});

// Every colour goes through a token (#666): component styles and scripts never write a literal.
// System colours (`Canvas`, `Highlight`, …) in forced-colours rules are fine; they are keywords.
const TOKEN_FILES = ["/src/styles/tokens.css", "/src/styles/vscode-tokens.css"];
const STYLES = import.meta.glob<string>("/src/**/*.css", { query: "?raw", import: "default", eager: true });
const SCRIPTS = import.meta.glob<string>(["/src/**/*.{ts,tsx}", "!/src/**/*.test.{ts,tsx}"], {
  query: "?raw",
  import: "default",
  eager: true,
});

describe("colour literals", () => {
  it("appear in no stylesheet but the token files", () => {
    const literal = /#[0-9a-f]{3,8}\b|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color-mix)\(/i;
    const offenders = Object.entries(STYLES)
      .filter(([path]) => !TOKEN_FILES.includes(path))
      .flatMap(([path, css]) =>
        css
          .replace(/\/\*[\s\S]*?\*\//g, "")
          .split("\n")
          .filter((line) => literal.test(line))
          .map((line) => `${path}: ${line.trim()}`),
      );
    expect(Object.keys(STYLES).length).toBeGreaterThan(10);
    expect(offenders).toEqual([]);
  });

  it("appear in no script", () => {
    const literal = /["'`](?:#[0-9a-f]{3,8}|(?:rgba?|hsla?)\([^"'`]*\))["'`]/i;
    const comment = /^\s*(?:\/\/|\/?\*)/;
    const offenders = Object.entries(SCRIPTS).flatMap(([path, source]) =>
      source
        .split("\n")
        .filter((line) => !comment.test(line) && literal.test(line))
        .map((line) => `${path}: ${line.trim()}`),
    );
    expect(Object.keys(SCRIPTS).length).toBeGreaterThan(10);
    expect(offenders).toEqual([]);
  });
});

describe("contrast helpers", () => {
  it("measures black on white as 21:1 and a colour on itself as 1:1", () => {
    expect(contrastRatio(parseColor("#000"), parseColor("#ffffff"))).toBeCloseTo(21, 5);
    expect(contrastRatio(parseColor("#777777"), parseColor("#777777"))).toBe(1);
  });

  it("resolves var() chains, fallbacks and color-mix with transparent", () => {
    const tokens = { "--a": "#ff0000", "--b": "var(--a)", "--c": "color-mix(in srgb, var(--b) 25%, transparent)" };
    expect(resolveColor("var(--b)", tokens)).toEqual({ r: 255, g: 0, b: 0, a: 1 });
    expect(resolveColor("var(--missing, #00ff00)", tokens)).toEqual({ r: 0, g: 255, b: 0, a: 1 });
    expect(resolveColor("var(--c)", tokens)).toEqual({ r: 255, g: 0, b: 0, a: 0.25 });
    expect(resolveColor("color-mix(in srgb, #000000 50%, #ffffff)", tokens)).toEqual({ r: 127.5, g: 127.5, b: 127.5, a: 1 });
  });

  it("draws a translucent background over the base before measuring", () => {
    const tokens = { "--color-bg": "#ffffff", "--fg": "#000000", "--tint": "rgba(0, 0, 0, 0.5)" };
    const half = contrastRatio(parseColor("#000000"), { r: 127.5, g: 127.5, b: 127.5, a: 1 });
    expect(tokenContrast(tokens, "var(--fg)", "var(--tint)")).toBeCloseTo(half, 5);
  });
});
