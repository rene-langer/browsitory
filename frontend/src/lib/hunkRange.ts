/** A hunk's line ranges, as git's `@@ -oldStart,oldLines +newStart,newLines @@` gives them. */
export interface HunkBounds {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
}

/** "line 7", "lines 12–19", or `null` for an empty side. */
function lines(start: number, count: number): string | null {
  if (count <= 0) return null;
  return count === 1 ? `line ${start}` : `lines ${start}–${start + count - 1}`;
}

const capitalized = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/**
 * A hunk header in words (#682): the new file's lines ("Lines 12–19"), or the removed ones for a
 * hunk that only removes; the tooltip names both sides.
 */
export function hunkRange(hunk: HunkBounds): { label: string; title: string } {
  const before = lines(hunk.oldStart, hunk.oldLines);
  const after = lines(hunk.newStart, hunk.newLines);
  const label = after !== null ? capitalized(after) : `Removed ${before ?? "lines"}`;
  return { label, title: `Before: ${before ?? "none"}. After: ${after ?? "none"}.` };
}
