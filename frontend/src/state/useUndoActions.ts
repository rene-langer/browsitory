import { useCallback } from "react";
import type { RepoClient, UndoOutcome } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutationWithOutcome } from "./useMutationRunner";

export interface UndoActions {
  undo(): Promise<UndoOutcome | null>;
  redo(): Promise<UndoOutcome | null>;
}

// Refusals arrive as resolved outcomes (they cross the VSCode transport as plain data); they are
// turned into the shared error banner here. Real failures reject and are prefixed after
// `runMutationWithOutcome` has stored them.
function refusalMessage(verb: "undo" | "redo", outcome: UndoOutcome): string | null {
  switch (outcome.kind) {
    case "RepositoryChanged":
      return `Can't ${verb} ${outcome.label}: the repository changed since. Use the reflog to recover.`;
    case "OperationInProgress":
      return "Finish or abort the merge or rebase first.";
    default:
      return null;
  }
}

export function useUndoActions(
  client: RepoClient,
  repoPath: string,
  runMutationWithOutcome: RunMutationWithOutcome,
  setState: (updater: (prev: AppState) => AppState) => void,
): UndoActions {
  const step = useCallback(
    async (verb: "undo" | "redo"): Promise<UndoOutcome | null> => {
      let outcome: UndoOutcome | null = null;
      const ok = await runMutationWithOutcome(async () => {
        outcome = verb === "undo" ? await client.undo(repoPath) : await client.redo(repoPath);
      });
      if (!ok) {
        const prefix = verb === "undo" ? "Undo failed: " : "Redo failed: ";
        setState((prev) => ({ ...prev, error: prev.error === null ? null : `${prefix}${prev.error}` }));
        return null;
      }
      const message = outcome === null ? null : refusalMessage(verb, outcome);
      if (message !== null) setState((prev) => ({ ...prev, error: message }));
      return outcome;
    },
    [client, repoPath, runMutationWithOutcome, setState],
  );
  const undo = useCallback(() => step("undo"), [step]);
  const redo = useCallback(() => step("redo"), [step]);
  return { undo, redo };
}
