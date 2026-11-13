import { segmentLines } from "./conflictHighlight";
import { wordDiff, type Segment } from "./wordDiff";

/**
 * Upper bound on the line table's size (our lines × their lines) before `blockWordMarks` stops
 * matching the lines both sides share and pairs them by position instead. A block is usually a
 * few lines; this keeps a huge one off the main thread's back.
 */
export const MAX_BLOCK_LINE_CELLS = 40_000;

/** Each line's parts, the changed ones marked, or `undefined` for a line without marks. */
export interface BlockWordMarks {
  ours: (Segment[] | undefined)[];
  theirs: (Segment[] | undefined)[];
}

/**
 * The words that differ between a conflict block's ours and theirs (#879). The lines both sides
 * share are matched first (they stay unmarked); between them, our and their lines are paired in
 * order, as the diff viewer pairs removed and added lines, and each pair is diffed word by word.
 * A pair with no word in common differs as a whole and stays unmarked, as does a line the other
 * side has no partner for.
 */
export function blockWordMarks(ours: string, theirs: string): BlockWordMarks {
  const ourLines = segmentLines(ours);
  const theirLines = segmentLines(theirs);
  const marks: BlockWordMarks = {
    ours: Array<Segment[] | undefined>(ourLines.length).fill(undefined),
    theirs: Array<Segment[] | undefined>(theirLines.length).fill(undefined),
  };

  const pairGap = (ourFrom: number, ourTo: number, theirFrom: number, theirTo: number) => {
    const count = Math.min(ourTo - ourFrom, theirTo - theirFrom);
    for (let k = 0; k < count; k++) {
      const i = ourFrom + k;
      const j = theirFrom + k;
      const { oldSegments, newSegments } = wordDiff(ourLines[i], theirLines[j]);
      const shared = oldSegments.some((part) => !part.changed && part.text.trim() !== "");
      if (!shared) continue;
      marks.ours[i] = oldSegments;
      marks.theirs[j] = newSegments;
    }
  };

  if (ourLines.length * theirLines.length > MAX_BLOCK_LINE_CELLS) {
    pairGap(0, ourLines.length, 0, theirLines.length);
    return marks;
  }

  // Longest common subsequence of whole lines.
  const lcs: number[][] = Array.from({ length: ourLines.length + 1 }, () =>
    new Array<number>(theirLines.length + 1).fill(0),
  );
  for (let i = ourLines.length - 1; i >= 0; i--) {
    for (let j = theirLines.length - 1; j >= 0; j--) {
      lcs[i][j] = ourLines[i] === theirLines[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }
  let i = 0;
  let j = 0;
  let gapOurs = 0;
  let gapTheirs = 0;
  while (i < ourLines.length && j < theirLines.length) {
    if (ourLines[i] === theirLines[j]) {
      pairGap(gapOurs, i, gapTheirs, j);
      i++;
      j++;
      gapOurs = i;
      gapTheirs = j;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      i++;
    } else {
      j++;
    }
  }
  pairGap(gapOurs, ourLines.length, gapTheirs, theirLines.length);
  return marks;
}
