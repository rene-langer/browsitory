import { useEffect, useState } from "react";
import { createHighlighterCore, createCssVariablesTheme, type HighlighterCore } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";
import type { DiffHunk, DiffLine } from "../ipc/RepoClient";
import type { HighlightLanguage } from "./languageForPath";

export interface Token {
  content: string;
  color: string | undefined;
}

export const HIGHLIGHT_LINE_LIMIT = 2000;
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

let highlighter: Promise<HighlighterCore> | null = null;
const loaded = new Map<HighlightLanguage, Promise<void>>();
const cache = new Map<string, Promise<Token[][]>>();
const CACHE_LIMIT = 500;

function getHighlighter(): Promise<HighlighterCore> {
  highlighter ??= createHighlighterCore({ themes: [THEME], langs: [], engine: createJavaScriptRegexEngine() });
  return highlighter;
}

async function ensureLanguage(h: HighlighterCore, lang: HighlightLanguage): Promise<void> {
  let p = loaded.get(lang);
  if (p === undefined) {
    p = LOADERS[lang]().then((mod) =>
      h.loadLanguage((mod as { default: Parameters<HighlighterCore["loadLanguage"]>[0] }).default),
    );
    loaded.set(lang, p);
  }
  return p;
}

export function highlightLines(lang: HighlightLanguage, code: string): Promise<Token[][]> {
  const key = `${lang}\u0000${code}`;
  const hit = cache.get(key);
  if (hit !== undefined) return hit;
  const result = (async () => {
    const h = await getHighlighter();
    await ensureLanguage(h, lang);
    const { tokens } = h.codeToTokens(code, { lang, theme: "browsitory" });
    return tokens.map((line) => line.map((t) => ({ content: t.content, color: t.color })));
  })();
  result.catch(() => cache.delete(key));
  cache.set(key, result);
  if (cache.size > CACHE_LIMIT) cache.delete(cache.keys().next().value!);
  return result;
}

const stripEol = (s: string) => s.replace(/\r?\n$/, "");
const isEofMarker = (l: DiffLine) => l.origin === "Context" && l.content.includes("\\ No newline at end of file");

/**
 * Tokenizes each hunk's old side (context + removed) and new side (context + added) as two
 * continuous blocks, so multi-line strings/comments inside a hunk come out right, then maps the
 * tokens back onto the hunk's lines. Returns null while loading, when disabled, over the line
 * cap, for unknown languages, or on failure (callers render plain text).
 */
export function useHunkTokens(
  hunks: DiffHunk[] | null,
  lang: HighlightLanguage | null,
  enabled: boolean,
  onError?: (e: unknown) => void,
): Map<DiffLine, Token[]> | null {
  const [tokens, setTokens] = useState<Map<DiffLine, Token[]> | null>(null);
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setTokens(null);
    if (!enabled || lang === null || hunks === null) return;
    const total = hunks.reduce((n, h) => n + h.lines.length, 0);
    if (total > HIGHLIGHT_LINE_LIMIT) return;
    let cancelled = false;
    (async () => {
      const map = new Map<DiffLine, Token[]>();
      for (const hunk of hunks) {
        const oldSide = hunk.lines.filter((l) => !isEofMarker(l) && l.origin !== "Add");
        const newSide = hunk.lines.filter((l) => !isEofMarker(l) && l.origin !== "Remove");
        const [oldTokens, newTokens] = await Promise.all([
          highlightLines(lang, oldSide.map((l) => stripEol(l.content)).join("\n")),
          highlightLines(lang, newSide.map((l) => stripEol(l.content)).join("\n")),
        ]);
        oldSide.forEach((l, i) => l.origin === "Remove" && map.set(l, oldTokens[i] ?? []));
        newSide.forEach((l, i) => map.set(l, newTokens[i] ?? []));
      }
      if (!cancelled) setTokens(map);
    })().catch((e: unknown) => {
      if (!cancelled) onError?.(e);
    });
    return () => {
      cancelled = true;
    };
  }, [hunks, lang, enabled, onError]);
  return tokens;
}
