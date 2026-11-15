import { useId, useRef, useState } from "react";
import { useEscapeToClose } from "../lib/escapeToClose";
import type { BranchInfo } from "../ipc/RepoClient";
import { readCheckoutNewBranch, writeCheckoutNewBranch } from "../lib/checkoutNewBranch";
import { branchNameProblem } from "../lib/refName";
import { InlineError } from "./primitives/InlineError";
import styles from "./BranchTree.module.css";
import { BlockedReason } from "./primitives/BlockedReason";
import { describedBy } from "../lib/ariaDescribedBy";

/**
 * The Branches section's inline "New branch" form, shown while `draft` is non-null. It owns its
 * fields; stay mounted while closed (it renders nothing then) so a typed name and the checkout
 * option survive closing and reopening the form.
 */
export function NewBranchForm({
  draft,
  branches,
  operationDisabled,
  busyReason,
  onCreateBranch,
  onClose,
}: {
  draft: { startPoint: string } | null;
  branches: BranchInfo[];
  /** Create follows the operation blockers (#561): it can also switch to the new branch. */
  operationDisabled: boolean;
  /** Why the form can't act now, shown in it; `null` when nothing blocks. */
  busyReason: string | null;
  // `null` = created; a string = the failure message to show inline. See `useAppState.ts`'s
  // `createBranch` (issue #30/UX-002). `checkout`: switch to it too, one undo step (#656).
  onCreateBranch: (name: string, startPoint: string, checkout: boolean) => Promise<string | null>;
  onClose: () => void;
}) {
  const reasonId = useId();
  // On by default and remembered (#656).
  const [checkoutAfterCreate, setCheckoutAfterCreate] = useState(readCheckoutNewBranch);
  const [newBranchName, setNewBranchName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  // Escape anywhere in the form closes it, focus back where it came from (#1001).
  const { containerRef: escapeRef, onKeyDown: onEscapeKeyDown, close: closeAndRefocus } = useEscapeToClose<HTMLFormElement>({
    open: draft !== null,
    initialFocus: nameRef,
    // The whole form, buttons included, even in a scrolled Branches section (#1010).
    reveal: true,
    onClose: () => {
      setCreateError(null);
      onClose();
    },
  });

  if (draft === null) return null;

  const newBranchProblem = branchNameProblem(newBranchName.trim());
  const startPoint = draft.startPoint;
  const currentBranchName = branches.find((branch) => branch.isCurrent)?.name;
  const createBaseLabel =
    startPoint === "HEAD" ? (currentBranchName ?? "HEAD") : /^[0-9a-f]{40}$/i.test(startPoint) ? startPoint.slice(0, 7) : startPoint;

  const submitCreate = async () => {
    // Re-checked: the repository can become busy while the form is open (#561).
    if (newBranchName.trim() === "" || operationDisabled) return;
    const failure = await onCreateBranch(newBranchName.trim(), draft.startPoint, checkoutAfterCreate);
    if (failure !== null) {
      setCreateError(failure);
      return;
    }
    setNewBranchName("");
    setCreateError(null);
  };

  return (
    <form
      ref={escapeRef}
      className={styles.draftForm}
      aria-label="New branch"
      onKeyDown={onEscapeKeyDown}
      onSubmit={(event) => {
        event.preventDefault();
        if (newBranchProblem === null) void submitCreate();
      }}
    >
      <p className={styles.draftBase}>
        New branch from <strong>{createBaseLabel}</strong>
      </p>
      <label htmlFor="new-branch-name" className={styles.draftLabel}>
        Branch name
      </label>
      <input
        id="new-branch-name"
        ref={nameRef}
        value={newBranchName}
        aria-invalid={newBranchProblem !== null}
        aria-describedby={newBranchProblem !== null ? "new-branch-name-problem" : undefined}
        onChange={(event) => {
          setNewBranchName(event.target.value);
          setCreateError(null);
        }}
        placeholder="New branch name"
      />
      {newBranchProblem !== null && (
        <p id="new-branch-name-problem" className={styles.draftProblem}>
          {newBranchProblem}
        </p>
      )}
      <label className={styles.draftCheckbox}>
        <input
          type="checkbox"
          checked={checkoutAfterCreate}
          onChange={(event) => {
            setCheckoutAfterCreate(event.target.checked);
            writeCheckoutNewBranch(event.target.checked);
          }}
        />
        Switch to it after creating
      </label>
      <BlockedReason id={reasonId} reason={busyReason} />
      <div className={styles.draftActions}>
        <button
          type="submit"
          disabled={newBranchName.trim() === "" || newBranchProblem !== null || operationDisabled}
          title={busyReason ?? undefined}
          aria-describedby={describedBy(reasonId, busyReason)}
        >
          Create
        </button>
        <button type="button" onClick={closeAndRefocus}>
          Cancel
        </button>
      </div>
      {createError !== null && <InlineError message={createError} onDismiss={() => setCreateError(null)} />}
    </form>
  );
}
