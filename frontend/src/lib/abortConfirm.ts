/**
 * Abort of a paused merge, cherry-pick, revert or rebase, or Cancel of an Edit commit… or Split
 * commit… stop (#979): it throws away work that Undo can't bring back (an abort "leaves nothing
 * to undo"), so it asks first when there is some, as Drop stash does (#662). When nothing was
 * touched it runs at once.
 */

/** What is aborted: a paused operation, or a history edit's stop (#994). */
export type AbortTarget =
  | { kind: "Merge" | "CherryPick" | "Revert" | "Rebase" }
  | { kind: "Edit" | "Split"; commitId: string };

/** The work an abort would lose, as far as the app can see it. */
export interface AbortWork {
  /** Files that were conflicted and are resolved now. */
  resolvedFiles: number;
  /** The open merge editor holds edits not saved yet. */
  mergeEditorEdits: boolean;
  /** An edit stop's files changed since the stop. */
  changedFiles: boolean;
  /** An edit stop's message box holds a message of its own. */
  messageChanged: boolean;
  /** A split's parts committed so far. */
  splitParts: number;
}

export const NO_ABORT_WORK: AbortWork = {
  resolvedFiles: 0,
  mergeEditorEdits: false,
  changedFiles: false,
  messageChanged: false,
  splitParts: 0,
};

export interface AbortConfirm {
  title: string;
  body: string;
  confirmLabel: string;
  cancelLabel: string;
}

const NOUNS = { Merge: "merge", CherryPick: "cherry-pick", Revert: "revert", Rebase: "rebase" } as const;

function files(count: number): string {
  return `${count} ${count === 1 ? "file" : "files"}`;
}

/** "a", "a and b", "a, b and c". */
function listOf(items: string[]): string {
  if (items.length <= 1) return items.join("");
  return `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}

/** The confirmation for aborting `target`, or `null` when nothing would be lost. */
export function abortConfirm(target: AbortTarget, work: AbortWork): AbortConfirm | null {
  // Each item, and whether it stands for one thing ("is lost") or several ("are lost").
  const items: { text: string; plural: boolean }[] = [];
  if (work.splitParts > 0) {
    items.push(
      work.splitParts === 1
        ? { text: "the part already committed", plural: false }
        : { text: `the ${work.splitParts} parts already committed`, plural: true },
    );
  }
  if (work.resolvedFiles > 0) items.push({ text: `your resolutions in ${files(work.resolvedFiles)}`, plural: true });
  if (work.mergeEditorEdits) items.push({ text: "your unsaved edits in the merge editor", plural: true });
  if (work.changedFiles) items.push({ text: "your changes to the files", plural: true });
  if (work.messageChanged) items.push({ text: "the new commit message", plural: false });
  if (items.length === 0) return null;

  const plural = items.length > 1 || items[0].plural;
  const list = listOf(items.map((item) => item.text));
  const body = `${list.charAt(0).toUpperCase()}${list.slice(1)} ${plural ? "are" : "is"} lost, and Undo can't bring ${plural ? "them" : "it"} back.`;
  if (target.kind === "Edit" || target.kind === "Split") {
    const noun = target.kind === "Edit" ? "edit" : "split";
    return {
      title: `Cancel the ${noun} of ${target.commitId.slice(0, 7)}?`,
      body,
      confirmLabel: `Cancel ${noun}`,
      cancelLabel: target.kind === "Edit" ? "Keep editing" : "Keep splitting",
    };
  }
  const noun = NOUNS[target.kind];
  return { title: `Abort the ${noun}?`, body, confirmLabel: `Abort ${noun}`, cancelLabel: "Cancel" };
}

/**
 * The paths git lists as conflicted in `MERGE_MSG` ("# Conflicts:" then "#\t<path>" lines, or the
 * older "Conflicts:" then "\t<path>"): the files the operation stopped on, also after a restart.
 */
export function mergeMessageConflicts(mergeMessage: string | null): string[] {
  if (mergeMessage === null) return [];
  const lines = mergeMessage.split("\n");
  const start = lines.findIndex((line) => /^#?\s*Conflicts:\s*$/.test(line));
  if (start < 0) return [];
  const paths: string[] = [];
  for (const line of lines.slice(start + 1)) {
    const match = /^#?\t(.+)$/.exec(line);
    if (match === null) break;
    paths.push(match[1]);
  }
  return paths;
}
