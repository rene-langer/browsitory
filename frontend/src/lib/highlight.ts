import { startTransition, useEffect, useState } from "react";
import type { DiffHunk, DiffLine } from "../ipc/RepoClient";
import { inProcessTransport } from "./highlightInProcess";
import { abortError, isAbortError, type HighlightTransport } from "./highlightWorkerProtocol";
import type { HighlightLanguage } from "./languageForPath";

export { withoutExcludedGrammars } from "./excludedGrammars";

export interface Token {
  content: string;
  color: string | undefined;
}

export const HIGHLIGHT_LINE_LIMIT = 2000;

/**
 * Where tokenizing runs (#843): the app's entry points hand in a Web Worker
 * (`setHighlightTransport(workerTransport(...))`); until then, and in tests, it runs on the page
 * in chunks (`highlightInProcess.ts`).
 */
let transport: HighlightTransport = inProcessTransport();

/** Sets where tokenizing runs; returns the previous transport (tests restore it). */
export function setHighlightTransport(next: HighlightTransport): HighlightTransport {
  const previous = transport;
  transport = next;
  return previous;
}

/** The grammars registered so far (tests check that excluded ones never are). */
export function loadedLanguages(): Promise<string[]> {
  return transport.loadedLanguages();
}

const cache = new Map<string, Token[][]>();
const CACHE_LIMIT = 500;

/** A request in flight, shared by every caller asking for the same text until all of them abort. */
interface Job {
  promise: Promise<Token[][]>;
  controller: AbortController;
  waiters: number;
}
const inflight = new Map<string, Job>();

/**
 * The tokens of `code`, line by line. `signal` cancels this caller's interest: the promise
 * rejects with an AbortError, and the work itself stops once no caller wants it any more.
 */
export function highlightLines(lang: HighlightLanguage, code: string, signal?: AbortSignal): Promise<Token[][]> {
  const key = `${lang}\u0000${code}`;
  const hit = cache.get(key);
  if (hit !== undefined) return Promise.resolve(hit);
  if (signal?.aborted) return Promise.reject(abortError());
  let job = inflight.get(key);
  if (job === undefined) {
    const controller = new AbortController();
    const created: Job = {
      controller,
      waiters: 0,
      promise: transport.tokenize(lang, code, controller.signal).then((tokens) => {
        cache.set(key, tokens);
        if (cache.size > CACHE_LIMIT) cache.delete(cache.keys().next().value!);
        return tokens;
      }),
    };
    const forget = () => {
      if (inflight.get(key) === created) inflight.delete(key);
    };
    created.promise.then(forget, forget);
    inflight.set(key, created);
    job = created;
  }
  const shared = job;
  shared.waiters += 1;
  return new Promise<Token[][]>((resolve, reject) => {
    let settled = false;
    const leave = () => {
      settled = true;
      shared.waiters -= 1;
      signal?.removeEventListener("abort", onAbort);
    };
    const onAbort = () => {
      if (settled) return;
      leave();
      reject(abortError());
      if (shared.waiters === 0) {
        shared.controller.abort();
        if (inflight.get(key) === shared) inflight.delete(key);
      }
    };
    signal?.addEventListener("abort", onAbort, { once: true });
    shared.promise.then(
      (tokens) => {
        if (settled) return;
        leave();
        resolve(tokens);
      },
      (error: unknown) => {
        if (settled) return;
        leave();
        reject(error);
      },
    );
  });
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
    const controller = new AbortController();
    (async () => {
      const map = new Map<DiffLine, Token[]>();
      for (const hunk of hunks) {
        const oldSide = hunk.lines.filter((l) => !isEofMarker(l) && l.origin !== "Add");
        const newSide = hunk.lines.filter((l) => !isEofMarker(l) && l.origin !== "Remove");
        const [oldTokens, newTokens] = await Promise.all([
          highlightLines(lang, oldSide.map((l) => stripEol(l.content)).join("\n"), controller.signal),
          highlightLines(lang, newSide.map((l) => stripEol(l.content)).join("\n"), controller.signal),
        ]);
        oldSide.forEach((l, i) => l.origin === "Remove" && map.set(l, oldTokens[i] ?? []));
        newSide.forEach((l, i) => map.set(l, newTokens[i] ?? []));
      }
      // A transition: rendering a long file's tokens yields to input instead of blocking it.
      if (!cancelled) startTransition(() => setTokens(map));
    })().catch((e: unknown) => {
      if (!cancelled && !isAbortError(e)) onError?.(e);
    });
    return () => {
      cancelled = true;
      controller.abort();
    };
  }, [hunks, lang, enabled, onError]);
  return tokens;
}
