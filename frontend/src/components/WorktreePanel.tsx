import { useState } from "react";
import type { BranchInfo, WorktreeInfo } from "../ipc/RepoClient";

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
    if (window.confirm(`Remove worktree at ${worktree.path}?`)) {
      void onRemoveWorktree(worktree.name);
    }
  };

  return (
    <section
      className="worktree-panel"
      aria-labelledby="worktree-panel-heading"
    >
      <h2 id="worktree-panel-heading">Worktrees</h2>
      <form onSubmit={createWorktree} aria-label="Create worktree">
        <label>
          Worktree name
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        <label>
          Worktree path
          <input
            value={path}
            onChange={(event) => setPath(event.target.value)}
          />
        </label>
        <label>
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
        <label>
          Start point
          <input
            value={startPoint}
            onChange={(event) => setStartPoint(event.target.value)}
          />
        </label>
        <button type="submit" disabled={operationDisabled}>
          Create worktree
        </button>
      </form>
      <ul className="worktree-list">
        {worktrees.map((worktree) => (
          <li key={worktree.path}>
            <strong>{worktree.isMain ? "Main" : "Linked"}</strong>
            <span>{worktree.path}</span>
            <span>{worktree.head ?? "Detached HEAD"}</span>
            {worktree.isLocked && <span>Locked</span>}
            {worktree.isPrunable && <span>Prunable</span>}
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
          </li>
        ))}
      </ul>
      <button
        type="button"
        disabled={operationDisabled}
        onClick={() => void onPruneWorktrees()}
      >
        Prune worktrees
      </button>
    </section>
  );
}
