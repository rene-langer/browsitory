/**
 * The part of VSCode's Git extension API (`extensions/git/src/api/git.d.ts`, `getAPI(1)`) the
 * commit hover link uses (#781). The extension doesn't depend on the Git extension's typings, so
 * the shapes are declared here.
 */
export interface GitExtensionExports {
  readonly enabled: boolean;
  readonly onDidChangeEnablement: (listener: (enabled: boolean) => void) => { dispose(): unknown };
  getAPI(version: 1): GitApi;
}

interface GitApi {
  registerSourceControlHistoryItemDetailsProvider?: (provider: HistoryItemDetailsProvider) => { dispose(): unknown };
}

interface HoverCommand {
  command: string;
  title: string;
  tooltip: string;
  arguments: unknown[];
}

interface HistoryItemDetailsProvider {
  provideAvatar(repository: unknown, query: unknown): Promise<undefined>;
  provideHoverCommands(repository: unknown): Promise<HoverCommand[] | undefined>;
  provideMessageLinks(repository: unknown, message: string): Promise<undefined>;
}

/** VSCode's built-in Git extension. */
export const GIT_EXTENSION_ID = "vscode.git";

/** The command the hover link runs, with the repository's root and the commit's hash. */
export const HOVER_COMMIT_COMMAND = "browsitory.showHoverCommit";

/**
 * Adds "Show in Browsitory" to the commit hovers of the Git extension (inline blame, Timeline,
 * Source Control Graph). The Git extension appends the commit's hash to the arguments. It shows
 * only the first provider's commands that returns any, and none for unpublished commits.
 */
export const hoverCommitProvider: HistoryItemDetailsProvider = {
  provideAvatar: async () => undefined,
  provideMessageLinks: async () => undefined,
  provideHoverCommands: async (repository) => {
    const root = (repository as { rootUri?: { scheme?: unknown; fsPath?: unknown } } | undefined)?.rootUri;
    if (root?.scheme !== "file" || typeof root.fsPath !== "string") return undefined;
    return [
      {
        command: HOVER_COMMIT_COMMAND,
        title: "Show in Browsitory",
        tooltip: "Show Commit in Browsitory",
        arguments: [root.fsPath],
      },
    ];
  },
};

/** The repository root and commit a hover link passes, when they are what the provider sent. */
export function hoverCommitTarget(root: unknown, hash: unknown): { repoPath: string; commitId: string } | undefined {
  if (typeof root !== "string" || root === "" || typeof hash !== "string") return undefined;
  return /^[0-9a-f]{7,64}$/i.test(hash) ? { repoPath: root, commitId: hash } : undefined;
}

/**
 * Registers `hoverCommitProvider` with the Git extension once it is enabled; the returned
 * function removes it (or stops waiting). Without the Git extension or the API method nothing
 * is registered.
 */
export function registerHoverCommits(
  git: GitExtensionExports,
  onError: (error: unknown) => void,
): () => void {
  let registration: { dispose(): unknown } | undefined;
  let stopped = false;
  const register = () => {
    if (stopped || registration !== undefined || !git.enabled) return;
    try {
      const api = git.getAPI(1);
      registration = api.registerSourceControlHistoryItemDetailsProvider?.(hoverCommitProvider);
    } catch (error) {
      onError(error);
    }
  };
  // Disabling Git drops its model and the providers registered with it.
  const enablement = git.onDidChangeEnablement((enabled) => {
    if (enabled) {
      register();
      return;
    }
    registration?.dispose();
    registration = undefined;
  });
  register();
  return () => {
    stopped = true;
    enablement.dispose();
    registration?.dispose();
    registration = undefined;
  };
}
