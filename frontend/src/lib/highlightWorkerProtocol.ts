import type { Token } from "./highlight";
import type { HighlightLanguage } from "./languageForPath";
import type { Tokenizer } from "./highlightTokenizer";

/**
 * The messages between the page and the highlight worker (#843), and both ends of them: the
 * worker's handler and the page's transport. A request the caller no longer needs is cancelled,
 * so the worker drops it after its current chunk instead of finishing a stale file first.
 */

export type HighlightRequest =
  | { type: "tokenize"; id: number; lang: HighlightLanguage; code: string }
  | { type: "cancel"; id: number }
  | { type: "languages"; id: number };

export type HighlightResponse =
  | { id: number; tokens: Token[][] }
  | { id: number; languages: string[] }
  | { id: number; cancelled: true }
  | { id: number; error: string };

/** How `highlight.ts` gets its tokens: from the worker, or on the page as the fallback. */
export interface HighlightTransport {
  tokenize(lang: HighlightLanguage, code: string, signal?: AbortSignal): Promise<Token[][]>;
  loadedLanguages(): Promise<string[]>;
}

/** The part of `Worker` the transport uses. */
export interface WorkerLike {
  postMessage(message: HighlightRequest): void;
  addEventListener(type: "message", listener: (event: { data: HighlightResponse }) => void): void;
  addEventListener(type: "error", listener: (event: unknown) => void): void;
  terminate(): void;
}

export function abortError(): Error {
  const error = new Error("Syntax highlighting was cancelled");
  error.name = "AbortError";
  return error;
}

export function isAbortError(error: unknown): boolean {
  return error instanceof Error && error.name === "AbortError";
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The worker's side: runs requests (several interleave, chunk by chunk) and cancels them. */
export function createHighlightWorkerHandler(
  tokenizer: Pick<Tokenizer, "tokenize" | "loadedLanguages">,
  post: (message: HighlightResponse) => void,
): (message: HighlightRequest) => void {
  const running = new Map<number, AbortController>();
  return (message) => {
    switch (message.type) {
      case "tokenize": {
        const controller = new AbortController();
        running.set(message.id, controller);
        const { id } = message;
        tokenizer.tokenize(message.lang, message.code, { signal: controller.signal }).then(
          (tokens) => post(controller.signal.aborted ? { id, cancelled: true } : { id, tokens }),
          (error: unknown) =>
            post(controller.signal.aborted || isAbortError(error) ? { id, cancelled: true } : { id, error: errorMessage(error) }),
        ).finally(() => running.delete(id));
        break;
      }
      case "cancel":
        running.get(message.id)?.abort();
        break;
      case "languages": {
        const { id } = message;
        tokenizer.loadedLanguages().then(
          (languages) => post({ id, languages }),
          (error: unknown) => post({ id, error: errorMessage(error) }),
        );
        break;
      }
    }
  };
}

interface Pending {
  resolve(value: never): void;
  reject(error: unknown): void;
  /** Runs the request again on the fallback when the worker fails. */
  retry(fallback: HighlightTransport): void;
}

/**
 * The page's side: starts the worker on first use, posts requests and cancels them when the
 * caller aborts. When the worker can't be created or reports an error (it failed to load), it
 * is dropped, and pending and later requests go to `fallback`; without one they fail.
 */
export function workerTransport(createWorker: () => WorkerLike, fallback?: HighlightTransport): HighlightTransport {
  let worker: WorkerLike | null = null;
  let failed: unknown = null;
  let nextId = 1;
  const pending = new Map<number, Pending>();

  const fail = (error: unknown) => {
    failed = error ?? new Error("The syntax highlighting worker failed");
    worker?.terminate();
    worker = null;
    const waiting = [...pending.values()];
    pending.clear();
    for (const request of waiting) {
      if (fallback !== undefined) request.retry(fallback);
      else request.reject(new Error("The syntax highlighting worker failed"));
    }
  };

  const ensureWorker = (): WorkerLike | null => {
    if (failed !== null) return null;
    if (worker !== null) return worker;
    try {
      const created = createWorker();
      created.addEventListener("message", ({ data }) => {
        const request = pending.get(data.id);
        if (request === undefined) return;
        pending.delete(data.id);
        if ("tokens" in data) request.resolve(data.tokens as never);
        else if ("languages" in data) request.resolve(data.languages as never);
        else if ("error" in data) request.reject(new Error(data.error));
        else request.reject(abortError());
      });
      created.addEventListener("error", (event) => fail(event));
      worker = created;
    } catch (error) {
      fail(error);
    }
    return worker;
  };

  const unavailable = () => Promise.reject(new Error("The syntax highlighting worker failed"));

  return {
    tokenize(lang, code, signal) {
      if (signal?.aborted) return Promise.reject(abortError());
      const w = ensureWorker();
      if (w === null) return fallback?.tokenize(lang, code, signal) ?? unavailable();
      const id = nextId++;
      return new Promise<Token[][]>((resolve, reject) => {
        const onAbort = () => {
          if (!pending.delete(id)) return;
          worker?.postMessage({ type: "cancel", id });
          reject(abortError());
        };
        const done = () => signal?.removeEventListener("abort", onAbort);
        pending.set(id, {
          resolve: (value) => {
            done();
            resolve(value);
          },
          reject: (error) => {
            done();
            reject(error);
          },
          retry: (other) => {
            done();
            other.tokenize(lang, code, signal).then(resolve, reject);
          },
        });
        signal?.addEventListener("abort", onAbort, { once: true });
        w.postMessage({ type: "tokenize", id, lang, code });
      });
    },
    loadedLanguages() {
      const w = ensureWorker();
      if (w === null) return fallback?.loadedLanguages() ?? unavailable();
      const id = nextId++;
      return new Promise<string[]>((resolve, reject) => {
        pending.set(id, { resolve, reject, retry: (other) => other.loadedLanguages().then(resolve, reject) });
        w.postMessage({ type: "languages", id });
      });
    },
  };
}
