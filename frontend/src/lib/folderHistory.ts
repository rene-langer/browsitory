/** The folder `path` is in, without a trailing slash, or `null` at the repository root. */
export function folderOf(path: string): string | null {
  const slash = path.lastIndexOf("/");
  return slash > 0 ? path.slice(0, slash) : null;
}

/** A `path:` search term for everything under `folder` (#447), quoted when it needs to be. */
export function folderSearchQuery(folder: string): string {
  const value = `${folder}/`;
  return /[\s"\\]/.test(value) ? `path:"${value.replace(/[\\"]/g, (c) => `\\${c}`)}"` : `path:${value}`;
}
