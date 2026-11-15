// Stub: the implementation follows the tests (#979).

export type AbortTarget =
  | { kind: "Merge" | "CherryPick" | "Revert" | "Rebase" }
  | { kind: "Edit" | "Split"; commitId: string };

export interface AbortWork {
  resolvedFiles: number;
  mergeEditorEdits: boolean;
  changedFiles: boolean;
  messageChanged: boolean;
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

export function abortConfirm(_target: AbortTarget, _work: AbortWork): AbortConfirm | null {
  return null;
}

export function mergeMessageConflicts(_mergeMessage: string | null): string[] {
  return [];
}
