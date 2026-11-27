import { describe, expect, it, vi } from "vitest";
import { createTokenizer } from "./highlightTokenizer";

// #843: the tokenizer works through a file in chunks of lines, carrying Shiki's grammar state
// from one chunk to the next, and gives the event loop a turn between chunks so a cancel (or, on
// the main thread, the UI) gets in.

const SOURCE = [
  "const a = 1;",
  "/* a comment",
  "   that spans",
  "   several lines",
  "   across chunks */",
  "const b = `template",
  "line ${a}",
  "end`;",
  "function f(x: number): string {",
  "  return String(x); // trailing",
  "}",
].join("\n");

// A machine so slow that every look at the clock is a second later than the last: Shiki's
// per-line time limit (it reads `Date.now()`) then cuts each line short at once.
async function onASlowMachine<T>(run: () => Promise<T>): Promise<T> {
  let calls = 0;
  const realNow = Date.now.bind(Date);
  const now = vi.spyOn(Date, "now").mockImplementation(() => realNow() + 1000 * calls++);
  try {
    return await run();
  } finally {
    now.mockRestore();
  }
}

describe("createTokenizer", () => {
  // #1210: the first, cold run compiles the grammar's patterns while it tokenizes the first line;
  // on a loaded machine that ran over Shiki's 500 ms per line, which cut the line short in one
  // run and not in the other. Without the time limit the tokens don't depend on the clock.
  it("gives the same tokens in chunks as in one piece", async () => {
    const tokenizer = createTokenizer();
    const whole = await tokenizer.tokenize("typescript", SOURCE, { chunkLines: 1000, timeLimitMs: 0 });
    const chunked = await tokenizer.tokenize("typescript", SOURCE, {
      chunkLines: 3,
      yieldToEventLoop: () => Promise.resolve(),
      timeLimitMs: 0,
    });
    expect(whole).toHaveLength(11);
    expect(chunked).toEqual(whole);
    // The comment's last line is in the second chunk, and still a comment.
    expect(chunked[4][0].color).toBe(chunked[1][0].color);
  }, 20000);

  it("cuts no line short on a slow machine when the time limit is off", async () => {
    const tokenizer = createTokenizer();
    const slow = await onASlowMachine(() =>
      tokenizer.tokenize("typescript", SOURCE, { chunkLines: 1000, timeLimitMs: 0 }),
    );
    const fast = await tokenizer.tokenize("typescript", SOURCE, { chunkLines: 1000, timeLimitMs: 0 });
    expect(slow).toEqual(fast);
  }, 20000);

  it("keeps Shiki's per-line time limit by default: a line over it is cut short", async () => {
    const tokenizer = createTokenizer();
    const full = await tokenizer.tokenize("typescript", "const a = 1;", { timeLimitMs: 0 });
    const slow = await onASlowMachine(() => tokenizer.tokenize("typescript", "const a = 1;"));
    expect(full[0].length).toBeGreaterThan(1);
    expect(slow).toEqual([[{ content: "const a = 1;", color: expect.any(String) }]]);
  }, 20000);

  it("yields to the event loop between chunks, not before the first", async () => {
    const tokenizer = createTokenizer();
    let yields = 0;
    await tokenizer.tokenize("typescript", SOURCE, {
      chunkLines: 3,
      yieldToEventLoop: () => {
        yields += 1;
        return Promise.resolve();
      },
    });
    // 11 lines in chunks of 3: four chunks, three turns in between.
    expect(yields).toBe(3);
  }, 20000);

  it("works in slices of 200 lines by default", async () => {
    const tokenizer = createTokenizer();
    let yields = 0;
    const code = Array.from({ length: 450 }, (_, i) => `let v${i} = ${i};`).join("\n");
    const tokens = await tokenizer.tokenize("typescript", code, {
      yieldToEventLoop: () => {
        yields += 1;
        return Promise.resolve();
      },
    });
    expect(tokens).toHaveLength(450);
    expect(yields).toBe(2);
  }, 20000);

  it("stops after the current chunk when cancelled", async () => {
    const tokenizer = createTokenizer();
    const controller = new AbortController();
    let yields = 0;
    const result = tokenizer.tokenize("typescript", SOURCE, {
      chunkLines: 3,
      signal: controller.signal,
      yieldToEventLoop: () => {
        yields += 1;
        controller.abort();
        return Promise.resolve();
      },
    });
    await expect(result).rejects.toMatchObject({ name: "AbortError" });
    expect(yields).toBe(1);
  }, 20000);

  it("rejects a request cancelled before it starts", async () => {
    const tokenizer = createTokenizer();
    const controller = new AbortController();
    controller.abort();
    await expect(tokenizer.tokenize("rust", "fn main() {}", { signal: controller.signal })).rejects.toMatchObject({
      name: "AbortError",
    });
  });

  it("lists the grammars it has loaded", async () => {
    const tokenizer = createTokenizer();
    await tokenizer.tokenize("rust", "fn main() {}");
    expect(await tokenizer.loadedLanguages()).toContain("rust");
  }, 20000);
});
