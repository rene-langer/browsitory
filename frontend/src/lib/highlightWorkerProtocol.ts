// Stub for the tests-first commit (#843); the implementation follows.
import type { Token } from "./highlight";
import type { HighlightLanguage } from "./languageForPath";
import type { Tokenizer } from "./highlightTokenizer";

export type HighlightRequest =
  | { type: "tokenize"; id: number; lang: HighlightLanguage; code: string }
  | { type: "cancel"; id: number }
  | { type: "languages"; id: number };

export type HighlightResponse =
  | { id: number; tokens: Token[][] }
  | { id: number; languages: string[] }
  | { id: number; cancelled: true }
  | { id: number; error: string };

export interface HighlightTransport {
  tokenize(lang: HighlightLanguage, code: string, signal?: AbortSignal): Promise<Token[][]>;
  loadedLanguages(): Promise<string[]>;
}

export interface WorkerLike {
  postMessage(message: HighlightRequest): void;
  addEventListener(type: "message", listener: (event: { data: HighlightResponse }) => void): void;
  addEventListener(type: "error", listener: (event: unknown) => void): void;
  terminate(): void;
}

export function createHighlightWorkerHandler(
  _tokenizer: Pick<Tokenizer, "tokenize" | "loadedLanguages">,
  _post: (message: HighlightResponse) => void,
): (message: HighlightRequest) => void {
  return () => {};
}

export function workerTransport(_create: () => WorkerLike, _fallback?: HighlightTransport): HighlightTransport {
  return {
    tokenize: () => Promise.reject(new Error("not implemented")),
    loadedLanguages: () => Promise.reject(new Error("not implemented")),
  };
}
