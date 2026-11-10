import { beforeEach, describe, expect, it } from "vitest";
import {
  applyThemePreference,
  loadThemePreference,
  parseThemePreference,
  persistThemePreference,
  resolveTheme,
} from "./theme";

beforeEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
});

describe("parseThemePreference", () => {
  it("keeps a stored light or dark choice", () => {
    expect(parseThemePreference("light")).toBe("light");
    expect(parseThemePreference("dark")).toBe("dark");
  });

  it("is system when nothing or an unknown value is stored", () => {
    expect(parseThemePreference(null)).toBe("system");
    expect(parseThemePreference("system")).toBe("system");
    expect(parseThemePreference("purple")).toBe("system");
  });
});

describe("resolveTheme", () => {
  it("uses an explicit choice whatever the system prefers", () => {
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
  });

  it("follows the system for system", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });
});

describe("persistThemePreference / loadThemePreference", () => {
  it("round-trips light and dark and pins the document's data-theme", () => {
    persistThemePreference("dark");
    expect(loadThemePreference()).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");

    persistThemePreference("light");
    expect(loadThemePreference()).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("goes back to following the system: clears the stored choice and data-theme", () => {
    persistThemePreference("dark");

    persistThemePreference("system");

    expect(localStorage.getItem("browsitory-theme")).toBeNull();
    expect(loadThemePreference()).toBe("system");
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});

describe("applyThemePreference", () => {
  it("sets the document's data-theme without touching localStorage", () => {
    applyThemePreference("dark");

    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(loadThemePreference()).toBe("system");
  });
});
