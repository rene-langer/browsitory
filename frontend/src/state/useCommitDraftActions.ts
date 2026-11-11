import { useCallback } from "react";
import type { CommitDraft } from "../components/CommitBox";
import type { AmendOptions, Trailer } from "../ipc/RepoClient";
import { recordCommitMessage } from "../lib/commitMessageHistory";
import type { HookRetry } from "../lib/hookFailure";
import { coAuthorTrailers, parsePerson } from "../lib/people";
import type { UseAppStateResult } from "./useAppState";

/**
 * `RepoWorkspace`'s commit actions that read or clear `App`'s per-repo commit-message draft
 * (see `useCommitDrafts`).
 */
export function useCommitDraftActions(
  repoPath: string,
  getCommitDraft: (repoPath: string) => CommitDraft | undefined,
  onCommitDraftChange: (repoPath: string, draft: CommitDraft) => void,
  appState: UseAppStateResult,
) {
  const handleCommitDraftChange = useCallback(
    (draft: CommitDraft) => onCommitDraftChange(repoPath, draft),
    [onCommitDraftChange, repoPath],
  );
  // A landed commit's message must not come back from the saved draft. CommitBox clears its own
  // field, but it may have unmounted mid-commit (tab switch, rebase panel), leaving the draft
  // holding the committed text; clear that too, unless it was edited since.
  const clearCommittedDraft = useCallback(
    (message: string) => {
      const draft = getCommitDraft(repoPath);
      if (draft !== undefined && draft.message === message) {
        onCommitDraftChange(repoPath, { ...draft, message: "", lastSeeded: "", coAuthors: [] });
      } else if (draft !== undefined && draft.stashed === message) {
        onCommitDraftChange(repoPath, { ...draft, stashed: "", stashedCoAuthors: [] });
      }
    },
    [getCommitDraft, onCommitDraftChange, repoPath],
  );
  // A landed message goes into the box's "Recent messages" (#443), unless it is git's own merge,
  // pick, revert or squash message, taken unedited.
  const seededMessage = appState.state.headInfo.operationLeftover ? null : appState.state.mergeMessage;
  const squashMessage = appState.state.headInfo.squashMessage;
  const recordLanded = useCallback(
    (message: string) => {
      if (message !== seededMessage && message !== squashMessage) recordCommitMessage(repoPath, message);
    },
    [repoPath, seededMessage, squashMessage],
  );
  const { commit: commitStaged, amendCommit, retryWithoutHooks } = appState;
  const commitAndClearDraft = useCallback(
    async (message: string, noVerify: boolean, trailers: Trailer[], onCommitted?: (commitId: string) => void) => {
      const ok = await commitStaged(message, noVerify, trailers, onCommitted);
      if (ok) {
        clearCommittedDraft(message);
        recordLanded(message);
      }
      return ok;
    },
    [commitStaged, clearCommittedDraft, recordLanded],
  );
  const amendAndRecord = useCallback(
    async (message: string, noVerify: boolean, options: AmendOptions) => {
      const ok = await amendCommit(message, noVerify, options);
      if (ok) recordLanded(message);
      return ok;
    },
    [amendCommit, recordLanded],
  );
  // "… without hooks" commits what the box holds now: the user may have fixed the message after
  // closing the dialog. Falls back to the failed message when the box is empty.
  const retryWithCurrentMessage = useCallback(
    async (retry: HookRetry) => {
      if (retry.kind === "push" || retry.kind === "continueSequence" || retry.kind === "pick" || retry.kind === "applyPatch") {
        return retryWithoutHooks(retry);
      }
      // In amend mode the box shows HEAD's message and the commit draft is stashed: a commit
      // retry takes the stashed draft, an amend retry only the amend-mode text.
      const draft = getCommitDraft(repoPath);
      const amending = draft?.stashed != null;
      const current =
        retry.kind === "commit" ? ((amending ? draft?.stashed : draft?.message) ?? "") : amending ? (draft?.message ?? "") : "";
      const message = current.trim() === "" ? retry.message : current;
      // While a merge/pick/squash message is seeded the box hides the chips and sends none.
      const seeded =
        (appState.state.mergeMessage !== null && !appState.state.headInfo.operationLeftover) ||
        appState.state.headInfo.squashMessage !== null;
      const fromBox = current.trim() !== "";
      if (retry.kind === "amend") {
        // The amend-mode Author field and chips as they are now, like the message.
        const chips = draft?.coAuthors;
        const trailers = seeded || !fromBox || chips == null ? retry.options.trailers : coAuthorTrailers(chips);
        // Blank (or never edited) keeps HEAD's author; text the box would refuse keeps the
        // author the failed amend was sent with.
        const authorText = draft?.amendAuthor?.trim() ?? "";
        const author = !fromBox
          ? retry.options.author
          : authorText === ""
            ? null
            : (parsePerson(authorText) ?? retry.options.author);
        const amended = await retryWithoutHooks({ ...retry, message, options: { author, trailers } });
        if (amended) recordLanded(message);
        return amended;
      }
      // The co-author chips the box holds now, too (the stashed ones while amending).
      const chips = draft === undefined ? undefined : amending ? draft.stashedCoAuthors : draft.coAuthors;
      const trailers = seeded || !fromBox || chips == null ? retry.trailers : coAuthorTrailers(chips);
      const ok = await retryWithoutHooks({ ...retry, message, trailers });
      if (ok && retry.kind === "commit") {
        clearCommittedDraft(message);
        recordLanded(message);
      }
      return ok;
    },
    [
      retryWithoutHooks,
      clearCommittedDraft,
      recordLanded,
      getCommitDraft,
      repoPath,
      appState.state.mergeMessage,
      appState.state.headInfo.operationLeftover,
      appState.state.headInfo.squashMessage,
    ],
  );
  return { handleCommitDraftChange, commitAndClearDraft, amendAndRecord, retryWithCurrentMessage };
}
