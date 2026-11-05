import { type RefObject, useCallback, useState } from "react";
import type { ConfirmedAbort, TerminalSequence } from "../ipc/RepoClient";
import { abortToast, rebaseAbortSwitchTarget, rebaseAbortToast, sequenceToast } from "../lib/operationStatus";
import type { useAnnounce } from "./useAnnounce";
import type { UseAppStateResult } from "./useAppState";
import type { ToastAction } from "./useToasts";

/**
 * `RepoWorkspace`'s cherry-pick/revert toasts and the Continue/Abort routes of a stopped
 * sequence, a rebase, a terminal cherry-pick and a merge, shared by the status strip, the
 * commit dock and the palette.
 */
export function useSequenceActions(
  appState: UseAppStateResult,
  pushToast: (message: string, action?: ToastAction) => void,
  mounted: RefObject<boolean>,
  announce: ReturnType<typeof useAnnounce>,
) {
  const { dismissError } = appState;
  // Abort of a terminal cherry-pick that removes its commits waits for this confirmation (#253).
  const [terminalAbortRequest, setTerminalAbortRequest] = useState<{
    sequence: TerminalSequence;
    headId: string | null;
  } | null>(null);
  // Cherry-pick/revert toasts come from the outcome, not from "no error": a pick that stopped on
  // a conflict succeeded as a call but must not say "Cherry-picked" (#107).
  const runPickAction = useCallback(
    async <T,>(action: () => Promise<T | null>, toast: (result: T) => string | null) => {
      dismissError();
      const result = await action();
      if (result === null || !mounted.current) return;
      const message = toast(result);
      if (message !== null) pushToast(message);
    },
    [dismissError, pushToast, mounted],
  );
  // A stopped multi-commit cherry-pick/revert: Continue and Abort from the strip, the commit
  // dock and the palette. Every Abort goes through `abortSequence` while one is stored, so the
  // sequence file never outlives the operation (the worker's AbortMerge is the backstop).
  const sequenceKind = appState.state.headInfo.sequence?.kind ?? null;
  const { continueSequence: continueStoredSequence, abortSequence: abortStoredSequence, abortMerge } = appState;
  // `noVerify`: the hook dialog's "Continue without hooks". A failing hook resolves `null` (the
  // dialog and banner show it), so no toast.
  const runContinueSequence = useCallback(
    (noVerify: boolean) => {
      if (sequenceKind === null) return;
      void runPickAction(
        () => continueStoredSequence(noVerify),
        (outcome) => sequenceToast(sequenceKind, outcome),
      );
    },
    [continueStoredSequence, runPickAction, sequenceKind],
  );
  // Takes no argument on purpose: it is wired to buttons, whose click event must never be read
  // as `noVerify`.
  const continueSequence = useCallback(() => runContinueSequence(false), [runContinueSequence]);
  const abortSequence = useCallback(() => {
    if (sequenceKind === null) return;
    void runPickAction(abortStoredSequence, (abort) => abortToast(sequenceKind, abort));
  }, [abortStoredSequence, runPickAction, sequenceKind]);
  // Abort rebase from the strip, the commit dock and the palette: the toast says whether HEAD
  // went back to the branch, from the rebase as it was before the abort.
  const { abortRebase: abortStoredRebase } = appState;
  const storedRebase = appState.state.headInfo.rebase;
  // After aborting a rebase whose branch moved, the toast offers "Switch to {branch}" through the
  // ordinary branch switch (a safe checkout that refuses rather than overwrite local changes).
  const { switchBranch: switchToBranch } = appState;
  const abortRebase = useCallback(async () => {
    const before = storedRebase;
    dismissError();
    const abort = await abortStoredRebase();
    if (abort === null || !mounted.current) return abort;
    const target = rebaseAbortSwitchTarget(abort, before);
    pushToast(
      rebaseAbortToast(abort, before),
      target === null
        ? undefined
        : {
            label: `Switch to ${target}`,
            run: () => {
              if (mounted.current) void announce("Switched branch", switchToBranch)(target);
            },
          },
    );
    return abort;
  }, [abortStoredRebase, announce, dismissError, pushToast, storedRebase, switchToBranch, mounted]);
  // A `git cherry-pick A B C` started in a terminal (#258): Abort works like `git cherry-pick
  // --abort` through `abortSequence` (#253). When it would remove the commits the sequence made,
  // it asks first.
  const terminalSequence = appState.state.headInfo.terminalSequence;
  // The abort carries what the user saw, so a sequence that moved on meanwhile (the terminal
  // went on, or this view was out of date) is refused instead of removing other commits.
  const headId = appState.state.headInfo.headId;
  const runTerminalAbort = useCallback(
    (sequence: TerminalSequence, confirmedHeadId: string | null) => {
      const confirmed: ConfirmedAbort = {
        headId: confirmedHeadId ?? "",
        startedAt: sequence.startedAt,
        rewinds: sequence.rewinds,
        commitsToRemove: sequence.commitsToRemove,
        pausedOnConflict: sequence.pausedOnConflict,
      };
      void runPickAction(
        () => abortStoredSequence(confirmed),
        (abort) => abortToast(sequence.kind, abort),
      );
    },
    [abortStoredSequence, runPickAction],
  );
  const abortTerminalSequence = useCallback(() => {
    if (terminalSequence === null) return;
    if (terminalSequence.rewinds && terminalSequence.commitsToRemove > 0)
      setTerminalAbortRequest({ sequence: terminalSequence, headId });
    else runTerminalAbort(terminalSequence, headId);
  }, [headId, runTerminalAbort, terminalSequence]);
  // A stored sequence first (its file must never outlive it), then a terminal sequence's paused
  // step (a merge paused during one is a plain merge), then the merge or single pick.
  const isTerminalStep = terminalSequence !== null && appState.state.headInfo.operation !== "Merge";
  const abortMergeOrSequence = useCallback(() => {
    if (sequenceKind !== null) abortSequence();
    else if (isTerminalStep) abortTerminalSequence();
    else void abortMerge();
  }, [abortSequence, abortMerge, abortTerminalSequence, isTerminalStep, sequenceKind]);
  // The palette's Abort entries take the same routes.
  const paletteAbortSequence = useCallback(async () => {
    if (sequenceKind !== null) return abortStoredSequence();
    abortTerminalSequence();
    return null;
  }, [abortStoredSequence, abortTerminalSequence, sequenceKind]);
  const paletteAbortMerge = useCallback(async () => abortMergeOrSequence(), [abortMergeOrSequence]);
  return {
    terminalAbortRequest,
    setTerminalAbortRequest,
    runPickAction,
    runContinueSequence,
    continueSequence,
    abortSequence,
    abortRebase,
    terminalSequence,
    runTerminalAbort,
    abortTerminalSequence,
    abortMergeOrSequence,
    paletteAbortSequence,
    paletteAbortMerge,
  };
}
