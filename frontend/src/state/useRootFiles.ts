import { useEffect, useState } from "react";
import type { RepoClient, RootFileName } from "../ipc/RepoClient";

/**
 * Which of `.gitignore` and `.gitattributes` exist at the work tree root (#624), read again
 * whenever `refreshKey` changes (the workspace's `refreshGeneration`: after each mutation and
 * window focus). `null` for a bare repository; `undefined` until the first answer, and after a
 * failed read (the palette then offers neither command).
 */
export function useRootFiles(client: RepoClient, repoPath: string, refreshKey: number): RootFileName[] | null | undefined {
  const [files, setFiles] = useState<{ repoPath: string; files: RootFileName[] | null | undefined }>({
    repoPath,
    files: undefined,
  });
  useEffect(() => {
    let current = true;
    client.getRootFiles(repoPath).then(
      (next) => {
        if (current) setFiles({ repoPath, files: next });
      },
      () => {
        if (current) setFiles({ repoPath, files: undefined });
      },
    );
    return () => {
      current = false;
    };
  }, [client, repoPath, refreshKey]);
  return files.repoPath === repoPath ? files.files : undefined;
}
