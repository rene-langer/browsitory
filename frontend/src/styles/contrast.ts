/**
 * WCAG 2.2 contrast checks for the colour tokens in `tokens.css` (#666). Used by
 * `tokens.contrast.test.ts`: it reads the token blocks, resolves `var()` and the
 * `color-mix(in srgb, …)` forms the tokens use, and measures the pairs that must stay legible.
 */

export interface Rgba {
  r: number;
  g: number;
  b: number;
  /** 0–1. */
  a: number;
}

export type TokenMap = Record<string, string>;

/** Parses `#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()`/`rgba()` and `transparent`. */
export function parseColor(text: string): Rgba {
  const value = text.trim().toLowerCase();
  if (value === "transparent") return { r: 0, g: 0, b: 0, a: 0 };
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/.exec(value);
  if (hex) {
    const digits = hex[1].length === 3 ? [...hex[1]].map((d) => d + d).join("") : hex[1];
    const channel = (i: number) => parseInt(digits.slice(i, i + 2), 16);
    return { r: channel(0), g: channel(2), b: channel(4), a: digits.length === 8 ? channel(6) / 255 : 1 };
  }
  const rgb = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:[\s,/]+([\d.]+%?))?\s*\)$/.exec(value);
  if (rgb) {
    const alpha = rgb[4] === undefined ? 1 : rgb[4].endsWith("%") ? parseFloat(rgb[4]) / 100 : parseFloat(rgb[4]);
    return { r: Number(rgb[1]), g: Number(rgb[2]), b: Number(rgb[3]), a: alpha };
  }
  throw new Error(`Unsupported colour: ${text}`);
}

/** `top` drawn over the opaque `bottom`. */
export function composite(top: Rgba, bottom: Rgba): Rgba {
  const mix = (t: number, b: number) => t * top.a + b * (1 - top.a);
  return { r: mix(top.r, bottom.r), g: mix(top.g, bottom.g), b: mix(top.b, bottom.b), a: 1 };
}

/** `color-mix(in srgb, a p%, b)`: premultiplied interpolation, as CSS Color 5 defines it. */
function colorMix(a: Rgba, b: Rgba, p: number): Rgba {
  const alpha = a.a * p + b.a * (1 - p);
  if (alpha === 0) return { r: 0, g: 0, b: 0, a: 0 };
  const channel = (x: number, y: number) => (x * a.a * p + y * b.a * (1 - p)) / alpha;
  return { r: channel(a.r, b.r), g: channel(a.g, b.g), b: channel(a.b, b.b), a: alpha };
}

/** Splits `text` on top-level commas (not those inside parentheses). */
function splitArgs(text: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i += 1) {
    if (text[i] === "(") depth += 1;
    else if (text[i] === ")") depth -= 1;
    else if (text[i] === "," && depth === 0) {
      parts.push(text.slice(start, i).trim());
      start = i + 1;
    }
  }
  parts.push(text.slice(start).trim());
  return parts;
}

/** Resolves a colour value: a literal, `var(--name[, fallback])` or `color-mix(in srgb, …)`. */
export function resolveColor(value: string, tokens: TokenMap, seen: string[] = []): Rgba {
  const text = value.trim();
  const fn = /^([a-z-]+)\((.*)\)$/s.exec(text);
  if (fn && fn[1] === "var") {
    const [name, fallback] = splitArgs(fn[2]);
    if (seen.includes(name)) throw new Error(`Token cycle: ${[...seen, name].join(" -> ")}`);
    const token = tokens[name];
    if (token !== undefined) return resolveColor(token, tokens, [...seen, name]);
    if (fallback !== undefined) return resolveColor(fallback, tokens, seen);
    throw new Error(`Undefined token: ${name}`);
  }
  if (fn && fn[1] === "color-mix") {
    const [space, first, second] = splitArgs(fn[2]);
    if (space !== "in srgb") throw new Error(`Unsupported colour space: ${space}`);
    const percent = (part: string): [string, number | null] => {
      const match = /^(.*?)\s+([\d.]+)%$/s.exec(part);
      return match ? [match[1], Number(match[2]) / 100] : [part, null];
    };
    const [a, pa] = percent(first);
    const [b, pb] = percent(second);
    const p = pa ?? (pb === null ? 0.5 : 1 - pb);
    return colorMix(resolveColor(a, tokens, seen), resolveColor(b, tokens, seen), p);
  }
  return parseColor(text);
}

function relativeLuminance({ r, g, b }: Rgba): number {
  const linear = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

/** The WCAG contrast ratio of two opaque colours. */
export function contrastRatio(a: Rgba, b: Rgba): number {
  const [hi, lo] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/**
 * The declarations of the first rule whose selector is exactly `selector` (outside or inside an
 * at-rule), as `--name -> value`. Comments are dropped.
 */
export function tokenBlock(css: string, selector: string): TokenMap {
  const source = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = new RegExp(`(?:^|[}\\s])${escaped}\\s*\\{([^}]*)\\}`).exec(source);
  if (!match) throw new Error(`No rule for ${selector}`);
  const tokens: TokenMap = {};
  for (const declaration of match[1].split(";")) {
    const colon = declaration.indexOf(":");
    if (colon === -1) continue;
    const name = declaration.slice(0, colon).trim();
    if (name.startsWith("--")) tokens[name] = declaration.slice(colon + 1).trim();
  }
  return tokens;
}

/**
 * The ratio of `fg` on `bg` in a theme. A translucent `bg` is drawn over `base` first (the page
 * background unless given), a translucent `fg` over the result.
 */
export function tokenContrast(tokens: TokenMap, fg: string, bg: string, base = "var(--color-bg)"): number {
  const ground = resolveColor(base, tokens);
  const back = composite(resolveColor(bg, tokens), ground);
  const front = composite(resolveColor(fg, tokens), back);
  return contrastRatio(front, back);
}
