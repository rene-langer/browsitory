import { beforeEach, describe, expect, it } from "vitest";
import { loadPersistedOpen, persistOpen } from "./persistedOpenState";

describe("persistedOpenState", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("returns the default when nothing is stored", () => {
    expect(loadPersistedOpen("k", true)).toBe(true);
    expect(loadPersistedOpen("k", false)).toBe(false);
  });

  it("round-trips true and false through localStorage", () => {
    persistOpen("k", true);
    expect(loadPersistedOpen("k", false)).toBe(true);
    persistOpen("k", false);
    expect(loadPersistedOpen("k", true)).toBe(false);
  });

  it("ignores unrelated storage keys", () => {
    persistOpen("other-key", true);
    expect(loadPersistedOpen("k", false)).toBe(false);
  });
});
