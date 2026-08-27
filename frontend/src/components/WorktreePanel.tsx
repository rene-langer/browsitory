import { useState } from "react";
import { FolderGit2, GitFork } from "lucide-react";
import type { BranchInfo, WorktreeInfo } from "../ipc/RepoClient";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./WorktreePanel.module.css";

export function WorktreePanel({
  worktrees,
  branches,
  onOpenWorktree,
  onCreateWorktree,
  onRemoveWorktree,
  onPruneWorktrees,
  operationDisabled,
  operationDisabledReason,
}: {
  worktrees: WorktreeInfo[];
  branches: BranchInfo[];
  onOpenWorktree: (path: string) => Promise<void>;
  // `null` = created; a string = the failure message to show inline next to the form. See
  // `useAppState.ts`'s `createWorktree` (issue #30/UX-002).
  onCreateWorktree: (
    name: string,
    path: string,
    branch: string,
    startPoint: string | null,
  ) => Promise<string | null>;
  onRemoveWorktree: (name: string) => Promise<void>;
  onPruneWorktrees: () => Promise<void>;
  operationDisabled: boolean;
  // Human-readable reason `operationDisabled` is true, shown as a `title` on the buttons it
  // disables (issue #31/UX-003). `null` when nothing is blocking.
  operationDisabledReason: string | null;
}) {
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [branch, setBranch] = useState("");
  const [startPoint, setStartPoint] = useState("");
  const [removeConfirmation, setRemoveConfirmation] = useState<WorktreeInfo | null>(null);
  const [createError, setCreateError] = useState<string | null>(null);

  const createWorktree = async (event: React.FormEvent) => {
    event.preventDefault();
    const values = [name.trim(), path.trim(), branch.trim()];
    if (values.some((value) => value === "")) return;
    const failure = await onCreateWorktree(
      values[0],
      values[1],
      values[2],
      startPoint.trim() || null,
    );
    if (failure !== null) {
      setCreateError(failure);
      return;
    }
    setCreateError(null);
    setName("");
    setPath("");
    setBranch("");
    setStartPoint("");
  };

  const removeWorktree = (worktree: WorktreeInfo) => {
    setRemoveConfirmation(worktree);
  };

  return (
    <AccordionSection title="Worktrees" storageKey="sidebar-worktrees" icon={GitFork} count={worktrees.length}>
      <form className={styles.form} onSubmit={createWorktree} aria-label="Create worktree">
        <label className={styles.label}>
          Worktree name
          <input
            value={name}
            onChange={(event) => {
              setName(event.target.value);
              setCreateError(null);
            }}
          />
        </label>
        <label className={styles.label}>
          Worktree path
          <input
            value={path}
            onChange={(event) => setPath(event.target.value)}
          />
        </label>
        <label className={styles.label}>
          Branch
          <input
            list="worktree-branches"
            value={branch}
            onChange={(event) => setBranch(event.target.value)}
          />
        </label>
        <datalist id="worktree-branches">
          {branches.map((item) => (
            <option key={item.name} value={item.name} />
          ))}
        </datalist>
        <label className={styles.label}>
          Start point
          <input
            value={startPoint}
            onChange={(event) => setStartPoint(event.target.value)}
          />
        </label>
        {createError !== null && (
          <InlineError message={createError} onDismiss={() => setCreateError(null)} />
        )}
        <Toolbar>
          <button
            type="submit"
            disabled={operationDisabled}
            title={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}
          >
            Create worktree
          </button>
        </Toolbar>
      </form>
      {worktrees.length === 0 ? (
        <p className={styles.empty}>No worktrees. Create one above to work on another branch in parallel.</p>
      ) : (
        <ul className={styles.list}>
          {worktrees.map((worktree) => (
            <ListRow key={worktree.path}>
              <FolderGit2 size={14} aria-hidden="true" className={styles.rowIcon} />
              <strong>{worktree.isMain ? "Main" : "Linked"}</strong>
              <span>{worktree.path}</span>
              <span>{worktree.head ?? "Detached HEAD"}</span>
              {worktree.isLocked && <span>Locked</span>}
              {worktree.isPrunable && <span>Prunable</span>}
              <Toolbar>
                <button
                  type="button"
                  disabled={operationDisabled}
                  title={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}
                  onClick={() => void onOpenWorktree(worktree.path)}
                >
                  Open {worktree.path}
                </button>
                <button
                  type="button"
                  disabled={operationDisabled || worktree.isMain}
                  title={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}
                  onClick={() => removeWorktree(worktree)}
                >
                  Remove {worktree.path}
                </button>
              </Toolbar>
            </ListRow>
          ))}
        </ul>
      )}
      <Toolbar>
        <button
          type="button"
          disabled={operationDisabled}
          title={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}
          onClick={() => void onPruneWorktrees()}
        >
          Prune worktrees
        </button>
      </Toolbar>
      {removeConfirmation !== null && (
        <ConfirmDialog
          ariaLabel={`Remove worktree ${removeConfirmation.path}`}
          message={<p>Remove worktree at {removeConfirmation.path}?</p>}
          confirmLabel="Remove worktree"
          confirmDisabled={operationDisabled}
          confirmTitle={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}
          onConfirm={() => void onRemoveWorktree(removeConfirmation.name).then(() => setRemoveConfirmation(null))}
          onCancel={() => setRemoveConfirmation(null)}
        />
      )}
    </AccordionSection>
  );
}
