import type { ReactNode } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import { CommitMessageSettingsContext, useCommitMessageSettings } from "./useCommitMessageSettings";

export function CommitMessageSettingsProvider({ client, children }: { client: RepoClient; children: ReactNode }) {
  const api = useCommitMessageSettings(client);
  return <CommitMessageSettingsContext.Provider value={api}>{children}</CommitMessageSettingsContext.Provider>;
}
