import { createOnigurumaEngine } from "shiki/engine/oniguruma";
import { describe, expect, it, vi } from "vitest";
import { createTokenizer, polyfillResponseBytes } from "./highlightTokenizer";

// #843 (owner decision): Shiki's Oniguruma WebAssembly engine, about 4 times faster than the
// JavaScript regex engine on TypeScript in WebKitGTK and about 3 times in V8.
vi.mock("shiki/engine/oniguruma", async (importOriginal) => {
  const actual = await importOriginal<typeof import("shiki/engine/oniguruma")>();
  return { ...actual, createOnigurumaEngine: vi.fn(actual.createOnigurumaEngine) };
});
vi.mock("shiki/engine/javascript", () => ({
  createJavaScriptRegexEngine: vi.fn(() => {
    throw new Error("the JavaScript regex engine was used");
  }),
}));

describe("createTokenizer's engine", () => {
  it("tokenizes with the Oniguruma WebAssembly engine", async () => {
    const tokens = await createTokenizer().tokenize("rust", "fn main() {}");
    expect(createOnigurumaEngine).toHaveBeenCalledTimes(1);
    expect(tokens[0][0]).toEqual({ content: "fn", color: "var(--shiki-token-keyword)" });
  }, 20000);
});

// Shiki's inlined WebAssembly loader reads the binary with `Response.bytes()`, which older
// webviews (WebKit before Safari 18, so older macOS) lack.
describe("polyfillResponseBytes", () => {
  it("adds bytes() where a Response lacks it", async () => {
    class OldResponse {
      arrayBuffer(): Promise<ArrayBuffer> {
        return Promise.resolve(new Uint8Array([1, 2, 3]).buffer);
      }
    }
    polyfillResponseBytes(OldResponse.prototype);
    const bytes = await (new OldResponse() as unknown as { bytes(): Promise<Uint8Array> }).bytes();
    expect(bytes).toBeInstanceOf(Uint8Array);
    expect([...bytes]).toEqual([1, 2, 3]);
  });

  it("leaves a native bytes() alone", () => {
    const native = () => Promise.resolve(new Uint8Array());
    const proto = { bytes: native, arrayBuffer: () => Promise.resolve(new ArrayBuffer(0)) };
    polyfillResponseBytes(proto);
    expect(proto.bytes).toBe(native);
  });
});
