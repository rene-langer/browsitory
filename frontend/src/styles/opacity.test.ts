import { describe, expect, it } from "vitest";

// #1140: `tokens.contrast.test.ts` checks the colour tokens, but an `opacity` below 1 on a rule
// lowers what is on screen below what the tokens guarantee. Text and icons get a token that
// meets the contrast on its own (`--color-text-muted`), never a faded one.
const sheets = import.meta.glob<string>("../**/*.css", { query: "?raw", import: "default", eager: true });

/**
 * The rules that fade on purpose, by file and selector, each with why. Disabled controls (a
 * `:disabled` or `aria-disabled` selector) are exempt (WCAG 1.4.3), and so are keyframes.
 */
const ALLOWED: Record<string, Record<string, string>> = {
  "../components/CommitGraph.module.css": {
    "li.searchDimmed": "#687: the rows a search doesn't match step back (owner decision)",
    'li.searchDimmed[aria-selected="true"]': "#687: as above, a selected one less so",
  },
  "../components/primitives/ListRow.module.css": {
    '.row[data-dragging="true"]': "#744: the row being dragged, while the pointer holds it",
  },
};

interface Rule {
  selector: string;
  declarations: string;
  inKeyframes: boolean;
}

/** A sheet's rules with their declarations; nested rules (`@media`, CSS nesting) included. */
function rulesOf(css: string): Rule[] {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const rules: Rule[] = [];
  const stack: { selector: string; declarations: string }[] = [];
  let buffer = "";
  const inKeyframes = () => stack.some((block) => block.selector.startsWith("@keyframes"));
  for (const char of text) {
    if (char === "{") {
      // Declarations before a nested rule belong to the enclosing one.
      const split = buffer.lastIndexOf(";");
      if (stack.length > 0) stack[stack.length - 1].declarations += buffer.slice(0, split + 1);
      stack.push({ selector: buffer.slice(split + 1).trim().replace(/\s+/g, " "), declarations: "" });
      buffer = "";
    } else if (char === "}") {
      const block = stack.pop();
      if (block !== undefined) {
        rules.push({ selector: block.selector, declarations: block.declarations + buffer, inKeyframes: inKeyframes() || block.selector.startsWith("@keyframes") });
      }
      buffer = "";
    } else {
      buffer += char;
    }
  }
  return rules;
}

function opacityOf(declarations: string): number | null {
  const match = /(?:^|;)\s*opacity\s*:\s*([0-9.]+)(%?)\s*(?:!important\s*)?(?:;|$)/.exec(declarations);
  if (match === null) return null;
  return Number(match[1]) / (match[2] === "%" ? 100 : 1);
}

const disabledOnly = (selector: string) =>
  selector.split(",").every((part) => part.includes(":disabled") || part.includes("aria-disabled"));

/** Every rule that fades what it shows (an opacity between 0 and 1) without a reason. */
function fadedRules(): string[] {
  return Object.entries(sheets).flatMap(([path, css]) =>
    rulesOf(css).flatMap(({ selector, declarations, inKeyframes }) => {
      const opacity = opacityOf(declarations);
      if (opacity === null || opacity <= 0 || opacity >= 1 || inKeyframes) return [];
      if (disabledOnly(selector) || ALLOWED[path]?.[selector] !== undefined) return [];
      return [`${path.replace("../", "src/")} ${selector} { opacity: ${opacity} }`];
    }),
  );
}

describe("no faded text or icons (#1140)", () => {
  it("finds the sheets and reads their rules", () => {
    expect(Object.keys(sheets)).toContain("../index.css");
    const rules = rulesOf(".a { color: red; opacity: 0.5; } @media (x) { .b:hover { opacity: 1 } }");
    expect(rules.map((rule) => [rule.selector, opacityOf(rule.declarations)])).toEqual([
      [".a", 0.5],
      [".b:hover", 1],
      ["@media (x)", null],
    ]);
  });

  it("lowers no rule's opacity below 1, except disabled controls and the recorded exceptions", () => {
    expect(fadedRules()).toEqual([]);
  });

  it("keeps every recorded exception in use", () => {
    for (const [path, selectors] of Object.entries(ALLOWED)) {
      const rules = rulesOf(sheets[path] ?? "");
      for (const selector of Object.keys(selectors)) {
        const rule = rules.find((candidate) => candidate.selector === selector);
        expect(rule === undefined ? null : opacityOf(rule.declarations), `${path} ${selector}`).toBeLessThan(1);
      }
    }
  });
});
