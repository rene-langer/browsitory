/**
 * The command palette's matcher (#644): every word of the query must match the label or a
 * keyword, in any order. A word matches, best first, as the start of a label word, anywhere in
 * the label, as letters in order from the start of a label word ("ftch" → "Fetch"), or as the
 * start of (or anywhere in) a keyword. The whole label typed, or a label prefix, ranks above all.
 */

/** A half-open `[start, end)` span of the label to highlight. */
export type MatchRange = readonly [number, number];

export interface Match {
  /** Higher is better. */
  score: number;
  /** Merged, sorted label spans that matched; keyword-only matches add none. */
  ranges: MatchRange[];
}

const WHOLE_LABEL = 1000;
const LABEL_PREFIX = 900;
const WORD_START = 100;
const FIRST_WORD_BONUS = 20;
const SUBSTRING = 60;
const FUZZY = 40;
const FUZZY_MIN = 25;
const KEYWORD_PREFIX = 20;
const KEYWORD_SUBSTRING = 10;
const IN_ORDER_BONUS = 5;

interface TokenMatch {
  score: number;
  ranges: MatchRange[];
}

function isWordStart(text: string, index: number): boolean {
  return index === 0 || !/[\p{L}\p{N}]/u.test(text[index - 1]);
}

/** `token`'s letters in order in `label` from the word start at `start`, as ranges; or null. */
function fuzzyFrom(label: string, token: string, start: number): { gaps: number; ranges: MatchRange[] } | null {
  const ranges: [number, number][] = [];
  let position = start;
  for (const char of token) {
    const found = label.indexOf(char, position);
    if (found === -1) return null;
    const last = ranges[ranges.length - 1];
    if (last !== undefined && last[1] === found) last[1] = found + 1;
    else ranges.push([found, found + 1]);
    position = found + 1;
  }
  return { gaps: position - start - token.length, ranges };
}

function matchToken(label: string, keywords: string[], token: string): TokenMatch | null {
  let best: TokenMatch | null = null;
  const consider = (candidate: TokenMatch) => {
    if (best === null || candidate.score > best.score) best = candidate;
  };

  for (let index = label.indexOf(token); index !== -1; index = label.indexOf(token, index + 1)) {
    const range: MatchRange = [index, index + token.length];
    if (isWordStart(label, index)) {
      consider({ score: WORD_START + (index === 0 ? FIRST_WORD_BONUS : 0), ranges: [range] });
      break;
    }
    consider({ score: SUBSTRING, ranges: [range] });
  }
  if (best === null && token.length >= 2) {
    for (let index = 0; index < label.length; index++) {
      if (label[index] !== token[0] || !isWordStart(label, index)) continue;
      const fuzzy = fuzzyFrom(label, token, index);
      if (fuzzy !== null) consider({ score: Math.max(FUZZY_MIN, FUZZY - fuzzy.gaps), ranges: fuzzy.ranges });
    }
  }
  if (best === null) {
    for (const keyword of keywords) {
      const lower = keyword.toLowerCase();
      if (lower.startsWith(token)) consider({ score: KEYWORD_PREFIX, ranges: [] });
      else if (lower.includes(token)) consider({ score: KEYWORD_SUBSTRING, ranges: [] });
    }
  }
  return best;
}

/** How well `item` matches `query`, or null if some word of the query matches nothing. */
export function matchItem(item: { label: string; keywords: string[] }, query: string): Match | null {
  const q = query.trim().toLowerCase().replace(/\s+/g, " ");
  if (q === "") return { score: 0, ranges: [] };
  const label = item.label.toLowerCase();
  // The whole label typed ("go to branch" for "Go to branch…") beats a longer label that merely
  // starts with it ("Go to Branches").
  if (label === q || label.replace(/…$/, "") === q) return { score: WHOLE_LABEL, ranges: [[0, q.length]] };
  if (label.startsWith(q)) return { score: LABEL_PREFIX, ranges: [[0, q.length]] };

  let score = 0;
  let inOrder = true;
  let lastStart = -1;
  const ranges: MatchRange[] = [];
  for (const token of q.split(" ")) {
    const match = matchToken(label, item.keywords, token);
    if (match === null) return null;
    score += match.score;
    const first = match.ranges[0];
    if (first !== undefined) {
      if (first[0] < lastStart) inOrder = false;
      lastStart = first[0];
    }
    ranges.push(...match.ranges);
  }
  if (inOrder) score += IN_ORDER_BONUS;
  return { score, ranges: mergeRanges(ranges) };
}

/** `ranges` sorted, with overlapping or touching ones merged. */
export function mergeRanges(ranges: readonly MatchRange[]): MatchRange[] {
  const sorted = [...ranges].sort((a, b) => a[0] - b[0]);
  const merged: [number, number][] = [];
  for (const [start, end] of sorted) {
    const last = merged[merged.length - 1];
    if (last !== undefined && start <= last[1]) last[1] = Math.max(last[1], end);
    else merged.push([start, end]);
  }
  return merged;
}
