import { afterEach, describe, expect, it } from "vitest";
import { applyAppearance, clampAppearance, codeFontStack } from "./appearance";

const defaults = { codeFont: null, codeFontSize: null, uiSize: "Normal" as const, tabSize: 4, dateFormat: "Iso" as const };

describe("clampAppearance", () => {
  it("keeps valid settings", () => {
    const settings = { codeFont: "Example Mono", codeFontSize: 14, uiSize: "Large" as const, tabSize: 8, dateFormat: "System" as const };
    expect(clampAppearance(settings)).toEqual(settings);
  });

  it("clamps sizes into range and rounds them", () => {
    expect(clampAppearance({ ...defaults, codeFontSize: 1, tabSize: 0 })).toEqual({ ...defaults, codeFontSize: 10, tabSize: 1 });
    expect(clampAppearance({ ...defaults, codeFontSize: 99, tabSize: 99 })).toEqual({ ...defaults, codeFontSize: 24, tabSize: 16 });
    expect(clampAppearance({ ...defaults, codeFontSize: 13.6, tabSize: 2.4 })).toEqual({ ...defaults, codeFontSize: 14, tabSize: 2 });
  });

  it("falls back to the defaults for blank, missing or unknown values", () => {
    expect(clampAppearance({ codeFont: "  ", uiSize: "Huge", tabSize: Number.NaN } as never)).toEqual(defaults);
    expect(clampAppearance({} as never)).toEqual(defaults);
  });
});

describe("codeFontStack", () => {
  it("is the default stack without a code font", () => {
    expect(codeFontStack(null)).toBe("var(--font-mono-default)");
  });

  it("quotes family names, keeps quoted and generic ones, and ends with the default stack", () => {
    expect(codeFontStack("Example Mono, 'Other Mono', MONOSPACE")).toBe(
      "\"Example Mono\", 'Other Mono', MONOSPACE, var(--font-mono-default)",
    );
  });

  it("escapes what would end the quoted name", () => {
    expect(codeFontStack('Bad"; color: red')).toBe('"Bad\\"; color: red", var(--font-mono-default)');
  });
});

describe("applyAppearance", () => {
  const root = document.createElement("div");

  afterEach(() => {
    root.removeAttribute("style");
    delete root.dataset.uiSize;
  });

  it("sets the custom properties and the interface size, and removes them again", () => {
    applyAppearance({ codeFont: "Example Mono", codeFontSize: 15, uiSize: "Large", tabSize: 2, dateFormat: "Iso" }, root);
    expect(root.style.getPropertyValue("--font-mono")).toBe('"Example Mono", var(--font-mono-default)');
    expect(root.style.getPropertyValue("--code-font-size")).toBe("15px");
    expect(root.style.getPropertyValue("--tab-size")).toBe("2");
    expect(root.dataset.uiSize).toBe("large");

    applyAppearance(defaults, root);
    expect(root.style.getPropertyValue("--font-mono")).toBe("");
    expect(root.style.getPropertyValue("--code-font-size")).toBe("");
    expect(root.style.getPropertyValue("--tab-size")).toBe("4");
    expect(root.dataset.uiSize).toBeUndefined();
  });
});
