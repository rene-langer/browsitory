import type { HighlightTransport } from "./highlightWorkerProtocol";
import type { Tokenizer } from "./highlightTokenizer";

/**
 * Highlighting on the page itself, in chunks with a turn of the event loop between them (about
 * 100 ms each for TypeScript), instead of one block of up to 2 s (#843). The fallback when the
 * worker can't start, and what the tests use (jsdom has no Worker). The tokenizer and its
 * grammars load on first use.
 */
export function inProcessTransport(): HighlightTransport {
  let tokenizer: Promise<Tokenizer> | null = null;
  const get = () => (tokenizer ??= import("./highlightTokenizer").then((m) => m.createTokenizer()));
  return {
    tokenize: async (lang, code, signal) => (await get()).tokenize(lang, code, { signal }),
    loadedLanguages: async () => (await get()).loadedLanguages(),
  };
}
