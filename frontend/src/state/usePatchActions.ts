import { useCallback } from "react";
import type { CommitChangeRequest, GraphCommit, PatchApply, PatchContent, PatchPreview, PatchSource, RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import { failureError, type RunMutationWithOutcome } from "./useMutationRunner";
import { withoutDialog } from "./workspaceDialog";

/**
 * A commit's file or hunk reverted or applied in the working tree, "Save as patch…" and "Apply
 * patch…" (#448). Reverting and applying never commit; a change that doesn't apply leaves the
 * working tree as it was and shows the reason in the error banner. Each finished action sets
 * `patchOutcome` for its toast.
 */
export interface PatchActions {
  applyCommitChange(change: CommitChangeRequest): Promise<void>;
  /** Asks where to save `source`'s patch, offering `suggestedName`. */
  savePatch(source: PatchSource, suggestedName: string): Promise<void>;
  /** Asks for a patch file and opens its preview (#658), which applies it with `applyPatch`. */
  applyPatchFile(): Promise<void>;
  /** Applies the previewed patch `how` the preview's buttons say, closing the preview. */
  applyPatch(content: PatchContent, preview: PatchPreview, how: PatchApply): Promise<void>;
}

const short = (id: string) => id.slice(0, 7);

/** The toast after `change` landed: "Reverted hunk in a.txt from 1234567". */
export function commitChangeMessage(change: CommitChangeRequest): string {
  const verb = change.reverse ? "Reverted" : "Applied";
  const what = change.hunk === null ? change.path : `hunk in ${change.path}`;
  const where = change.stage ? " (staged)" : " in the working tree";
  return `${verb} ${what} from ${short(change.commitId)}${where}`;
}

/** The toast after a patch file was applied `how` (#658). */
export function applyPatchMessage(preview: PatchPreview, how: PatchApply): string {
  if (how === "Commits") {
    const n = preview.parts.length;
    return n === 1 ? "Committed the patch" : `Committed the patch as ${n} commits`;
  }
  return how === "Stage" ? "Applied and staged the patch" : "Applied the patch to the working tree";
}

/** A file name for a patch: `0001-fix-the-thing.patch`, like `git format-patch`. */
export function patchFileName(subject: string, number = 1): string {
  const slug = subject
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 52)
    .replace(/-+$/, "");
  return `${String(number).padStart(4, "0")}-${slug || "patch"}.patch`;
}

/** "Save as patch…" for commits (oldest first): `0001-subject.patch`, or `3-commits.patch`. */
export function commitsPatchName(commitIds: string[], commits: GraphCommit[] | undefined): string {
  if (commitIds.length !== 1) return `${commitIds.length}-commits.patch`;
  const summary = commits?.find((commit) => commit.id === commitIds[0])?.summary ?? commitIds[0].slice(0, 7);
  return patchFileName(summary);
}

export function usePatchActions(
  client: RepoClient,
  repoPath: string,
  runMutationWithOutcome: RunMutationWithOutcome,
  setState: (updater: (prev: AppState) => AppState) => void,
): PatchActions {
  const applyCommitChange = useCallback(
    async (change: CommitChangeRequest) => {
      setState((prev) => ({ ...prev, patchOutcome: null }));
      const ok = await runMutationWithOutcome(() => client.applyCommitChange(repoPath, change));
      if (ok) setState((prev) => ({ ...prev, patchOutcome: { message: commitChangeMessage(change), undoable: true } }));
    },
    [client, repoPath, runMutationWithOutcome, setState],
  );

  // Reads only (the dialog and the file are the host's), so no `pending` and no refresh.
  const savePatch = useCallback(
    async (source: PatchSource, suggestedName: string) => {
      setState((prev) => ({ ...prev, patchOutcome: null, error: null }));
      try {
        const content = await client.getPatch(repoPath, source);
        const saved = await client.savePatchFile(suggestedName, content);
        if (saved !== null) setState((prev) => ({ ...prev, patchOutcome: { message: `Saved patch to ${saved}`, undoable: false } }));
      } catch (err) {
        setState((prev) => ({ ...prev, ...failureError(err) }));
      }
    },
    [client, repoPath, setState],
  );

  const applyPatchFile = useCallback(async () => {
    setState((prev) => ({ ...prev, patchOutcome: null, error: null }));
    let content;
    try {
      content = await client.openPatchFile();
    } catch (err) {
      setState((prev) => ({ ...prev, ...failureError(err) }));
      return;
    }
    if (content === null) return;
    // The preview only reads, like the file dialog: no `pending`, no refresh.
    try {
      const preview = await client.previewPatch(repoPath, content);
      setState((prev) => ({ ...prev, dialog: { kind: "applyPatch", content, preview } }));
    } catch (err) {
      setState((prev) => ({ ...prev, ...failureError(err) }));
    }
  }, [client, repoPath, setState]);

  const applyPatch = useCallback(
    async (content: PatchContent, preview: PatchPreview, how: PatchApply) => {
      setState((prev) => ({ ...prev, patchOutcome: null, dialog: withoutDialog(prev.dialog, "applyPatch") }));
      const ok = await runMutationWithOutcome(() => client.applyPatch(repoPath, content, how));
      if (ok) setState((prev) => ({ ...prev, patchOutcome: { message: applyPatchMessage(preview, how), undoable: true } }));
    },
    [client, repoPath, runMutationWithOutcome, setState],
  );

  return { applyCommitChange, savePatch, applyPatchFile, applyPatch };
}
