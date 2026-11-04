import { describe, expect, it } from "vitest";
import { highlightLines, loadedLanguages, withoutExcludedGrammars } from "./highlight";

describe("highlightLines", () => {
  it("tokenizes per line with css-variable colors", async () => {
    const lines = await highlightLines("typescript", "const a = 1;\n// note");
    expect(lines).toHaveLength(2);
    expect(lines[0].map((t) => t.content).join("")).toBe("const a = 1;");
    expect(lines[0].some((t) => t.color?.startsWith("var(--shiki-token"))).toBe(true);
  }, 20000);

  it("caches identical requests", async () => {
    const a = await highlightLines("rust", "fn main() {}");
    const b = await highlightLines("rust", "fn main() {}");
    expect(b).toBe(a);
  }, 20000);

  it("loads Ruby without the C++ grammars its module brings along", async () => {
    // Vitest doesn't run vite.grammars.ts, so shiki's ruby module still includes cpp, cpp-macro
    // and glsl here: this checks that highlight.ts itself never registers them.
    const lines = await highlightLines("ruby", "x = <<~CPP\n  int main() {}\nCPP\nputs x");
    expect(lines).toHaveLength(4);
    expect(lines[3].map((t) => t.content).join("")).toBe("puts x");
    const languages = await loadedLanguages();
    expect(languages).toEqual(expect.arrayContaining(["ruby", "c", "yaml"]));
    for (const excluded of ["cpp", "cpp-macro", "glsl"]) expect(languages).not.toContain(excluded);
  }, 20000);
});

describe("withoutExcludedGrammars", () => {
  it("drops excluded grammars and the embedded references to them", () => {
    const grammar = (name: string, embeddedLangs?: string[]) =>
      ({ name, scopeName: `source.${name}`, patterns: [], repository: {}, embeddedLangs }) as never;
    const result = withoutExcludedGrammars([
      grammar("glsl"),
      grammar("cpp-macro"),
      grammar("cpp", ["glsl"]),
      grammar("c"),
      grammar("ruby", ["cpp", "c", "yaml"]),
    ]);
    expect(result.map((l) => l.name)).toEqual(["c", "ruby"]);
    expect(result[1].embeddedLangs).toEqual(["c", "yaml"]);
  });
});
