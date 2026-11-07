/**
 * Whether `selection` names every one of `branchNames`: the default graph, which callers
 * restore by clearing the saved selection rather than saving the names (#402). A saved list
 * would hide branches created later, and with one local branch walk first parents only.
 */
export function coversEveryBranch(selection: string[], branchNames: string[]): boolean {
  return branchNames.every((name) => selection.includes(name));
}

/**
 * The persisted graph branch selection after deleting `name`, or `null` when it needs no write.
 * An emptied selection becomes `remaining` (every local branch left, i.e. "show all") so
 * isolating a branch and then deleting it never leaves an empty graph (#97); the caller then
 * clears the selection (`coversEveryBranch`, #402).
 */
export function withoutBranch(selection: string[] | null, name: string, remaining: string[]): string[] | null {
  if (selection === null || !selection.includes(name)) return null;
  const next = selection.filter((candidate) => candidate !== name);
  return next.length > 0 ? next : remaining;
}

/** The selection after renaming `oldName`, or `null` when it needs no write. */
export function withRenamedBranch(selection: string[] | null, oldName: string, newName: string): string[] | null {
  if (selection === null || !selection.includes(oldName)) return null;
  return selection.map((candidate) => (candidate === oldName ? newName : candidate));
}
