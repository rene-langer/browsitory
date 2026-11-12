import {
  createCssVariablesTheme,
  createHighlighterCore,
  type GrammarState,
  type HighlighterCore,
  type LanguageRegistration,
} from "shiki/core";
import { createOnigurumaEngine } from "shiki/engine/oniguruma";
import { withoutExcludedGrammars } from "./excludedGrammars";
import type { Token } from "./highlight";
import type { HighlightLanguage } from "./languageForPath";
import { abortError } from "./highlightWorkerProtocol";

/**
 * Shiki's tokenizer, chunked (#843). It runs in the highlight worker (`highlight.worker.ts`),
 * or on the page as the fallback when no worker can start (`highlightInProcess.ts`).
 */

/** Lines per chunk: tens of ms for TypeScript, the slowest grammar measured. */
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
  cpp: () => import("shiki/langs/cpp.mjs"),
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

/**
 * Shiki's inlined WebAssembly loader (`shiki/wasm`) reads the binary with `Response.bytes()`,
 * which WebKit added only in Safari 18 (older macOS keeps an older WKWebView).
 */
export function polyfillResponseBytes(proto: { bytes?: unknown; arrayBuffer(): Promise<ArrayBuffer> }): void {
  if (typeof proto.bytes === "function") return;
  proto.bytes = async function bytes(this: { arrayBuffer(): Promise<ArrayBuffer> }) {
    return new Uint8Array(await this.arrayBuffer());
  };
}

export function createTokenizer(): Tokenizer {
  let highlighter: Promise<HighlighterCore> | null = null;
  const loaded = new Map<HighlightLanguage, Promise<void>>();

  const getHighlighter = () => {
    if (typeof Response !== "undefined") polyfillResponseBytes(Response.prototype);
    // The Oniguruma WebAssembly engine (#843): about 4 times faster than Shiki's JavaScript
    // regex engine on TypeScript in WebKitGTK. Compiling it needs 'wasm-unsafe-eval' in the
    // CSP (crates/tauri-app/tauri.conf.json, extension/src/webviewHtml.ts).
    highlighter ??= createHighlighterCore({
      themes: [THEME],
      langs: [],
      engine: createOnigurumaEngine(import("shiki/wasm")),
    });
    return highlighter;
  };

  const ensureLanguage = (h: HighlighterCore, lang: HighlightLanguage): Promise<void> => {
    let p = loaded.get(lang);
    if (p === undefined) {
      p = LOADERS[lang]().then(async (mod) => {
        const langs = withoutExcludedGrammars((mod as { default: LanguageRegistration[] }).default);
        // An own grammar can embed a language its module doesn't bring along (the own glsl
        // grammar is built on C, #326): load that one first.
        const missing = langs
          .flatMap((l) => l.embeddedLangs ?? [])
          .filter((name): name is HighlightLanguage => name in LOADERS && !langs.some((l) => l.name === name));
        await Promise.all([...new Set(missing)].map((name) => ensureLanguage(h, name)));
        await h.loadLanguage(langs);
      });
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
