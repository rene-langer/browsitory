import { describe, expect, it } from "vitest";
import { languageForPath } from "./languageForPath";

describe("languageForPath", () => {
  it.each([
    ["src/a.ts", "typescript"], ["App.tsx", "tsx"], ["x.mjs", "javascript"], ["lib.rs", "rust"],
    ["Cargo.toml", "toml"], ["a.yml", "yaml"], ["README.md", "markdown"], ["run.sh", "shellscript"],
    ["dir.with.dots/file.PY", "python"], ["main.cpp", "cpp"], ["x.hpp", "cpp"], ["y.cc", "cpp"],
  ])("%s → %s", (path, lang) => expect(languageForPath(path)).toBe(lang));

  it("returns null for unknown or extensionless files", () => {
    expect(languageForPath("LICENSE")).toBeNull();
    expect(languageForPath("data.bin")).toBeNull();
  });
});
