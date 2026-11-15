import { type RefObject, useCallback, useState } from "react";
import type { ConfirmedAbort, EditRest, TerminalSequence } from "../ipc/RepoClient";
import { type AbortConfirm, type AbortTarget, type AbortWork, abortConfirm } from "../lib/abortConfirm";
import {
  abortToast,
  editStopAbortToast,
  editStopDoneToast,
  editStopKind,
  operationAbortToast,
  rebaseAbortSwitchTarget,
  rebaseAbortToast,
  sequenceToast,
  sequenceUndoable,
} from "../lib/operationStatus";
import type { useAnnounce } from "./useAnnounce";
import type { UseAppStateResult } from "./useAppState";
import type { PushToast, ToastAction } from "./useToasts";

/**
 * `RepoWorkspace`'s cherry-pick/revert toasts and the Continue/Abort routes of a stopped
 * sequence, a rebase, a terminal cherry-pick and a merge, shared by the status strip and the
 * palette. An Abort that would lose work Undo can't bring back asks first (#979): `abortWork`
 * says what it would lose, `abortRequest` is the open confirmation.
 */
export function useSequenceActions(
  appState: UseAppStateResult,
  pushToast: PushToast,
  mounted: RefObject<boolean>,
  announce: ReturnType<typeof useAnnounce>,
  undoToastAction: () => ToastAction,
  abortWork: () => AbortWork,
) {
  const { dismissError } = appState;
  // The confirmation an Abort waits for (#979), and what it runs once confirmed.
  const [abortRequest, setAbortRequest] = useState<{ confirm: AbortConfirm; run: () => void } | null>(null);
  const guardAbort = useCallback(
    (target: AbortTarget, run: () => void) => {
      const confirm = abortConfirm(target, abortWork());
      if (confirm === null) run();
      else setAbortRequest({ confirm, run });
    },
    [abortWork],
  );
  // Abort of a terminal cherry-pick that removes its commits waits for this confirmation (#253).
  const [terminalAbortRequest, setTerminalAbortRequest] = useState<{
    sequence: TerminalSequence;
    headId: string | null;
  } | null>(null);
  // Cherry-pick/revert toasts come from the outcome, not from "no error": a pick that stopped on
  // a conflict succeeded as a call but must not say "Cherry-picked" (#107). `undoable`: the
  // result finished the operation, so it is one undo step and the toast offers Undo (#662).
  const runPickAction = useCallback(
    async <T,>(
      action: () => Promise<T | null>,
      toast: (result: T) => string | null,
      undoable: (result: T) => boolean = () => false,
    ) => {
      dismissError();
      const result = await action();
      if (result === null || !mounted.current) return;
      const message = toast(result);
      if (message !== null) pushToast(message, undoable(result) ? { action: undoToastAction() } : undefined);
    },
    [dismissError, pushToast, mounted, undoToastAction],
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
        sequenceUndoable,
      );
    },
    [continueStoredSequence, runPickAction, sequenceKind],
  );
  // Takes no argument on purpose: it is wired to buttons, whose click event must never be read
  // as `noVerify`.
  const continueSequence = useCallback(() => runContinueSequence(false), [runContinueSequence]);
  const abortSequence = useCallback(() => {
    if (sequenceKind === null) return;
    guardAbort({ kind: sequenceKind }, () => {
      void runPickAction(abortStoredSequence, (abort) => abortToast(sequenceKind, abort));
    });
  }, [abortStoredSequence, guardAbort, runPickAction, sequenceKind]);
  // Abort rebase from the strip and the palette: the toast says whether HEAD went back to the
  // branch, from the rebase as it was before the abort. An Edit commit… or Split commit… stop
  // is cancelled instead, and says so (#994).
  const { abortRebase: abortStoredRebase } = appState;
  const storedRebase = appState.state.headInfo.rebase;
  const editKind = editStopKind(storedRebase, appState.state.splitting, appState.state.editing);
  // After aborting a rebase whose branch moved, the toast offers "Switch to {branch}" through the
  // ordinary branch switch (a safe checkout that refuses rather than overwrite local changes).
  const { switchBranch: switchToBranch } = appState;
  const runAbortRebase = useCallback(async () => {
    const before = storedRebase;
    const kind = editKind;
    dismissError();
    const abort = await abortStoredRebase();
    if (abort === null || !mounted.current) return abort;
    const target = rebaseAbortSwitchTarget(abort, before);
    pushToast(
      abort.restored && kind !== null && before?.editing != null
        ? editStopAbortToast(kind, before.editing)
        : rebaseAbortToast(abort, before),
      target === null
        ? undefined
        : {
            action: {
              label: `Switch to ${target}`,
              run: () => {
                if (mounted.current) void announce(`Switched to ${target}`, switchToBranch, { undoable: true })(target);
              },
            },
          },
    );
    return abort;
  }, [abortStoredRebase, announce, dismissError, editKind, pushToast, storedRebase, switchToBranch, mounted]);
  const abortRebase = useCallback(async () => {
    const target: AbortTarget =
      editKind !== null && storedRebase?.editing != null
        ? { kind: editKind, commitId: storedRebase.editing }
        : { kind: "Rebase" };
    const confirm = abortConfirm(target, abortWork());
    if (confirm === null) return runAbortRebase();
    setAbortRequest({ confirm, run: () => void runAbortRebase() });
    return null;
  }, [abortWork, editKind, runAbortRebase, storedRebase]);
  // Continue rebase from the strip, the palette and the rest dialog: an Edit commit… or Split
  // commit… stop that finished the rebase says so (#994), as one undo step.
  const { rebaseContinue } = appState;
  const continueRebase = useCallback(
    async (rest?: EditRest) => {
      const kind = editKind;
      const commitId = storedRebase?.editing ?? null;
      const step = await rebaseContinue(rest);
      if (step?.kind === "Done" && kind !== null && commitId !== null && mounted.current) {
        pushToast(editStopDoneToast(kind, commitId), { action: undoToastAction() });
      }
      return step;
    },
    [editKind, mounted, pushToast, rebaseContinue, storedRebase, undoToastAction],
  );
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
    // No commits to remove: it still asks when it would lose resolutions (#979).
    else guardAbort({ kind: terminalSequence.kind }, () => runTerminalAbort(terminalSequence, headId));
  }, [guardAbort, headId, runTerminalAbort, terminalSequence]);
  // A stored sequence first (its file must never outlive it), then a terminal sequence's paused
  // step (a merge paused during one is a plain merge), then the merge or single pick.
  const isTerminalStep = terminalSequence !== null && appState.state.headInfo.operation !== "Merge";
  const operation = appState.state.headInfo.operation ?? "Merge";
  // A leftover's changes are already in HEAD: its abort loses nothing (#240).
  const leftover = appState.state.headInfo.operationLeftover;
  const abortMergeOrSequence = useCallback(() => {
    if (sequenceKind !== null) abortSequence();
    else if (isTerminalStep) abortTerminalSequence();
    else {
      // "Aborted merge" once it worked (#979).
      const run = () => void announce(operationAbortToast(operation), abortMerge)();
      if (leftover) run();
      else guardAbort({ kind: operation }, run);
    }
  }, [abortSequence, abortMerge, abortTerminalSequence, announce, guardAbort, isTerminalStep, leftover, operation, sequenceKind]);
  // The palette's Abort entries take the same routes.
  const paletteAbortSequence = useCallback(async () => {
    if (sequenceKind !== null) abortSequence();
    else abortTerminalSequence();
    return null;
  }, [abortSequence, abortTerminalSequence, sequenceKind]);
  const paletteAbortMerge = useCallback(async () => abortMergeOrSequence(), [abortMergeOrSequence]);
  return {
    abortRequest,
    setAbortRequest,
    continueRebase,
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
