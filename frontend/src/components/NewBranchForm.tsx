import { useId, useState } from "react";
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

  const close = () => {
    setCreateError(null);
    onClose();
  };

  return (
    <form
      className={styles.draftForm}
      aria-label="New branch"
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
        autoFocus
        value={newBranchName}
        aria-invalid={newBranchProblem !== null}
        aria-describedby={newBranchProblem !== null ? "new-branch-name-problem" : undefined}
        onChange={(event) => {
          setNewBranchName(event.target.value);
          setCreateError(null);
        }}
        placeholder="New branch name"
        onKeyDown={(event) => {
          if (event.key === "Escape") close();
        }}
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
        Check out after creating
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
        <button type="button" onClick={close}>
          Cancel
        </button>
      </div>
      {createError !== null && <InlineError message={createError} onDismiss={() => setCreateError(null)} />}
    </form>
  );
}
