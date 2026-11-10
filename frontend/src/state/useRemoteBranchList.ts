import { useEffect, useState } from "react";
import type { RemoteInfo, RepoClient } from "../ipc/RepoClient";
import { logFailure } from "../lib/logger";

export interface RemoteBranchName {
  remoteName: string;
  branchName: string;
}

/**
 * Every remote's branches (`listRemoteBranches`), read each time `enabled` turns on: the "Switch
 * branch…" picker lists the ones without a local branch (#726) while the palette is open. A
 * remote whose list can't be read is left out.
 */
export function useRemoteBranchList(
  client: RepoClient,
  repoPath: string,
  remotes: readonly RemoteInfo[],
  enabled: boolean,
): readonly RemoteBranchName[] {
  const [branches, setBranches] = useState<readonly RemoteBranchName[]>([]);
  const names = remotes.map((remote) => remote.name).join("\n");
  useEffect(() => {
    if (!enabled || names === "") return;
    let current = true;
    void Promise.all(
      names.split("\n").map((remoteName) =>
        client.listRemoteBranches(repoPath, remoteName).then(
          (list) => list.map((branchName) => ({ remoteName, branchName })),
          (error: unknown) => {
            logFailure(`Listing the branches of ${remoteName} failed`, error);
            return [];
          },
        ),
      ),
    ).then((lists) => {
      if (current) setBranches(lists.flat());
    });
    return () => {
      current = false;
    };
  }, [client, repoPath, names, enabled]);
  return branches;
}
