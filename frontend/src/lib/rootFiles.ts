import { ROOT_FILE_NAMES, type RepoClient, type RootFileName } from "../ipc/RepoClient";

/** "Open .gitignore" or, while it is missing, "Create .gitignore" (#624). */
export interface RootFileItem {
  name: RootFileName;
  exists: boolean;
  label: string;
}

/**
 * The tab menu's and the palette's items for `existing` (`RepoClient.getRootFiles`): none for a
 * bare repository (`null`) or before the answer arrived (`undefined`).
 */
export function rootFileItems(existing: readonly RootFileName[] | null | undefined): RootFileItem[] {
  if (existing == null) return [];
  return ROOT_FILE_NAMES.map((name) => {
    const exists = existing.includes(name);
    return { name, exists, label: `${exists ? "Open" : "Create"} ${name}` };
  });
}

/**
 * Opens the work tree root's `name` in the editor, creating it empty first when it was missing.
 * Creating leaves a file that appeared in the meantime as it is.
 */
export async function openRootFile(client: RepoClient, repoPath: string, item: Pick<RootFileItem, "name" | "exists">): Promise<void> {
  if (!item.exists) await client.createRootFile(repoPath, item.name);
  await client.openInEditor(repoPath, item.name, null);
}
