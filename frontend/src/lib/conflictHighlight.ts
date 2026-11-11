import { useEffect, useState } from "react";
import type { ConflictSegment } from "../ipc/RepoClient";
import { HIGHLIGHT_LINE_LIMIT, highlightLines, type Token } from "./highlight";
import type { HighlightLanguage } from "./languageForPath";

/**
 * Syntax highlighting for the merge editor (#763), with the diff viewer's highlighter: each
 * side's version of the file (the unchanged runs plus that side's lines of every conflict) is
 * tokenized as one text, so a string or comment that spans a conflict comes out right, and the
 * tokens are mapped back onto the segments.
 */

export type ConflictSide = "ours" | "theirs" | "base";

const SIDES: readonly ConflictSide[] = ["ours", "theirs", "base"];

/** A segment's lines without their line ends; a final line end starts no further line. */
export function segmentLines(text: string): string[] {
  if (text === "") return [];
  const lines = text.split("\n").map((line) => line.replace(/\r$/, ""));
  if (text.endsWith("\n")) lines.pop();
  return lines;
}

export interface SideVersion {
  /** The side's version of the file, its lines joined with "\n". */
  text: string;
  /** The line each segment starts on in `text`, by segment index. */
  starts: number[];
}

function sideContent(segment: ConflictSegment, side: ConflictSide): string {
  return segment.kind === "Clean" ? segment.content : segment[side];
}

/** Ours', theirs' and base's version of the file. */
export function conflictVersions(segments: readonly ConflictSegment[]): Record<ConflictSide, SideVersion> {
  const version = (side: ConflictSide): SideVersion => {
    const lines: string[] = [];
    const starts: number[] = [];
    for (const segment of segments) {
      starts.push(lines.length);
      lines.push(...segmentLines(sideContent(segment, side)));
    }
    return { text: lines.join("\n"), starts };
  };
  return { ours: version("ours"), theirs: version("theirs"), base: version("base") };
}

/**
 * Each segment's tokens per line, by segment index: `ours` holds the unchanged runs' tokens too
 * (they are the same on every side); `theirs` and `base` only the conflicts'.
 */
export type ConflictTokens = Record<ConflictSide, (Token[][] | undefined)[]>;

export async function conflictTokens(
  segments: readonly ConflictSegment[],
  lang: HighlightLanguage,
): Promise<ConflictTokens> {
  const versions = conflictVersions(segments);
  const [ours, theirs, base] = await Promise.all(SIDES.map((side) => highlightLines(lang, versions[side].text)));
  const tokensBySide: Record<ConflictSide, Token[][]> = { ours, theirs, base };
  const result: ConflictTokens = { ours: [], theirs: [], base: [] };
  for (const side of SIDES) {
    segments.forEach((segment, index) => {
      if (segment.kind === "Clean" && side !== "ours") return;
      const start = versions[side].starts[index];
      const count = segmentLines(sideContent(segment, side)).length;
      result[side][index] = Array.from({ length: count }, (_, i) => tokensBySide[side][start + i] ?? []);
    });
  }
  return result;
}

/**
 * The merge editor's tokens; null while loading, when disabled, over the diff viewer's line cap,
 * for an unknown language or on failure (the editor then shows plain text).
 */
export function useConflictTokens(
  segments: readonly ConflictSegment[],
  lang: HighlightLanguage | null,
  enabled: boolean,
  onError?: (error: unknown) => void,
): ConflictTokens | null {
  const [state, setState] = useState<{ segments: readonly ConflictSegment[]; tokens: ConflictTokens } | null>(null);
  const tooLong = Object.values(conflictVersionsLineCounts(segments)).some((n) => n > HIGHLIGHT_LINE_LIMIT);
  const active = enabled && lang !== null && !tooLong && segments.length > 0;
  useEffect(() => {
    if (!active || lang === null) return;
    let cancelled = false;
    conflictTokens(segments, lang).then(
      (tokens) => {
        if (!cancelled) setState({ segments, tokens });
      },
      (error: unknown) => {
        if (!cancelled) onError?.(error);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [active, lang, segments, onError]);
  return active && state !== null && state.segments === segments ? state.tokens : null;
}

function conflictVersionsLineCounts(segments: readonly ConflictSegment[]): Record<ConflictSide, number> {
  const count = (side: ConflictSide) =>
    segments.reduce((n, segment) => n + segmentLines(sideContent(segment, side)).length, 0);
  return { ours: count("ours"), theirs: count("theirs"), base: count("base") };
}
