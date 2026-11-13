import type { RepoClient } from "../ipc/RepoClient";

/** The operation log panel (#889). Not built yet. */
export function OperationLogDialog(_props: {
  client: RepoClient;
  repoPath: string;
  focus: "latest" | "latestFailed";
  onClose: () => void;
}) {
  return null;
}
