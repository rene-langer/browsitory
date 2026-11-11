import * as path from "node:path";

/**
 * What the extension host asks the webview to do (#677, #678, #679, #759), sent as a `hostRequest`
 * notification. The frontend's `RepoClient.HostRequest` is the same shape; the extension never
 * imports from `frontend/`, so it is duplicated here (as `toolUris.ts` does with `DiffSource`).
 */
export type HostRequest =
  /** Opens each repository as a tab (or keeps its tab) and shows the first one. */
  | { kind: "openRepositories"; paths: string[] }
  /**
   * Opens `repoPath`'s tab and shows the history of `path` (relative to the work tree, with "/"),
   * or of its lines `startLine`..`endLine` as they are in HEAD.
   */
  | { kind: "showHistory"; repoPath: string; path: string; lines?: { startLine: number; endLine: number } }
  /** Opens `repoPath`'s tab and selects commit `commitId` in the graph, as Go to does (#759). */
  | { kind: "showCommit"; repoPath: string; commitId: string }
  /** A global shortcut VSCode would otherwise take, pressed in the Browsitory panel. */
  | { kind: "shortcut"; id: string; index?: number };

/** Whether a path exists; `.git` is a folder in a repository and a file in a worktree or submodule. */
export type PathExists = (target: string) => Promise<boolean>;

/** The work tree root of the repository that holds `target` (a file or folder), if any. */
export async function findRepositoryRoot(target: string, exists: PathExists): Promise<string | undefined> {
  let current = path.resolve(target);
  for (;;) {
    if (await exists(path.join(current, ".git"))) return current;
    const parent = path.dirname(current);
    if (parent === current) return undefined;
    current = parent;
  }
}

/** Lists the names of the folders directly in `folder`. */
export type ListSubfolders = (folder: string) => Promise<string[]>;

/**
 * The repositories "Browsitory: Open" offers for the workspace (#759): each folder's repository,
 * then the repositories directly in its subfolders (one level down, so a folder of checkouts
 * and a repository nested in another are found without walking the whole tree). Each once.
 */
export async function repositoryCandidates(
  folders: readonly string[],
  listSubfolders: ListSubfolders,
  exists: PathExists,
): Promise<string[]> {
  const found: string[] = [];
  const add = (root: string) => {
    if (!found.includes(root)) found.push(root);
  };
  for (const folder of folders) {
    const root = await findRepositoryRoot(folder, exists);
    if (root !== undefined) add(root);
    const names = (await listSubfolders(folder)).filter((name) => !name.startsWith(".")).sort();
    for (const name of names) {
      const nested = path.join(folder, name);
      if (await exists(path.join(nested, ".git"))) add(nested);
    }
  }
  return found;
}

/**
 * The commit of an item VSCode's Timeline view passes (#759): the Git timeline's commits carry
 * their id as `ref` (and `id`); its uncommitted entries ("~", "") and other sources' items don't.
 */
export function timelineCommitId(item: unknown): string | undefined {
  if (typeof item !== "object" || item === null) return undefined;
  const { ref, id } = item as { ref?: unknown; id?: unknown };
  const candidate = typeof ref === "string" ? ref : id;
  return typeof candidate === "string" && /^[0-9a-f]{7,64}$/i.test(candidate) ? candidate : undefined;
}

/** `file` relative to `root`, written with "/" as the repository stores it. */
export function repoRelativePath(root: string, file: string): string {
  return path.relative(root, file).split(path.sep).join("/");
}

/** Each path's repository once, in the order given; paths outside any repository are skipped. */
export async function repositoryRoots(targets: readonly string[], exists: PathExists): Promise<string[]> {
  const roots: string[] = [];
  for (const target of targets) {
    const root = await findRepositoryRoot(target, exists);
    if (root !== undefined && !roots.includes(root)) roots.push(root);
  }
  return roots;
}

/**
 * The file system path a command was invoked on: an Explorer or editor `Uri`, or a Source Control
 * provider (the SCM title bar passes one, with its `rootUri`). Only `file:` resources count.
 */
export function commandTargetPath(argument: unknown): string | undefined {
  if (typeof argument !== "object" || argument === null) return undefined;
  const candidate = "rootUri" in argument ? (argument as { rootUri?: unknown }).rootUri : argument;
  if (typeof candidate !== "object" || candidate === null) return undefined;
  const uri = candidate as { scheme?: unknown; fsPath?: unknown };
  return uri.scheme === "file" && typeof uri.fsPath === "string" ? uri.fsPath : undefined;
}

/** The shortcut commands' prefix: `browsitory.shortcut.<id>` runs the global shortcut `<id>`. */
export const SHORTCUT_COMMAND_PREFIX = "browsitory.shortcut.";

/** The ids of the shortcut commands `package.json` contributes (#679). */
export function contributedShortcutIds(packageJson: unknown): string[] {
  const commands = (packageJson as { contributes?: { commands?: unknown } } | undefined)?.contributes?.commands;
  if (!Array.isArray(commands)) return [];
  return commands
    .map((entry: unknown) => (entry as { command?: unknown }).command)
    .filter((command): command is string => typeof command === "string" && command.startsWith(SHORTCUT_COMMAND_PREFIX))
    .map((command) => command.slice(SHORTCUT_COMMAND_PREFIX.length));
}

/** The shortcut request a contributed keybinding's command sends; `args.index` picks a tab. */
export function shortcutRequest(id: string, args: unknown): HostRequest {
  const index = (args as { index?: unknown } | undefined)?.index;
  return typeof index === "number" ? { kind: "shortcut", id, index } : { kind: "shortcut", id };
}

/** The editor selection's lines, 1-based; a selection ending at a line's start leaves that line out. */
export function selectedLines(selection: {
  start: { line: number };
  end: { line: number; character: number };
}): { startLine: number; endLine: number } {
  const startLine = selection.start.line + 1;
  const lastLine = selection.end.character === 0 && selection.end.line > selection.start.line ? selection.end.line - 1 : selection.end.line;
  return { startLine, endLine: lastLine + 1 };
}
