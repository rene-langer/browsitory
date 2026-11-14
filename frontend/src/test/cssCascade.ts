// A small model of the CSS cascade for tests: jsdom lays nothing out and Vitest stubs module CSS,
// so a test that needs to know which rule wins reads the stylesheets with `?raw` and checks the
// rules (#970, #972). It covers the selectors this app's stylesheets use: classes, elements,
// attributes, pseudo-classes and `:where(...)`, which adds no specificity.

export interface Rule {
  selector: string;
  body: string;
}

// The rules of a stylesheet: each selector of a rule (split at commas) with its declarations.
export function rulesOf(css: string): Rule[] {
  const plain = css.replace(/\/\*[\s\S]*?\*\//g, "");
  return [...plain.matchAll(/([^{}]+)\{([^{}]*)\}/g)].flatMap(([, selectors, body]) =>
    splitSelectors(selectors).map((selector) => ({ selector: selector.trim(), body })),
  );
}

// Splits a selector list at its top-level commas (not those inside `:where(...)`).
function splitSelectors(list: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < list.length; i += 1) {
    if (list[i] === "(") depth += 1;
    else if (list[i] === ")") depth -= 1;
    else if (list[i] === "," && depth === 0) {
      parts.push(list.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(list.slice(start));
  return parts;
}

const withoutWhere = (selector: string) => selector.replace(/:where\([^()]*\)/g, "");

// [ids, classes/attributes/pseudo-classes, elements] of one selector. `:where(...)` counts 0.
export function specificity(selector: string): [number, number, number] {
  const counted = withoutWhere(selector);
  const ids = (counted.match(/#[\w-]+/g) ?? []).length;
  const classes = (counted.match(/\.[\w-]+|\[[^\]]*\]|:(?!:)[\w-]+/g) ?? []).length;
  const elements = (counted.replace(/[.#:][\w-]+|\[[^\]]*\]/g, "").match(/[a-z][\w-]*/gi) ?? []).length;
  return [ids, classes, elements];
}

function compare(a: [number, number, number], b: [number, number, number]): number {
  for (let i = 0; i < 3; i += 1) if (a[i] !== b[i]) return a[i] - b[i];
  return 0;
}

// The compound selectors of a selector, `:where(...)` unwrapped: ".dialog label" and
// ":where(.dialog) label" both give [".dialog", "label"].
export function compoundsOf(selector: string): string[] {
  return selector
    .replace(/:where\(([^()]*)\)/g, "$1")
    .split(/[\s>+~]+/)
    .filter(Boolean);
}

// The last compound selector: the element the rule styles.
export const subjectOf = (selector: string) => compoundsOf(selector).at(-1) ?? "";

// The value a rule's body declares for `property`, or null.
export function declarationOf(body: string, property: string): { value: string; important: boolean } | null {
  const found = [...body.matchAll(/([\w-]+)\s*:\s*([^;]+)/g)].filter(([, name]) => name === property).at(-1);
  if (!found) return null;
  const raw = found[2].trim();
  const important = /!\s*important$/.test(raw);
  return { value: raw.replace(/\s*!\s*important$/, ""), important };
}

/**
 * The value `property` gets from `rules`, all of which match the element: `!important` first,
 * then the highest specificity. Rules from different CSS modules have no dependable order, so
 * two equally specific rules that disagree make it throw rather than guess. Null when none of
 * the rules declares the property.
 */
export function cascadedValue(rules: Rule[], property: string): string | null {
  const declared = rules.flatMap((rule) => {
    const declaration = declarationOf(rule.body, property);
    return declaration ? [{ ...declaration, selector: rule.selector, specificity: specificity(rule.selector) }] : [];
  });
  if (declared.length === 0) return null;
  const important = declared.filter((d) => d.important);
  const candidates = important.length > 0 ? important : declared;
  const best = candidates.reduce((a, b) => (compare(b.specificity, a.specificity) > 0 ? b : a));
  const tied = candidates.filter((d) => compare(d.specificity, best.specificity) === 0 && d.value !== best.value);
  if (tied.length > 0) {
    const selectors = [best, ...tied].map((d) => d.selector).join(", ");
    throw new Error(`${property}: ${selectors} tie on specificity; the winner depends on load order`);
  }
  return best.value;
}
