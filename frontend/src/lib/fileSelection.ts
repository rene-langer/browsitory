// Which changed files are selected in DiffPane's working-tree lists (#449). A selection lives in
// one group only (`staged`), since every action on it (stage, unstage, discard, …) is
// group-specific. `anchor` is the pivot for Shift-extend; it is always a member of `paths` unless
// `paths` is empty.
export interface FileSelection {
  staged: boolean;
  paths: ReadonlySet<string>;
  anchor: string | null;
}

export const EMPTY_FILE_SELECTION: FileSelection = { staged: false, paths: new Set(), anchor: null };

export function selectOnlyFile(staged: boolean, path: string): FileSelection {
  return { staged, paths: new Set([path]), anchor: path };
}

/** Ctrl/Cmd+click: adds or removes `path`; a file of the other group starts a new selection. */
export function toggleFile(selection: FileSelection, staged: boolean, path: string): FileSelection {
  if (selection.staged !== staged || selection.paths.size === 0) return selectOnlyFile(staged, path);
  const paths = new Set(selection.paths);
  if (paths.has(path)) {
    paths.delete(path);
    if (paths.size === 0) return EMPTY_FILE_SELECTION;
    const anchor = selection.anchor !== null && paths.has(selection.anchor) ? selection.anchor : [...paths][0];
    return { staged, paths, anchor };
  }
  paths.add(path);
  return { staged, paths, anchor: path };
}

/**
 * Shift+click or Shift+arrow: every file between the anchor and `path` (inclusive), in list order
 * (`groupPaths`). Without a usable anchor in the same group it degrades to a single selection.
 */
export function extendFileSelection(
  selection: FileSelection,
  staged: boolean,
  groupPaths: readonly string[],
  path: string,
): FileSelection {
  if (selection.staged !== staged || selection.anchor === null) return selectOnlyFile(staged, path);
  const anchorIndex = groupPaths.indexOf(selection.anchor);
  const targetIndex = groupPaths.indexOf(path);
  if (anchorIndex === -1 || targetIndex === -1) return selectOnlyFile(staged, path);
  const start = Math.min(anchorIndex, targetIndex);
  const end = Math.max(anchorIndex, targetIndex);
  return { staged, paths: new Set(groupPaths.slice(start, end + 1)), anchor: selection.anchor };
}

/** Ctrl/Cmd+A: the whole group, keeping the anchor when it is in the group. */
export function selectAllFiles(selection: FileSelection, staged: boolean, groupPaths: readonly string[]): FileSelection {
  if (groupPaths.length === 0) return EMPTY_FILE_SELECTION;
  const anchor =
    selection.staged === staged && selection.anchor !== null && groupPaths.includes(selection.anchor)
      ? selection.anchor
      : groupPaths[0];
  return { staged, paths: new Set(groupPaths), anchor };
}

/** The selected paths that still exist in the group, in list order. */
export function selectedPaths(selection: FileSelection, staged: boolean, groupPaths: readonly string[]): string[] {
  if (selection.staged !== staged) return [];
  return groupPaths.filter((path) => selection.paths.has(path));
}
