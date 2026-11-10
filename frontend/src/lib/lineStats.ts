// Text of the lines-changed numbers (#620), shared by the commit list's badge and the commit
// details.

/** `1234` as is; from 10 000 on, rounded to thousands (`12k`), so a badge stays narrow. */
export function compactCount(count: number): string {
  return count < 10_000 ? String(count) : `${Math.round(count / 1000)}k`;
}

/** "12 lines added, 3 removed": the exact numbers, for the accessible name and the tooltip. */
export function lineStatsLabel(insertions: number, deletions: number): string {
  return `${insertions} ${insertions === 1 ? "line" : "lines"} added, ${deletions} removed`;
}

/** "4 files changed" / "1 file changed". */
export function filesChangedLabel(files: number): string {
  return `${files} ${files === 1 ? "file" : "files"} changed`;
}
