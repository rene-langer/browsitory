import { describe, expect, it } from "vitest";
import { loadStoredTheme, persistTheme, resolveTheme } from "./theme";

describe("resolveTheme", () => {
  it("uses the stored theme when it is a valid value", () => {
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
  });

  it("falls back to the OS preference when nothing is stored", () => {
    expect(resolveTheme(null, true)).toBe("dark");
    expect(resolveTheme(null, false)).toBe("light");
  });

  it("falls back to the OS preference when the stored value is invalid", () => {
    expect(resolveTheme("purple", true)).toBe("dark");
  });
});

describe("persistTheme / loadStoredTheme", () => {
  it("round-trips through localStorage and sets the document's data-theme", () => {
    persistTheme("dark");
    expect(loadStoredTheme()).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");

    persistTheme("light");
    expect(loadStoredTheme()).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});
