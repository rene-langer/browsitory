import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { GLSL_GRAMMAR } from "./grammars/glsl";
import {
  highlightLines,
  loadedLanguages,
  setHighlightTransport,
  useHunkTokens,
  withoutExcludedGrammars,
  type Token,
} from "./highlight";
import type { HighlightTransport } from "./highlightWorkerProtocol";

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

  it("loads Ruby with the C++ grammars its module brings along, and a glsl grammar for them", async () => {
    // #326: C++ ships again. Vitest doesn't run vite.grammars.ts, so shiki's ruby module still
    // includes the unlicensed glsl grammar here; `withoutExcludedGrammars` swaps in Browsitory's
    // own (see its tests below), so a `glsl` grammar is still registered.
    const lines = await highlightLines("ruby", "x = <<~CPP\n  int main() {}\nCPP\nputs x");
    expect(lines).toHaveLength(4);
    expect(lines[3].map((t) => t.content).join("")).toBe("puts x");
    const languages = await loadedLanguages();
    expect(languages).toEqual(expect.arrayContaining(["ruby", "c", "yaml", "cpp", "cpp-macro", "glsl"]));
  }, 20000);
});

describe("C++ highlighting (#326)", () => {
  const color = (line: Token[], content: string) => line.find((t) => t.content.trim() === content)?.color;

  it("highlights C++ with the C++ grammar", async () => {
    const [line] = await highlightLines("cpp", "template <typename T> class Box {};");
    expect(line.map((t) => t.content).join("")).toBe("template <typename T> class Box {};");
    // The C grammar has no `template` or `typename` keywords; C++'s does.
    expect(color(line, "template")).toMatch(/^var\(--shiki-token/);
    expect(color(line, "typename")).toMatch(/^var\(--shiki-token/);
  }, 20000);

  it("highlights GLSL in a glsl raw string with Browsitory's own grammar", async () => {
    const [line] = await highlightLines("cpp", 'auto src = R"glsl(uniform vec4 tint;)glsl";');
    expect(line.map((t) => t.content).join("")).toBe('auto src = R"glsl(uniform vec4 tint;)glsl";');
    // Inside the raw string, GLSL's qualifier and type are their own tokens, coloured unlike
    // the identifier next to them.
    const vec4 = color(line, "vec4");
    expect(vec4).toMatch(/^var\(--shiki-token/);
    expect(color(line, "uniform")).toMatch(/^var\(--shiki-token/);
    expect(color(line, "tint")).not.toBe(vec4);
  }, 20000);
});

describe("GLSL_GRAMMAR", () => {
  it("is a grammar for the scope the C++ grammar embeds, built on the C grammar", () => {
    expect(GLSL_GRAMMAR).toMatchObject({ name: "glsl", scopeName: "source.glsl", embeddedLangs: ["c"] });
    expect(GLSL_GRAMMAR.patterns.at(-1)).toEqual({ include: "source.c" });
  });
});

describe("withoutExcludedGrammars", () => {
  it("puts Browsitory's own glsl grammar in place of the excluded one", () => {
    const grammar = (name: string, embeddedLangs?: string[]) =>
      ({ name, scopeName: `source.${name}`, patterns: [], repository: {}, embeddedLangs }) as never;
    const upstream = grammar("glsl", ["c"]);
    const result = withoutExcludedGrammars([grammar("c"), upstream, grammar("cpp", ["cpp-macro", "glsl"])]);
    expect(result).not.toContain(upstream);
    expect(result[0]).toBe(GLSL_GRAMMAR);
    expect(result.map((l) => l.name)).toEqual(["glsl", "c", "cpp"]);
    expect(result[2].embeddedLangs).toEqual(["cpp-macro", "glsl"]);
  });

  it("adds the own glsl grammar only where something embeds glsl", () => {
    const grammar = (name: string, embeddedLangs?: string[]) =>
      ({ name, scopeName: `source.${name}`, patterns: [], repository: {}, embeddedLangs }) as never;
    expect(withoutExcludedGrammars([grammar("rust")]).map((l) => l.name)).toEqual(["rust"]);
    expect(withoutExcludedGrammars([grammar("glsl")]).map((l) => l.name)).toEqual([]);
  });

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
    // #326: only the unlicensed glsl grammar is excluded now, and Browsitory's own replaces it.
    expect(result.map((l) => l.name)).toEqual(["glsl", "cpp-macro", "cpp", "c", "ruby"]);
    expect(result[2].embeddedLangs).toEqual(["glsl"]);
    expect(result[4].embeddedLangs).toEqual(["cpp", "c", "yaml"]);
  });
});

// #843: highlighting runs off the main thread (a Web Worker in the app, a fake here). A request
// whose caller moved on is cancelled, so a stale file doesn't hold up the next one.
describe("highlightLines with a transport", () => {
  let restore: HighlightTransport | undefined;
  afterEach(() => {
    if (restore !== undefined) setHighlightTransport(restore);
    restore = undefined;
  });

  /** A transport whose requests stay pending until the test resolves them. */
  function pendingTransport() {
    const requests: { code: string; signal?: AbortSignal; resolve: (tokens: Token[][]) => void }[] = [];
    const transport: HighlightTransport = {
      tokenize: vi.fn(
        (_lang: string, code: string, signal?: AbortSignal) =>
          new Promise<Token[][]>((resolve) => requests.push({ code, signal, resolve })),
      ),
      loadedLanguages: vi.fn().mockResolvedValue([]),
    };
    return { transport, requests };
  }

  it("rejects and cancels the request when its caller aborts", async () => {
    const { transport, requests } = pendingTransport();
    restore = setHighlightTransport(transport);
    const controller = new AbortController();

    const result = highlightLines("rust", "fn aborted() {}", controller.signal);
    controller.abort();

    await expect(result).rejects.toMatchObject({ name: "AbortError" });
    expect(requests[0].signal?.aborted).toBe(true);
  });

  it("keeps a request two callers share until both abort", async () => {
    const { transport, requests } = pendingTransport();
    restore = setHighlightTransport(transport);
    const first = new AbortController();
    const second = new AbortController();

    const a = highlightLines("rust", "fn shared() {}", first.signal);
    const b = highlightLines("rust", "fn shared() {}", second.signal);
    expect(transport.tokenize).toHaveBeenCalledTimes(1);
    first.abort();
    await expect(a).rejects.toMatchObject({ name: "AbortError" });
    expect(requests[0].signal?.aborted).toBe(false);

    requests[0].resolve([[{ content: "fn", color: undefined }]]);
    await expect(b).resolves.toEqual([[{ content: "fn", color: undefined }]]);
  });

  it("asks the transport which grammars are loaded", async () => {
    const { transport } = pendingTransport();
    restore = setHighlightTransport(transport);
    await loadedLanguages();
    expect(transport.loadedLanguages).toHaveBeenCalled();
  });

  it("cancels a diff's requests when the diff goes away", async () => {
    const { transport, requests } = pendingTransport();
    restore = setHighlightTransport(transport);
    const hunks: DiffHunk[] = [
      {
        oldStart: 1,
        oldLines: 1,
        newStart: 1,
        newLines: 1,
        lines: [
          { origin: "Remove", content: "let gone = 1;\n" },
          { origin: "Add", content: "let unmounted = 2;\n" },
        ],
      },
    ];

    const { unmount } = renderHook(() => useHunkTokens(hunks, "typescript", true));
    await vi.waitFor(() => expect(requests).toHaveLength(2));
    unmount();

    expect(requests.map((r) => r.signal?.aborted)).toEqual([true, true]);
  });
});
