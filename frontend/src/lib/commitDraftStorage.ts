import type { CommitDraft } from "../components/CommitBox";
import type { SignatureInput } from "../ipc/RepoClient";

// The commit message being written, per repository and per machine (#1143), so a quit, a closed
// VSCode panel or the window's Reload doesn't lose it. Kept like the recent messages
// (`commitMessageHistory.ts`), and forgotten with the repository's other web storage
// (`repoWebStorage.ts`, #1096).
const storageKey = (repoPath: string) => `commit-draft:${repoPath}`;

const isString = (value: unknown): value is string => typeof value === "string";
const isPeople = (value: unknown): value is SignatureInput[] =>
  Array.isArray(value) &&
  value.every((person) => typeof person === "object" && person !== null && isString(person.name) && isString(person.email));

/** A draft with nothing in it: no message, no message set aside for amend mode, no people. */
function isEmpty(draft: CommitDraft): boolean {
  return (
    draft.message === "" &&
    (draft.stashed ?? "") === "" &&
    (draft.coAuthors ?? []).length === 0 &&
    (draft.stashedCoAuthors ?? []).length === 0 &&
    (draft.amendAuthor ?? null) === null
  );
}

/** The draft kept for `repoPath`, if any. */
export function readCommitDraft(repoPath: string): CommitDraft | undefined {
  try {
    const stored = localStorage.getItem(storageKey(repoPath));
    if (stored === null) return undefined;
    const parsed: unknown = JSON.parse(stored);
    if (typeof parsed !== "object" || parsed === null) return undefined;
    const draft = parsed as Record<string, unknown>;
    if (!isString(draft.message) || !isString(draft.lastSeeded)) return undefined;
    return {
      message: draft.message,
      lastSeeded: draft.lastSeeded,
      stashed: isString(draft.stashed) ? draft.stashed : null,
      coAuthors: isPeople(draft.coAuthors) ? draft.coAuthors : [],
      stashedCoAuthors: isPeople(draft.stashedCoAuthors) ? draft.stashedCoAuthors : null,
      amendAuthor: isString(draft.amendAuthor) ? draft.amendAuthor : null,
    };
  } catch {
    return undefined;
  }
}

/** Keeps `draft` for `repoPath`; an empty one (a commit landed, the field cleared) is removed. */
export function writeCommitDraft(repoPath: string, draft: CommitDraft): void {
  try {
    if (isEmpty(draft)) localStorage.removeItem(storageKey(repoPath));
    else localStorage.setItem(storageKey(repoPath), JSON.stringify(draft));
  } catch {
    // Blocked or full storage: the draft lives in memory only, as before #1143.
  }
}
