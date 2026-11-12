import { describe, expect, it, vi } from "vitest";
import type { Token } from "./highlight";
import {
  createHighlightWorkerHandler,
  workerTransport,
  type HighlightRequest,
  type HighlightResponse,
  type HighlightTransport,
  type WorkerLike,
} from "./highlightWorkerProtocol";

// #843: the highlighter runs in a Web Worker. The worker side answers tokenize requests and
// cancels; the page side posts requests, cancels them when the caller aborts, and falls back
// when the worker can't start.

const TOKENS: Token[][] = [[{ content: "x", color: "var(--shiki-token-keyword)" }]];

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("createHighlightWorkerHandler", () => {
  it("answers a tokenize request with the tokens", async () => {
    const tokenize = vi.fn().mockResolvedValue(TOKENS);
    const posted: HighlightResponse[] = [];
    const handle = createHighlightWorkerHandler({ tokenize, loadedLanguages: vi.fn() }, (m) => posted.push(m));

    handle({ type: "tokenize", id: 1, lang: "rust", code: "fn main() {}" });

    await vi.waitFor(() => expect(posted).toEqual([{ id: 1, tokens: TOKENS }]));
    expect(tokenize).toHaveBeenCalledWith("rust", "fn main() {}", expect.objectContaining({ signal: expect.any(AbortSignal) }));
  });

  it("aborts a running request on cancel and reports it cancelled, not its tokens", async () => {
    const running = deferred<Token[][]>();
    let signal: AbortSignal | undefined;
    const tokenize = vi.fn((_lang: string, _code: string, options: { signal?: AbortSignal }) => {
      signal = options.signal;
      signal?.addEventListener("abort", () => {
        const error = new Error("aborted");
        error.name = "AbortError";
        running.reject(error);
      });
      return running.promise;
    });
    const posted: HighlightResponse[] = [];
    const handle = createHighlightWorkerHandler({ tokenize, loadedLanguages: vi.fn() }, (m) => posted.push(m));

    handle({ type: "tokenize", id: 7, lang: "typescript", code: "const a = 1;" });
    handle({ type: "cancel", id: 7 });

    expect(signal?.aborted).toBe(true);
    await vi.waitFor(() => expect(posted).toEqual([{ id: 7, cancelled: true }]));
  });

  it("reports a failure as an error message", async () => {
    const tokenize = vi.fn().mockRejectedValue(new Error("boom"));
    const posted: HighlightResponse[] = [];
    const handle = createHighlightWorkerHandler({ tokenize, loadedLanguages: vi.fn() }, (m) => posted.push(m));

    handle({ type: "tokenize", id: 2, lang: "rust", code: "x" });

    await vi.waitFor(() => expect(posted).toEqual([{ id: 2, error: "boom" }]));
  });

  it("answers which grammars are loaded", async () => {
    const posted: HighlightResponse[] = [];
    const handle = createHighlightWorkerHandler(
      { tokenize: vi.fn(), loadedLanguages: vi.fn().mockResolvedValue(["rust"]) },
      (m) => posted.push(m),
    );

    handle({ type: "languages", id: 3 });

    await vi.waitFor(() => expect(posted).toEqual([{ id: 3, languages: ["rust"] }]));
  });
});

/** A worker whose messages the test answers by hand. */
class FakeWorker implements WorkerLike {
  readonly posted: HighlightRequest[] = [];
  terminated = false;
  private readonly listeners: { message: ((e: { data: HighlightResponse }) => void)[]; error: ((e: unknown) => void)[] } = {
    message: [],
    error: [],
  };

  postMessage(message: HighlightRequest): void {
    this.posted.push(message);
  }

  addEventListener(type: "message" | "error", listener: (event: never) => void): void {
    this.listeners[type].push(listener as never);
  }

  terminate(): void {
    this.terminated = true;
  }

  reply(data: HighlightResponse): void {
    for (const listener of this.listeners.message) listener({ data });
  }

  fail(): void {
    for (const listener of this.listeners.error) listener(new Event("error"));
  }
}

function lastRequestId(worker: FakeWorker): number {
  return worker.posted[worker.posted.length - 1].id;
}

describe("workerTransport", () => {
  it("posts a tokenize request and resolves with the worker's tokens", async () => {
    const worker = new FakeWorker();
    const create = vi.fn(() => worker);
    const transport = workerTransport(create);

    const result = transport.tokenize("rust", "fn main() {}");
    expect(worker.posted).toEqual([{ type: "tokenize", id: expect.any(Number), lang: "rust", code: "fn main() {}" }]);
    worker.reply({ id: lastRequestId(worker), tokens: TOKENS });

    await expect(result).resolves.toEqual(TOKENS);
  });

  it("starts the worker on first use and only once", async () => {
    const worker = new FakeWorker();
    const create = vi.fn(() => worker);
    const transport = workerTransport(create);
    expect(create).not.toHaveBeenCalled();

    const first = transport.tokenize("rust", "a");
    const second = transport.tokenize("rust", "b");
    worker.reply({ id: worker.posted[0].id, tokens: TOKENS });
    worker.reply({ id: worker.posted[1].id, tokens: [] });

    await expect(first).resolves.toEqual(TOKENS);
    await expect(second).resolves.toEqual([]);
    expect(create).toHaveBeenCalledTimes(1);
  });

  it("cancels the worker's job when the caller aborts", async () => {
    const worker = new FakeWorker();
    const transport = workerTransport(() => worker);
    const controller = new AbortController();

    const result = transport.tokenize("typescript", "const a = 1;", controller.signal);
    const id = lastRequestId(worker);
    controller.abort();

    await expect(result).rejects.toMatchObject({ name: "AbortError" });
    expect(worker.posted[1]).toEqual({ type: "cancel", id });
  });

  it("posts nothing for a request aborted before it is made", async () => {
    const worker = new FakeWorker();
    const transport = workerTransport(() => worker);
    const controller = new AbortController();
    controller.abort();

    await expect(transport.tokenize("rust", "x", controller.signal)).rejects.toMatchObject({ name: "AbortError" });
    expect(worker.posted).toEqual([]);
  });

  it("rejects with the worker's error message", async () => {
    const worker = new FakeWorker();
    const transport = workerTransport(() => worker);

    const result = transport.tokenize("rust", "x");
    worker.reply({ id: lastRequestId(worker), error: "boom" });

    await expect(result).rejects.toThrow("boom");
  });

  it("hands pending and later requests to the fallback when the worker fails", async () => {
    const worker = new FakeWorker();
    const fallback: HighlightTransport = {
      tokenize: vi.fn().mockResolvedValue(TOKENS),
      loadedLanguages: vi.fn().mockResolvedValue(["rust"]),
    };
    const create = vi.fn(() => worker);
    const transport = workerTransport(create, fallback);

    const pending = transport.tokenize("rust", "a");
    worker.fail();

    await expect(pending).resolves.toEqual(TOKENS);
    await expect(transport.tokenize("rust", "b")).resolves.toEqual(TOKENS);
    expect(fallback.tokenize).toHaveBeenCalledTimes(2);
    expect(create).toHaveBeenCalledTimes(1);
    expect(worker.terminated).toBe(true);
  });

  it("uses the fallback when the worker can't even be created", async () => {
    const fallback: HighlightTransport = {
      tokenize: vi.fn().mockResolvedValue(TOKENS),
      loadedLanguages: vi.fn(),
    };
    const transport = workerTransport(() => {
      throw new Error("blocked");
    }, fallback);

    await expect(transport.tokenize("rust", "a")).resolves.toEqual(TOKENS);
  });

  it("rejects when the worker fails and there is no fallback", async () => {
    const worker = new FakeWorker();
    const transport = workerTransport(() => worker);

    const pending = transport.tokenize("rust", "a");
    worker.fail();

    await expect(pending).rejects.toThrow();
    await expect(transport.tokenize("rust", "b")).rejects.toThrow();
  });

  it("asks the worker which grammars are loaded", async () => {
    const worker = new FakeWorker();
    const transport = workerTransport(() => worker);

    const result = transport.loadedLanguages();
    expect(worker.posted).toEqual([{ type: "languages", id: expect.any(Number) }]);
    worker.reply({ id: lastRequestId(worker), languages: ["rust"] });

    await expect(result).resolves.toEqual(["rust"]);
  });
});
