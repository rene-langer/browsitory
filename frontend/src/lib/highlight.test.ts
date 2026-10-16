import { describe, expect, it } from "vitest";
import { highlightLines } from "./highlight";

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
});
