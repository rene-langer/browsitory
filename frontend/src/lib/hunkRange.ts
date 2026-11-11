export interface HunkBounds {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
}

export function hunkRange(_hunk: HunkBounds): { label: string; title: string } {
  throw new Error("not implemented");
}
