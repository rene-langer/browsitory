import { useState } from "react";
import { FolderGit2, GitFork } from "lucide-react";
import type { BranchInfo, WorktreeInfo } from "../ipc/RepoClient";
import { AccordionSection } from "./primitives/AccordionSection";
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
}: {
  worktrees: WorktreeInfo[];
  branches: BranchInfo[];
  onOpenWorktree: (path: string) => Promise<void>;
  onCreateWorktree: (
    name: string,
    path: string,
    branch: string,
    startPoint: string | null,
  ) => Promise<void>;
  onRemoveWorktree: (name: string) => Promise<void>;
  onPruneWorktrees: () => Promise<void>;
  operationDisabled: boolean;
}) {
  const [name, setName] = useState("");
  const [path, setPath] = useState("");
  const [branch, setBranch] = useState("");
  const [startPoint, setStartPoint] = useState("");
  const [removeConfirmation, setRemoveConfirmation] = useState<WorktreeInfo | null>(null);

  const createWorktree = async (event: React.FormEvent) => {
    event.preventDefault();
    const values = [name.trim(), path.trim(), branch.trim()];
    if (values.some((value) => value === "")) return;
    await onCreateWorktree(
      values[0],
      values[1],
      values[2],
      startPoint.trim() || null,
    );
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
            onChange={(event) => setName(event.target.value)}
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
        <Toolbar>
          <button type="submit" disabled={operationDisabled}>
            Create worktree
          </button>
        </Toolbar>
      </form>
      <ul className={styles.list}>
        {worktrees.map((worktree) => (
          <li key={worktree.path}>
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
                onClick={() => void onOpenWorktree(worktree.path)}
              >
                Open {worktree.path}
              </button>
              <button
                type="button"
                disabled={operationDisabled || worktree.isMain}
                onClick={() => removeWorktree(worktree)}
              >
                Remove {worktree.path}
              </button>
            </Toolbar>
          </li>
        ))}
      </ul>
      <Toolbar>
        <button
          type="button"
          disabled={operationDisabled}
          onClick={() => void onPruneWorktrees()}
        >
          Prune worktrees
        </button>
      </Toolbar>
      {removeConfirmation !== null && (
        <dialog open aria-label={`Remove worktree ${removeConfirmation.path}`}>
          <p>Remove worktree at {removeConfirmation.path}?</p>
          <button
            type="button"
            disabled={operationDisabled}
            onClick={() => void onRemoveWorktree(removeConfirmation.name).then(() => setRemoveConfirmation(null))}
          >
            Remove worktree
          </button>
          <button type="button" onClick={() => setRemoveConfirmation(null)}>
            Cancel
          </button>
        </dialog>
      )}
    </AccordionSection>
  );
}
