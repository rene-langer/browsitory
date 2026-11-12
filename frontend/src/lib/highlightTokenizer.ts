// Stub for the tests-first commit (#843); the implementation follows.
import type { Token } from "./highlight";
import type { HighlightLanguage } from "./languageForPath";

export interface TokenizeOptions {
  signal?: AbortSignal;
  chunkLines?: number;
  yieldToEventLoop?: () => Promise<void>;
}

export interface Tokenizer {
  tokenize(lang: HighlightLanguage, code: string, options?: TokenizeOptions): Promise<Token[][]>;
  loadedLanguages(): Promise<string[]>;
}

export function createTokenizer(): Tokenizer {
  return {
    tokenize: () => Promise.reject(new Error("not implemented")),
    loadedLanguages: () => Promise.reject(new Error("not implemented")),
  };
}
