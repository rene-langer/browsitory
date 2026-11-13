import { IdentityDialog } from "./IdentityDialog";
import { useCallback, useEffect, useState } from "react";
import type { GraphCommit, HeadInfo, RepoClient } from "../ipc/RepoClient";
import { messageHistoryEntries, readCommitMessages, recentBranchCommits } from "../lib/commitMessageHistory";
import { conflictReason, isSplitting, rebaseInterruption, sequenceNoun } from "../lib/operationStatus";
import { useCommitMessageSettingsContext } from "../state/useCommitMessageSettings";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import { CommitBox, type CommitDraft } from "./CommitBox";
import styles from "./DiffPane.module.css";
import { RebaseProgressPanel } from "./RebaseProgressPanel";

/**
 * The working tree's commit dock: the commit box (message, options, then Commit), or
 * Continue/Abort while a cherry-pick/revert sequence or a rebase is paused. Stash is in the
 * repository toolbar (#637). Its actions come from `WorkspaceActionsContext` (#491).
 */
export function CommitDock({
  repoPath,
  client,
  commits,
  branchCommits,
  mergeMessage,
  amendMode,
  headInfo,
  rebaseProgress,
  sequencePending,
  stagedCount,
  conflictCount,
  initialCommitDraft,
  focusCommitMessage,
  clearCommitMessage,
}: {
  repoPath: string;
  client: RepoClient;
  /** The loaded history (may be a file history or search results); stands in for
   * `branchCommits` when that is absent. */
  commits?: GraphCommit[];
  /** The graph's commits, for the branch's recent messages and HEAD's author when amending. */
  branchCommits?: GraphCommit[];
  mergeMessage: string | null;
  amendMode: boolean;
  headInfo: HeadInfo;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  sequencePending: boolean;
  stagedCount: number;
  conflictCount: number;
  initialCommitDraft?: CommitDraft;
  focusCommitMessage: boolean;
  clearCommitMessage: string | null;
}) {
  const {
    onSequenceContinue,
    onSequenceAbort,
    onRebaseContinue,
    onRebaseEditMessage,
    onRebaseCommitPart,
    splittingCommit = null,
    onRebaseAbort,
    onCommit,
    onAbortMerge,
    onCommitDraftChange,
    onCommitMessageFocused,
    onCommitMessageCleared,
    commitHooks,
    onSetAmendMode,
    onAmendCommit,
    commitFlow,
  } = useWorkspaceActions();
  const { settings: commitMessageSettings } = useCommitMessageSettingsContext();
  const loadRecentAuthors = useCallback(() => client.listRecentAuthors(repoPath), [client, repoPath]);
  // A leftover's message belongs to a commit already made: the next commit is the user's own.
  const seededMergeMessage = headInfo.operationLeftover ? null : mergeMessage;
  const loadMessageHistory = useCallback(
    () =>
      messageHistoryEntries(
        readCommitMessages(repoPath),
        recentBranchCommits(branchCommits ?? commits ?? [], headInfo.headId),
        (commitId) => client.getCommitMessage(repoPath, commitId),
      ),
    [client, repoPath, branchCommits, commits, headInfo.headId],
  );
  // HEAD's author comes from the graph's commits: in file-history or search mode `commits` holds
  // those results, which may not include HEAD (#464).
  const headCommit =
    headInfo.headId === null ? undefined : (branchCommits ?? commits)?.find((c) => c.id === headInfo.headId);
  const headAuthor = headCommit === undefined ? null : { name: headCommit.authorName, email: headCommit.authorEmail };
  const [headMessage, setHeadMessage] = useState<string | null>(null);
  useEffect(() => {
    if (!amendMode || headInfo.headId === null) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setHeadMessage(null);
      return;
    }
    let cancelled = false;
    void client.getCommitMessage(repoPath, headInfo.headId).then(
      (text) => !cancelled && setHeadMessage(text),
      () => !cancelled && setHeadMessage(""),
    );
    return () => {
      cancelled = true;
    };
  }, [amendMode, headInfo.headId, client, repoPath]);
  // An Edit step's stop (Edit commit…, #882): the commit's message, editable for the recommit.
  // `original` is what Continue keeps when the text is left as it is.
  const editing = headInfo.rebase?.health.kind === "Paused" ? headInfo.rebase.editing : null;
  const [editMessage, setEditMessage] = useState<{ commitId: string; original: string; text: string } | null>(null);
  useEffect(() => {
    // Whatever was typed for another stop (or before a remount) is not this box's text.
    onRebaseEditMessage?.(null);
    if (editing === null) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setEditMessage(null);
      return;
    }
    let cancelled = false;
    void client.getCommitMessage(repoPath, editing).then(
      (text) => !cancelled && setEditMessage({ commitId: editing, original: text, text }),
      // Without the message there is nothing to edit; Continue keeps it.
      () => !cancelled && setEditMessage(null),
    );
    return () => {
      cancelled = true;
    };
  }, [editing, client, repoPath, onRebaseEditMessage]);
  const shownEditMessage = editMessage !== null && editMessage.commitId === editing ? editMessage : null;
  const emptyEditMessage = shownEditMessage !== null && shownEditMessage.text.trim() === "";
  // Split a commit (#884): after a part, an empty box continues without a message of its own.
  const splitParts = headInfo.rebase?.splitParts ?? 0;
  const splitting = isSplitting(headInfo.rebase, splittingCommit);
  const rebaseBlockedReason =
    rebaseInterruption(headInfo.rebase) ??
    conflictReason(conflictCount) ??
    (emptyEditMessage && splitParts === 0 ? "Write a commit message." : undefined);
  const commitPart = async () => {
    if (shownEditMessage === null || onRebaseCommitPart === undefined) return;
    const committed = await onRebaseCommitPart(shownEditMessage.text);
    if (!committed) return;
    // The next part starts from an empty box; Continue then sends no message unless one is typed.
    setEditMessage({ commitId: shownEditMessage.commitId, original: "", text: "" });
    onRebaseEditMessage?.(null);
  };
  return (
    <div className={styles.commitDock} role="group" aria-label="Commit actions">
      {headInfo.sequence !== null ? (
        // A stopped cherry-pick/revert sequence: no editable message per step; Continue
        // commits the resolved step with its MERGE_MSG, like rebase continue.
        <RebaseProgressPanel
          currentStep={headInfo.sequence.currentStep}
          totalSteps={headInfo.sequence.totalSteps}
          disabled={sequencePending || conflictCount > 0}
          disabledReason={conflictReason(conflictCount)}
          onContinue={onSequenceContinue}
          onAbort={onSequenceAbort}
          abortDisabled={sequencePending}
          title={headInfo.sequence.kind === "CherryPick" ? "Cherry-pick in progress" : "Revert in progress"}
          continueLabel={`Continue ${sequenceNoun(headInfo.sequence.kind)}`}
          abortLabel={`Abort ${sequenceNoun(headInfo.sequence.kind)}`}
          hooks={
            commitHooks === undefined ? undefined : { running: commitHooks.running, onCancel: commitHooks.onCancel }
          }
        />
      ) : rebaseProgress !== null ? (
        // An interrupted rebase (changed outside Browsitory) can only be aborted.
        <RebaseProgressPanel
          currentStep={rebaseProgress.currentStep}
          totalSteps={rebaseProgress.totalSteps}
          disabled={rebaseBlockedReason !== undefined}
          disabledReason={rebaseBlockedReason}
          onContinue={onRebaseContinue}
          onAbort={onRebaseAbort}
          title={editing === null ? undefined : `${splitting ? "Splitting" : "Editing"} ${editing.slice(0, 7)}`}
          message={
            shownEditMessage === null
              ? undefined
              : {
                  value: shownEditMessage.text,
                  onChange: (text) => {
                    setEditMessage({ ...shownEditMessage, text });
                    onRebaseEditMessage?.(text === shownEditMessage.original ? null : text);
                  },
                }
          }
          commitPart={
            shownEditMessage === null || onRebaseCommitPart === undefined
              ? undefined
              : {
                  onCommit: () => void commitPart(),
                  disabled: emptyEditMessage || rebaseInterruption(headInfo.rebase) !== undefined,
                }
          }
        />
      ) : (
        <CommitBox
          onCommit={onCommit}
          stagedCount={stagedCount}
          branchName={commitFlow?.branch}
          commitAndPush={
            commitFlow === undefined
              ? undefined
              : { onCommitAndPush: commitFlow.onCommitAndPush, disabledReason: commitFlow.commitAndPushReason }
          }
          disabled={stagedCount === 0 || conflictCount > 0}
          disabledReason={conflictCount > 0 ? conflictReason(conflictCount) : "Stage changes to commit"}
          onAbortMerge={onAbortMerge}
          abortLabel={
            headInfo.operation === "CherryPick"
              ? "Abort cherry-pick"
              : headInfo.operation === "Revert"
                ? "Abort revert"
                : "Abort merge"
          }
          // A pending squash merge pre-fills the message too, but there is no merge to abort.
          initialMessage={seededMergeMessage ?? headInfo.squashMessage ?? undefined}
          // Co-authors for a squash message, not for git's merge/pick/revert message (#142).
          initialMessageIsSquash={seededMergeMessage === null && headInfo.squashMessage !== null}
          // A terminal rebase's step writes MERGE_MSG (and a merge step MERGE_HEAD) too; only the
          // terminal ends it (#263).
          showAbort={mergeMessage !== null && headInfo.terminalRebase === null}
          initialDraft={initialCommitDraft}
          onDraftChange={onCommitDraftChange}
          focusMessage={focusCommitMessage}
          onFocusMessageHandled={onCommitMessageFocused}
          hooks={
            commitHooks === undefined
              ? undefined
              : {
                  installed: headInfo.commitHooks.length > 0,
                  // The worker lists `pre-merge-commit` only when the commit will run it.
                  merging: headInfo.commitHooks.includes("pre-merge-commit"),
                  ...commitHooks,
                }
          }
          clearMessage={clearCommitMessage}
          onClearMessageHandled={onCommitMessageCleared}
          messageSettings={commitMessageSettings}
          loadRecentAuthors={loadRecentAuthors}
          loadMessageHistory={loadMessageHistory}
          amend={{
            // A leftover merge/pick/revert doesn't block amend: the backend clears it first (#240).
            available:
              headInfo.headId !== null &&
              (headInfo.operationLeftover || (headInfo.operation === null && mergeMessage === null)) &&
              headInfo.sequence === null &&
              headInfo.terminalRebase === null,
            on: amendMode,
            headMessage,
            pushed: headInfo.headPushed,
            onToggle: onSetAmendMode,
            onAmend: onAmendCommit,
            headAuthor,
          }}
        />
      )}
      {commitFlow?.identity.required === true && (
        <IdentityDialog
          retrying={commitFlow.identity.retrying}
          onSave={commitFlow.identity.onSave}
          onCancel={commitFlow.identity.onCancel}
        />
      )}
    </div>
  );
}
