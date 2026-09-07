export type BranchTreeNode<T> =
  | { kind: "folder"; name: string; path: string; children: BranchTreeNode<T>[] }
  | { kind: "leaf"; name: string; path: string; value: T };

type FolderBuilder<T> = { children: Map<string, FolderBuilder<T> | { value: T }> };

function isFolderBuilder<T>(node: FolderBuilder<T> | { value: T }): node is FolderBuilder<T> {
  return "children" in node;
}

// Splits each item's `path` on "/" into nested folders — e.g. "feat/foo" and "feat/bar" share a
// "feat" folder — mirroring how a filesystem tree view groups slash-delimited names.
export function buildBranchTree<T>(items: { path: string; value: T }[]): BranchTreeNode<T>[] {
  const root: FolderBuilder<T> = { children: new Map() };

  for (const { path, value } of items) {
    const segments = path.split("/");
    let node = root;
    for (let i = 0; i < segments.length - 1; i += 1) {
      const segment = segments[i];
      const existing = node.children.get(segment);
      const next = existing !== undefined && isFolderBuilder(existing) ? existing : { children: new Map() };
      node.children.set(segment, next);
      node = next;
    }
    node.children.set(segments[segments.length - 1], { value });
  }

  return toNodes(root, "");
}

function toNodes<T>(builder: FolderBuilder<T>, prefix: string): BranchTreeNode<T>[] {
  const folders: BranchTreeNode<T>[] = [];
  const leaves: BranchTreeNode<T>[] = [];

  for (const [name, child] of builder.children) {
    const path = prefix === "" ? name : `${prefix}/${name}`;
    if (isFolderBuilder(child)) {
      folders.push({ kind: "folder", name, path, children: toNodes(child, path) });
    } else {
      leaves.push({ kind: "leaf", name, path, value: child.value });
    }
  }

  folders.sort((a, b) => a.name.localeCompare(b.name));
  leaves.sort((a, b) => a.name.localeCompare(b.name));
  return [...folders, ...leaves];
}
