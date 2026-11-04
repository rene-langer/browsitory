import { describe, expect, it, vi } from "vitest";
import { excludeGrammarsPlugin } from "./vite.grammars.ts";

type Hook = (this: unknown, ...args: unknown[]) => unknown;
const LANGS = "/repo/frontend/node_modules/.pnpm/@shikijs+langs@4.6.0/node_modules/@shikijs/langs/dist";

function hooks() {
  const plugin = excludeGrammarsPlugin();
  return {
    resolveId: plugin.resolveId as unknown as Hook,
    load: plugin.load as unknown as Hook,
    generateBundle: plugin.generateBundle as unknown as Hook,
  };
}

describe("excludeGrammarsPlugin", () => {
  it("resolves an excluded grammar import from a Shiki language module to an empty list", () => {
    const { resolveId, load } = hooks();
    const id = resolveId.call({}, "./cpp.mjs", `${LANGS}/ruby.mjs`, {});
    expect(typeof id).toBe("string");
    expect(load.call({}, id)).toBe("export default [];");

    expect(resolveId.call({}, "./c.mjs", `${LANGS}/ruby.mjs`, {})).toBeNull();
    expect(resolveId.call({}, "./cpp.mjs", "/repo/frontend/src/lib/highlight.ts", {})).toBeNull();
    expect(load.call({}, `${LANGS}/c.mjs`)).toBeNull();
  });

  it("fails the build when an output file still defines an excluded grammar", () => {
    const { generateBundle } = hooks();
    const error = vi.fn((message: string) => {
      throw new Error(message);
    });
    const chunk = (code: string) => ({ "assets/x.js": { type: "chunk", code } });
    const ruby = 'JSON.parse("{\\"scopeName\\":\\"source.ruby\\",\\"patterns\\":[{\\"include\\":\\"source.cpp\\"}]}")';
    const glsl = 'JSON.parse("{\\"displayName\\":\\"GLSL\\",\\"scopeName\\":\\"source.glsl\\"}")';

    generateBundle.call({ error }, {}, chunk(ruby));
    expect(error).not.toHaveBeenCalled();

    expect(() => generateBundle.call({ error }, {}, chunk(glsl))).toThrow(/assets\/x\.js contains the excluded "glsl" grammar/);
  });
});
