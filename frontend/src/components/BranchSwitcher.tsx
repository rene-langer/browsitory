import { useState, type KeyboardEvent } from "react";
import type { BranchInfo } from "../ipc/RepoClient";

export function BranchSwitcher({
  branches,
  createBranchDraft,
  onSwitchBranch,
  onCreateBranch,
  onDeleteBranch,
  onRenameBranch,
  onOpenCreateBranchDraft,
  onCloseCreateBranchDraft,
}: {
  branches: BranchInfo[];
  createBranchDraft: { startPoint: string } | null;
  onSwitchBranch: (name: string) => void;
  onCreateBranch: (name: string, startPoint: string) => void;
  onDeleteBranch: (name: string, force: boolean) => Promise<void>;
  onRenameBranch: (oldName: string, newName: string) => void;
  onOpenCreateBranchDraft: (startPoint: string) => void;
  onCloseCreateBranchDraft: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [newBranchName, setNewBranchName] = useState("");
  const [pendingForceFor, setPendingForceFor] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");

  const current = branches.find((b) => b.isCurrent);

  const submitCreate = () => {
    if (newBranchName.trim() === "" || createBranchDraft === null) {
      return;
    }
    onCreateBranch(newBranchName.trim(), createBranchDraft.startPoint);
    setNewBranchName("");
  };

  const handleDeleteClick = async (name: string) => {
    await onDeleteBranch(name, false);
    // useAppState swallows a rejected mutation into state.error rather than rethrowing, so the
    // only reliable "did it actually delete" signal here is whether `name` is still present in
    // `branches` on the next render — a successful delete drops it from the list entirely,
    // which also makes the "Force Delete" button below disappear along with the row.
    setPendingForceFor(name);
  };

  const handleRenameKeyDown = (event: KeyboardEvent<HTMLInputElement>, oldName: string) => {
    if (event.key === "Enter") {
      onRenameBranch(oldName, renameValue);
      setRenaming(null);
    }
  };

  return (
    <div>
      <button aria-label="Branch switcher" onClick={() => setOpen((o) => !o)}>
        {current?.name ?? "no branch"}
      </button>
      {open && (
        <div>
          <ul>
            {branches.map((b) => (
              <li key={b.name}>
                {renaming === b.name ? (
                  <input
                    value={renameValue}
                    onChange={(event) => setRenameValue(event.target.value)}
                    onKeyDown={(event) => handleRenameKeyDown(event, b.name)}
                  />
                ) : (
                  <button
                    onClick={() => {
                      onSwitchBranch(b.name);
                      setOpen(false);
                    }}
                  >
                    {b.name}
                    {b.isCurrent && " (current)"}
                  </button>
                )}
                <button
                  onClick={() => {
                    setRenaming(b.name);
                    setRenameValue(b.name);
                  }}
                >
                  Rename
                </button>
                {pendingForceFor === b.name ? (
                  <button
                    onClick={() => {
                      onDeleteBranch(b.name, true);
                      setPendingForceFor(null);
                    }}
                  >
                    Force Delete
                  </button>
                ) : (
                  <button onClick={() => handleDeleteClick(b.name)}>Delete</button>
                )}
              </li>
            ))}
          </ul>
          <button onClick={() => onOpenCreateBranchDraft("HEAD")}>New Branch…</button>
        </div>
      )}
      {createBranchDraft !== null && (
        <div>
          <input
            value={newBranchName}
            onChange={(event) => setNewBranchName(event.target.value)}
            placeholder="New branch name"
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                submitCreate();
              }
            }}
          />
          <button onClick={submitCreate} disabled={newBranchName.trim() === ""}>
            Create
          </button>
          <button onClick={onCloseCreateBranchDraft}>Cancel</button>
        </div>
      )}
    </div>
  );
}
