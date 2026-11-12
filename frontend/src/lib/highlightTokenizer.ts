import {
  createCssVariablesTheme,
  createHighlighterCore,
  type GrammarState,
  type HighlighterCore,
  type LanguageRegistration,
} from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import { withoutExcludedGrammars } from "./excludedGrammars";
import type { Token } from "./highlight";
import type { HighlightLanguage } from "./languageForPath";
import { abortError } from "./highlightWorkerProtocol";

/**
 * Shiki's tokenizer, chunked (#843). It runs in the highlight worker (`highlight.worker.ts`),
 * or on the page as the fallback when no worker can start (`highlightInProcess.ts`).
 */

/** Lines per chunk: about 100 ms for TypeScript, the slowest grammar measured. */
export const HIGHLIGHT_CHUNK_LINES = 200;

const THEME = createCssVariablesTheme({ name: "browsitory", variablePrefix: "--shiki-", fontStyle: false });

// Each loader is a static string so Vite can split it into its own chunk.
const LOADERS: Record<HighlightLanguage, () => Promise<unknown>> = {
  typescript: () => import("shiki/langs/typescript.mjs"),
  tsx: () => import("shiki/langs/tsx.mjs"),
  javascript: () => import("shiki/langs/javascript.mjs"),
  jsx: () => import("shiki/langs/jsx.mjs"),
  json: () => import("shiki/langs/json.mjs"),
  rust: () => import("shiki/langs/rust.mjs"),
  python: () => import("shiki/langs/python.mjs"),
  go: () => import("shiki/langs/go.mjs"),
  java: () => import("shiki/langs/java.mjs"),
  c: () => import("shiki/langs/c.mjs"),
  csharp: () => import("shiki/langs/csharp.mjs"),
  css: () => import("shiki/langs/css.mjs"),
  scss: () => import("shiki/langs/scss.mjs"),
  html: () => import("shiki/langs/html.mjs"),
  markdown: () => import("shiki/langs/markdown.mjs"),
  yaml: () => import("shiki/langs/yaml.mjs"),
  toml: () => import("shiki/langs/toml.mjs"),
  shellscript: () => import("shiki/langs/shellscript.mjs"),
  sql: () => import("shiki/langs/sql.mjs"),
  ruby: () => import("shiki/langs/ruby.mjs"),
  php: () => import("shiki/langs/php.mjs"),
  kotlin: () => import("shiki/langs/kotlin.mjs"),
  swift: () => import("shiki/langs/swift.mjs"),
};

export interface TokenizeOptions {
  /** Cancels the request: checked between chunks, so a cancel waits for one chunk at most. */
  signal?: AbortSignal;
  /** Lines per chunk; HIGHLIGHT_CHUNK_LINES by default. */
  chunkLines?: number;
  /** Gives the event loop a turn between chunks (a macrotask by default). */
  yieldToEventLoop?: () => Promise<void>;
}

export interface Tokenizer {
  tokenize(lang: HighlightLanguage, code: string, options?: TokenizeOptions): Promise<Token[][]>;
  /** The grammars registered so far (tests check that excluded ones never are). */
  loadedLanguages(): Promise<string[]>;
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (signal?.aborted) throw abortError();
}

/** A macrotask, so queued messages (a cancel) and, on the page, input and paint get a turn. */
function nextTask(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

export function createTokenizer(): Tokenizer {
  let highlighter: Promise<HighlighterCore> | null = null;
  const loaded = new Map<HighlightLanguage, Promise<void>>();

  const getHighlighter = () => {
    highlighter ??= createHighlighterCore({ themes: [THEME], langs: [], engine: createJavaScriptRegexEngine() });
    return highlighter;
  };

  const ensureLanguage = (h: HighlighterCore, lang: HighlightLanguage): Promise<void> => {
    let p = loaded.get(lang);
    if (p === undefined) {
      p = LOADERS[lang]().then((mod) =>
        h.loadLanguage(withoutExcludedGrammars((mod as { default: LanguageRegistration[] }).default)),
      );
      loaded.set(lang, p);
    }
    return p;
  };

  return {
    async tokenize(lang, code, options = {}) {
      const { signal, chunkLines = HIGHLIGHT_CHUNK_LINES, yieldToEventLoop = nextTask } = options;
      throwIfAborted(signal);
      const h = await getHighlighter();
      await ensureLanguage(h, lang);
      throwIfAborted(signal);
      // Shiki splits lines at "\n" and "\r\n"; so do the chunks.
      const lines = code.split(/\r?\n/);
      const result: Token[][] = [];
      let grammarState: GrammarState | undefined;
      for (let start = 0; start < lines.length; start += chunkLines) {
        if (start > 0) {
          await yieldToEventLoop();
          throwIfAborted(signal);
        }
        const chunk = lines.slice(start, start + chunkLines).join("\n");
        const tokens = h.codeToTokens(chunk, { lang, theme: "browsitory", grammarState });
        grammarState = tokens.grammarState;
        for (const line of tokens.tokens) result.push(line.map((t) => ({ content: t.content, color: t.color })));
      }
      return result;
    },
    async loadedLanguages() {
      return (await getHighlighter()).getLoadedLanguages();
    },
  };
}
