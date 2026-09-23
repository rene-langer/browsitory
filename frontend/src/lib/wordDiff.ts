export interface Segment {
  text: string;
  changed: boolean;
}

// Splits on word boundaries but keeps the separators (spaces, punctuation) as their own tokens,
// so re-joining `tokens.join("")` reconstructs the original line exactly.
function tokenize(line: string): string[] {
  return line.match(/\w+|\s+|[^\w\s]/g) ?? [];
}

/**
 * Upper bound on the LCS table's size (old tokens × new tokens) before `wordDiff` gives up and
 * marks both lines wholly changed. The table is O(n·m) in both time and memory, and a single
 * minified-JS or lockfile line can run to thousands of tokens — millions of cells, computed on
 * the webview's main thread. 40,000 cells is roughly a 200×200-token pair (a token is a word, a
 * whitespace run or one punctuation character, so a typical 100-character code line is ~40–60
 * tokens, and a long prose/Markdown line ~150): enough that ordinary code and prose lines keep
 * their word-level highlight, while one pair's worst case stays well under a millisecond and a
 * few hundred KB.
 */
export const MAX_LCS_CELLS = 40_000;

/**
 * Token-level LCS diff between two single lines. Used to highlight just the changed words in a
 * paired Remove/Add line, rather than marking the whole line as changed.
 *
 * When the two lines share no common tokens at all, the LCS degenerates to zero and every token
 * on both sides comes back marked `changed` — the whole line lights up, which is the reasonable
 * fallback for lines with no common structure. Lines too long to diff within `MAX_LCS_CELLS` take
 * that same fallback without building the table.
 */
export function wordDiff(oldText: string, newText: string): { oldSegments: Segment[]; newSegments: Segment[] } {
  const oldTokens = tokenize(oldText);
  const newTokens = tokenize(newText);
  const oldSegments: Segment[] = [];
  const newSegments: Segment[] = [];

  if (oldTokens.length * newTokens.length > MAX_LCS_CELLS) {
    pushRemaining(oldSegments, oldTokens, 0);
    pushRemaining(newSegments, newTokens, 0);
    return { oldSegments, newSegments };
  }

  // Standard LCS table over tokens (not characters), bounded by `MAX_LCS_CELLS` above.
  const lcs: number[][] = Array.from({ length: oldTokens.length + 1 }, () => new Array(newTokens.length + 1).fill(0));
  for (let i = oldTokens.length - 1; i >= 0; i--) {
    for (let j = newTokens.length - 1; j >= 0; j--) {
      lcs[i][j] = oldTokens[i] === newTokens[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }

  let i = 0;
  let j = 0;
  while (i < oldTokens.length && j < newTokens.length) {
    if (oldTokens[i] === newTokens[j]) {
      pushSegment(oldSegments, oldTokens[i], false);
      pushSegment(newSegments, newTokens[j], false);
      i++;
      j++;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      pushSegment(oldSegments, oldTokens[i], true);
      i++;
    } else {
      pushSegment(newSegments, newTokens[j], true);
      j++;
    }
  }
  pushRemaining(oldSegments, oldTokens, i);
  pushRemaining(newSegments, newTokens, j);
  return { oldSegments, newSegments };
}

/** Marks every token from `start` onward as changed — the tail of a diff, or the whole line. */
function pushRemaining(segments: Segment[], tokens: string[], start: number) {
  for (let k = start; k < tokens.length; k++) {
    pushSegment(segments, tokens[k], true);
  }
}

function pushSegment(segments: Segment[], text: string, changed: boolean) {
  const last = segments[segments.length - 1];
  if (last !== undefined && last.changed === changed) {
    last.text += text;
  } else {
    segments.push({ text, changed });
  }
}
