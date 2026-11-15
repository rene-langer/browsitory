// A small model of a CSS grid row for tests (#931): jsdom lays nothing out, so a test that needs
// to know how wide a grid cell's content is works it out from the stylesheet's rules (read with
// `?raw`, see cssCascade.ts) and the inline custom properties the component sets. Widths are kept
// symbolic, as a sum of named terms: `fr` for a `minmax(0, 1fr)` track's share of the free
// space, `px` for fixed pixels and `--name` for a custom property the model doesn't resolve. Two
// cells whose widths have the same terms are equally wide at every grid width.

import { cascadedValue, compoundsOf, type Rule } from "./cssCascade";

/** A width as a sum of terms: `{ fr: 1, "--merge-arrow": -1 }` is one share less the arrow. */
export type Width = Record<string, number>;

function plus(a: Width, b: Width, factor = 1): Width {
  const sum = { ...a };
  for (const [term, count] of Object.entries(b)) {
    sum[term] = (sum[term] ?? 0) + factor * count;
    if (sum[term] === 0) delete sum[term];
  }
  return sum;
}

/** Replaces each `var(--name)` that `vars` defines with its value, until none is left. */
export function substitute(value: string, vars: Record<string, string>): string {
  let result = value;
  for (let round = 0; round < 10; round += 1) {
    const next = result.replace(/var\((--[\w-]+)\)/g, (whole, name: string) => vars[name] ?? whole);
    if (next === result) return result;
    result = next;
  }
  throw new Error(`custom properties refer to each other: ${value}`);
}

/** One length: `0`, `Npx`, `minmax(0, Nfr)`, `var(--name)` or a `calc()` of them with + and -. */
export function widthOf(value: string, vars: Record<string, string> = {}): Width {
  const text = substitute(value, vars).trim();
  if (text === "0") return {};
  const px = /^(-?\d+(?:\.\d+)?)px$/.exec(text);
  if (px) return { px: Number(px[1]) };
  const share = /^minmax\(0,\s*(\d+)fr\)$/.exec(text);
  if (share) return { fr: Number(share[1]) };
  const variable = /^var\((--[\w-]+)\)$/.exec(text);
  if (variable) return { [variable[1]]: 1 };
  const calc = /^calc\((.*)\)$/.exec(text);
  if (calc) {
    // calc's + and - need spaces around them, so "a - b" is a difference and "-1px" is not.
    const inner = calc[1];
    let sum: Width = {};
    let sign = 1;
    let depth = 0;
    let current = "";
    for (let i = 0; i < inner.length; i += 1) {
      const char = inner[i];
      if (char === "(") depth += 1;
      if (char === ")") depth -= 1;
      if (depth === 0 && (char === "+" || char === "-") && inner[i - 1] === " " && inner[i + 1] === " ") {
        sum = plus(sum, widthOf(current, vars), sign);
        sign = char === "-" ? -1 : 1;
        current = "";
      } else {
        current += char;
      }
    }
    return plus(sum, widthOf(current, vars), sign);
  }
  // A bare `1fr` grows to its content's minimum width, so its share isn't equal to the others'.
  throw new Error(`the grid model can't measure "${text}"`);
}

/** Splits `text` at its top-level whitespace (not inside parentheses). */
function splitTracks(text: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const char of text) {
    if (char === "(") depth += 1;
    if (char === ")") depth -= 1;
    if (depth === 0 && /\s/.test(char)) {
      parts.push(current);
      current = "";
    } else {
      current += char;
    }
  }
  parts.push(current);
  return parts.filter((part) => part !== "");
}

/** A `grid-template-columns` value's tracks, `repeat(n, …)` expanded. */
export function tracksOf(template: string, vars: Record<string, string> = {}): Width[] {
  return splitTracks(substitute(template, vars)).flatMap((track) => {
    const repeat = /^repeat\((\d+),\s*(.*)\)$/.exec(track);
    if (repeat) return Array.from({ length: Number(repeat[1]) }, () => tracksOf(repeat[2])).flat();
    return [widthOf(track)];
  });
}

/** Whether a compound selector (`.cell[data-pane="ours"]`) matches an element with `classes`. */
function compoundMatches(compound: string, element: Element, classes: readonly string[]): boolean {
  if (/:/.test(compound.replace(/\[[^\]]*\]/g, ""))) return false;
  const needed = compound.match(/\.[\w-]+/g) ?? [];
  if (!needed.every((name) => classes.includes(name.slice(1)))) return false;
  return [...compound.matchAll(/\[([\w-]+)(?:="([^"]*)")?\]/g)].every(([, name, value]) =>
    value === undefined ? element.hasAttribute(name) : element.getAttribute(name) === value,
  );
}

/**
 * The rules of a stylesheet that style `element` on their own, given the classes the stylesheet
 * gives it (Vitest stubs CSS modules, so the rendered element doesn't carry them). Rules that
 * need an ancestor or a pseudo-class are left out.
 */
export function rulesFor(rules: Rule[], element: Element, classes: readonly string[]): Rule[] {
  return rules.filter(({ selector }) => {
    const compounds = compoundsOf(selector);
    return compounds.length === 1 && compoundMatches(compounds[0], element, classes);
  });
}

/** How many tracks a `grid-column` value spans: `span N`, `1 / -1` (all of them) or one. */
function spanOf(gridColumn: string | null, trackCount: number): number {
  if (gridColumn === null) return 1;
  const span = /^span\s+(\d+)$/.exec(gridColumn.trim());
  if (span) return Number(span[1]);
  if (/^1\s*\/\s*-1$/.test(gridColumn.trim())) return trackCount;
  throw new Error(`the grid model can't place "grid-column: ${gridColumn}"`);
}

/** A cell's place in its row: the first track it covers and how many. */
export interface Placement {
  start: number;
  span: number;
  /** Its content box's width: its tracks and the gaps between them, less padding and border. */
  content: Width;
}

/**
 * Places a row's cells, in order, one after the other on the row's tracks (auto-placement in a
 * single row), and works out each one's content width.
 */
export function placeRow(
  cells: { rules: Rule[] }[],
  tracks: Width[],
  gap: Width,
  vars: Record<string, string> = {},
): Placement[] {
  let next = 0;
  return cells.map(({ rules }) => {
    const span = spanOf(cascadedValue(rules, "grid-column"), tracks.length);
    const start = next;
    next += span;
    if (next > tracks.length) throw new Error(`the row's cells need ${next} tracks; it has ${tracks.length}`);
    let content = tracks.slice(start, start + span).reduce((sum, track) => plus(sum, track), {});
    content = plus(content, gap, span - 1);
    for (const property of ["padding-left", "padding-right"]) {
      const value = cascadedValue(rules, property);
      if (value !== null) content = plus(content, widthOf(value, vars), -1);
    }
    for (const property of ["border-left", "border-right"]) {
      const value = cascadedValue(rules, property);
      if (value !== null) content = plus(content, widthOf(value.split(/\s+/)[0], vars), -1);
    }
    return { start, span, content };
  });
}
