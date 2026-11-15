import { describe, expect, it } from "vitest";
import { cascadedValue, compoundsOf, rulesOf, type Rule } from "../test/cssCascade";
import indexCss from "../index.css?raw";
import inlineErrorCss from "./primitives/InlineError.module.css?raw";
import graphFilterStripCss from "./GraphFilterStrip.module.css?raw";

// A hovered button's fill (#984, #985): `index.css`'s `button:hover:not(:disabled):not(...)`
// (0,3,1) outranks a module's `.action.primary` or `.showAll:hover` (0,2,0), and its
// `--color-control-hover` fill under text drawn in the banner's colours hides the label.

/** An enabled `<button>` with `classes`, hovered or not. */
interface Button {
  classes: string[];
  hover: boolean;
}

/** Whether a one-compound `selector` matches `button`; a selector with combinators never does. */
function matches(selector: string, button: Button): boolean {
  if (compoundsOf(selector).length !== 1) return false;
  let rest = selector.replace(/:not\(:disabled\)|:not\(\[aria-disabled="true"\]\)/g, "");
  if (rest.includes(":hover")) {
    if (!button.hover) return false;
    rest = rest.replace(/:hover/g, "");
  }
  const parts = rest.match(/^[a-z]+|\.[\w-]+/gi) ?? [];
  if (parts.join("") !== rest) return false;
  return parts.every((part) => (part.startsWith(".") ? button.classes.includes(part.slice(1)) : part === "button"));
}

const rulesFor = (button: Button, ...sheets: string[]): Rule[] =>
  sheets.flatMap((css) => rulesOf(css)).filter((rule) => matches(rule.selector, button));

describe("hovered banner and strip buttons keep their colours (B2)", () => {
  it("the error banner's filled first step (Pull, Remove) keeps its fill on hover", () => {
    const hovered = rulesFor({ classes: ["action", "primary"], hover: true }, indexCss, inlineErrorCss);
    expect(cascadedValue(hovered, "background")).toBe("var(--color-danger-text)");
    expect(cascadedValue(hovered, "color")).toBe("var(--color-danger)");
  });

  it("the error banner's ✕ inverts on hover", () => {
    const hovered = rulesFor({ classes: ["dismiss"], hover: true }, indexCss, inlineErrorCss);
    expect(cascadedValue(hovered, "background")).toBe("var(--color-danger-text)");
    expect(cascadedValue(hovered, "color")).toBe("var(--color-danger)");
  });

  it("the graph filter strip's Show all branches inverts on hover", () => {
    const hovered = rulesFor({ classes: ["showAll"], hover: true }, indexCss, graphFilterStripCss);
    expect(cascadedValue(hovered, "background")).toBe("var(--color-info-text)");
    expect(cascadedValue(hovered, "color")).toBe("var(--color-info-bg)");
  });
});
