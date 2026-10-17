import { CloneError, type CloneOutcome, type RepoClient } from "../ipc/RepoClient";

export type NewRepositoryResult =
  | { kind: "created"; path: string }
  | { kind: "exists"; path: string }
  | { kind: "cancelled" };

/** Folder picker (the native dialog can create folders), then `git init` there. */
export async function createNewRepository(client: RepoClient): Promise<NewRepositoryResult> {
  const path = await client.pickRepoFolder();
  if (path === null) return { kind: "cancelled" };
  try {
    await client.initRepo(path);
    return { kind: "created", path };
  } catch (error) {
    if (error instanceof CloneError && error.kind === "AlreadyRepository") return { kind: "exists", path };
    throw error;
  }
}

export function cloneToastMessage(outcome: CloneOutcome, name: string): string {
  if (outcome.submoduleFailures.length > 0) {
    return `Cloned ${name} — ${outcome.submoduleFailures.length} submodules failed to update`;
  }
  if (outcome.credentialSaveFailed) return `Cloned ${name}, but the token couldn't be saved.`;
  return `Cloned ${name}`;
}
