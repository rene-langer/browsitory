// What a failed mutation's banner says around git's own text (#1032, after #997): the operation
// and its object first ("Couldn't switch to feature/x: …"), and the next step last. The next step
// here is for an error only git's text explains (kind `Other`); a kind with its own next step
// (`errorMessages.ts`'s hints) uses that one instead (`failureError`). The operation log keeps
// git's full text.

import type { HistoryEditKind, SequenceKind } from "../ipc/RepoClient";
import { shortRefName } from "./shortRefName";

/** A mutation that failed: what the banner starts with, and the next step it ends with. */
export interface FailedOperation {
  /** "Couldn't create branch feature/x": the operation and its object, no full stop. */
  failed: string;
  /** What to do for an error only git's text explains: a full sentence. */
  next: string;
}

/**
 * A failure the app makes itself (a failed commit hook) with its own next step, which the banner
 * shows in place of the operation's; `null` when the message already says what to do.
 */
export class FailureWithNextStep extends Error {
  readonly next: string | null;
  constructor(message: string, next: string | null) {
    super(message);
    this.name = "FailureWithNextStep";
    this.next = next;
  }
}

/** The next step for a failed hook: its output says what to fix. */
export function hookNextStep(again: string): string {
  return `Fix what the hook reports, then ${again}.`;
}

const short = (id: string) => id.slice(0, 7);

/** The file name at the end of `path`, for messages. */
function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

// The usual cause of a write that git refuses without a kind of its own: another program holds
// the repository's lock (`index.lock`).
const unlocked = (again: string) => `Check that no other Git program is using the repository, then ${again}.`;

const op = (failed: string, next: string): FailedOperation => ({ failed, next });

const HISTORY_EDIT: Record<HistoryEditKind, (id: string) => string> = {
  Reword: (id) => `Couldn't reword ${id}`,
  Squash: (id) => `Couldn't squash ${id} into its parent`,
  Fixup: (id) => `Couldn't fix up ${id} into its parent`,
  MoveUp: (id) => `Couldn't move ${id} up`,
  MoveDown: (id) => `Couldn't move ${id} down`,
  Drop: (id) => `Couldn't drop ${id}`,
  Edit: (id) => `Couldn't start editing ${id}`,
  Split: (id) => `Couldn't start splitting ${id}`,
};

/** One per mutation the user starts. */
export const failedOperation = {
  switchBranch: (name: string) =>
    op(
      `Couldn't switch to ${name}`,
      "Check that the branch still exists and no other Git program is using the repository, then switch again.",
    ),
  switchDetached: (commitId: string) => op(`Couldn't switch to ${short(commitId)} (detached)`, unlocked("switch again")),
  stashAndSwitch: (name: string) =>
    op(`Couldn't stash and switch to ${name}`, "Check the stash list for your changes, then switch again."),
  createBranch: (name: string) =>
    op(`Couldn't create branch ${name}`, "Check the name and the start point, then create it again."),
  deleteBranch: (name: string) =>
    op(
      `Couldn't delete branch ${name}`,
      "If another worktree is on it, switch that worktree to another branch, then delete it again.",
    ),
  renameBranch: (oldName: string, newName: string) =>
    op(`Couldn't rename branch ${oldName} to ${newName}`, "Pick another name, then rename it again."),
  createTag: (name: string) => op(`Couldn't create tag ${name}`, "Check the name and the commit, then create it again."),
  deleteTag: (name: string) => op(`Couldn't delete tag ${name}`, "Check that the tag still exists, then delete it again."),
  saveStash: () => op("Couldn't stash your changes", "Check that there are changes to stash, then stash again."),
  applyStash: (index: number) =>
    op(`Couldn't apply stash@{${index}}`, "Commit or stash your changes, then apply it again."),
  popStash: (index: number) => op(`Couldn't pop stash@{${index}}`, "Commit or stash your changes, then pop it again."),
  dropStash: (index: number) =>
    op(`Couldn't drop stash@{${index}}`, "Check that the stash still exists, then drop it again."),
  reset: (commitId: string) => op(`Couldn't reset to ${short(commitId)}`, unlocked("reset again")),
  commit: () => op("Couldn't commit", "Check the message and the staged changes, then commit again."),
  amend: () => op("Couldn't amend the last commit", "Check the message, then amend again."),
  /** `fastForwardOnly`: such a merge fails when the branches have diverged. */
  merge: (ref: string, fastForwardOnly = false) =>
    op(
      `Couldn't merge ${shortRefName(ref)}`,
      fastForwardOnly ? "Pick “Fast-forward if possible” and merge again, or rebase first." : unlocked("merge again"),
    ),
  rebaseOnto: (ref: string) => op(`Couldn't rebase onto ${shortRefName(ref)}`, unlocked("rebase again")),
  historyEdit: (kind: HistoryEditKind, commitId: string) => op(HISTORY_EDIT[kind](short(commitId)), unlocked("try again")),
  foldStaged: (commitId: string) =>
    op(`Couldn't fold the staged changes into ${short(commitId)}`, unlocked("try again")),
  continueRebase: () =>
    op("Couldn't continue the rebase", "Check that every conflict is resolved, then continue again."),
  commitPart: () => op("Couldn't commit this part", "Check the message and the staged changes, then commit again."),
  abortRebase: () => op("Couldn't abort the rebase", unlocked("abort again")),
  abortMerge: () => op("Couldn't abort the merge", unlocked("abort again")),
  /** A cherry-pick or revert that was refused before it started ("Cherry-pick of 4c5beaf", #997). */
  pick: (kind: SequenceKind, commitIds: readonly string[]) => {
    const noun = kind === "CherryPick" ? "Cherry-pick" : "Revert";
    const what = commitIds.length === 1 ? short(commitIds[0]) : `${commitIds.length} commits`;
    return op(`${noun} of ${what} didn't start`, unlocked("try again"));
  },
  /** A cherry-pick or revert that failed after it started: some steps or the paused one stay. */
  pickStopped: (kind: SequenceKind, commitIds: readonly string[]) => {
    const noun = kind === "CherryPick" ? "cherry-pick" : "revert";
    const what = commitIds.length === 1 ? short(commitIds[0]) : `${commitIds.length} commits`;
    return op(`Couldn't finish the ${noun} of ${what}`, `Fix what stopped it, then continue or abort the ${noun}.`);
  },
  createWorktree: (name: string) =>
    op(
      `Couldn't create worktree ${name}`,
      "Check that its folder doesn't exist yet and no other worktree is on its branch, then create it again.",
    ),
  removeWorktree: (name: string) =>
    op(`Couldn't remove worktree ${name}`, "Check that it has no uncommitted changes and isn't locked, then remove it again."),
  pruneWorktrees: () => op("Couldn't prune worktrees", unlocked("prune again")),
  initSubmodule: (path: string) =>
    op(`Couldn't initialize submodule ${path}`, "Check the submodule's entry in .gitmodules, then initialize it again."),
  updateSubmodule: (path: string) =>
    op(`Couldn't update submodule ${path}`, "Check the submodule's URL and your connection, then update it again."),
  /** A commit's file or hunk applied or reverted in the working tree (#448). */
  commitChange: (change: { commitId: string; path: string; hunk: unknown; reverse: boolean }) => {
    const what = change.hunk === null ? change.path : `the hunk in ${change.path}`;
    return op(
      `Couldn't ${change.reverse ? "revert" : "apply"} ${what} from ${short(change.commitId)}`,
      "Commit or stash the changes it touches, then try again.",
    );
  },
  savePatch: () => op("Couldn't save the patch", "Check that you can write to that folder, then save it again."),
  /** `path`: the patch file, when known (a dropped file); `null` for the one "Apply patch…" picks. */
  readPatch: (path: string | null) =>
    op(
      path === null ? "Couldn't read the patch file" : `Couldn't read ${fileName(path)}`,
      "Check that the file exists and is a patch, then open it again.",
    ),
  applyPatch: (path: string | null) =>
    op(
      path === null ? "Couldn't apply the patch" : `Couldn't apply ${fileName(path)}`,
      "Commit or stash the changes it touches, then apply it again.",
    ),
  restoreReflog: (reference: string, newId: string) =>
    op(`Couldn't restore ${shortRefName(reference)} to ${short(newId)}`, unlocked("restore it again")),
};
