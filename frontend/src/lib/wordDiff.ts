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
 * Token-level LCS diff between two single lines. Used to highlight just the changed words in a
 * paired Remove/Add line, rather than marking the whole line as changed.
 *
 * When the two lines share no common tokens at all, the LCS degenerates to zero and every token
 * on both sides comes back marked `changed` — the whole line lights up, which is the reasonable
 * fallback for lines with no common structure.
 */
export function wordDiff(oldText: string, newText: string): { oldSegments: Segment[]; newSegments: Segment[] } {
  const oldTokens = tokenize(oldText);
  const newTokens = tokenize(newText);

  // Standard LCS table over tokens (not characters) — cheap for single-line inputs.
  const lcs: number[][] = Array.from({ length: oldTokens.length + 1 }, () => new Array(newTokens.length + 1).fill(0));
  for (let i = oldTokens.length - 1; i >= 0; i--) {
    for (let j = newTokens.length - 1; j >= 0; j--) {
      lcs[i][j] = oldTokens[i] === newTokens[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }

  const oldSegments: Segment[] = [];
  const newSegments: Segment[] = [];
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
  while (i < oldTokens.length) {
    pushSegment(oldSegments, oldTokens[i], true);
    i++;
  }
  while (j < newTokens.length) {
    pushSegment(newSegments, newTokens[j], true);
    j++;
  }
  return { oldSegments, newSegments };
}

function pushSegment(segments: Segment[], text: string, changed: boolean) {
  const last = segments[segments.length - 1];
  if (last !== undefined && last.changed === changed) {
    last.text += text;
  } else {
    segments.push({ text, changed });
  }
}
