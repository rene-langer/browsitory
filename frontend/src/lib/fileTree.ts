import { buildBranchTree, type BranchTreeNode } from "./branchTree";

/**
 * The working tree's file list as folders (#729): the branch tree's grouping by `/` (folders
 * first, then files, each by name), with a chain of folders that each hold one folder and nothing
 * else shown as one ("src/lib"), so deep paths don't cost a row per level.
 */
export type FileTreeNode<T> = BranchTreeNode<T>;

export function buildFileTree<T>(items: { path: string; value: T }[]): FileTreeNode<T>[] {
  return buildBranchTree(items).map(compact);
}

function compact<T>(node: FileTreeNode<T>): FileTreeNode<T> {
  if (node.kind === "leaf") return node;
  let folder = node;
  let name = node.name;
  while (folder.children.length === 1 && folder.children[0].kind === "folder") {
    folder = folder.children[0];
    name = `${name}/${folder.name}`;
  }
  return { kind: "folder", name, path: folder.path, children: folder.children.map(compact) };
}

/** Every file in `nodes`, in tree order, leaving out the contents of collapsed folders. */
export function visibleFiles<T>(nodes: FileTreeNode<T>[], isCollapsed: (path: string) => boolean): T[] {
  return nodes.flatMap((node) => {
    if (node.kind === "leaf") return [node.value];
    return isCollapsed(node.path) ? [] : visibleFiles(node.children, isCollapsed);
  });
}

/** Every file under `node`, collapsed or not. */
export function filesUnder<T>(node: FileTreeNode<T>): T[] {
  return node.kind === "leaf" ? [node.value] : node.children.flatMap(filesUnder);
}
