import { useEffect, useRef } from "react";
import type { HostRequest, RepoClient } from "../ipc/RepoClient";
import { requestCommit, requestHistory, requestWorkspace } from "../lib/workspaceRequests";

/**
 * Carries out the VSCode extension host's requests (#677, #678, #679, #759): opening repositories
 * as tabs, a file's or lines' history or a commit in a repository's tab, and global shortcuts
 * VSCode took first.
 * Subscribes once `ready` (the restored tabs are open), so a request's tab isn't replaced by the
 * restore; the host holds its requests until then.
 */
export function useHostRequests(client: RepoClient, ready: boolean, openRepoTab: (path: string) => Promise<unknown>): void {
  const openRef = useRef(openRepoTab);
  useEffect(() => {
    openRef.current = openRepoTab;
  });

  useEffect(() => {
    if (!ready) return;
    // One request after the other: each opens tabs the next may switch away from.
    let queue: Promise<unknown> = Promise.resolve();
    const handle = async (request: HostRequest) => {
      const open = openRef.current;
      switch (request.kind) {
        case "openRepositories":
          // The first one ends up active.
          for (const path of [...request.paths].reverse()) await open(path);
          return;
        case "showHistory":
          requestHistory({ repoPath: request.repoPath, path: request.path, lines: request.lines });
          await open(request.repoPath);
          return;
        case "showCommit":
          requestCommit({ repoPath: request.repoPath, commitId: request.commitId });
          await open(request.repoPath);
          return;
        case "shortcut":
          requestWorkspace({ kind: "shortcut", id: request.id, index: request.index });
          return;
      }
    };
    return client.subscribeHostRequests((request) => {
      queue = queue.then(() => handle(request)).catch(() => {});
    });
  }, [client, ready]);
}
