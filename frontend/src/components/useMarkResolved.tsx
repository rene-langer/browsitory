import { useState, type ReactNode } from "react";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import { ConfirmDialog } from "./primitives/ConfirmDialog";

/**
 * "Mark resolved" for a conflicted file: asks first when conflict markers remain, and clears the
 * merge tool's notice once resolved. `confirmDialog` renders that question.
 */
export function useMarkResolved(path: string): {
  markResolved: (force: boolean) => Promise<void>;
  confirmDialog: ReactNode;
} {
  const { onMarkResolved, externalTools } = useWorkspaceActions();
  const [confirmMarkers, setConfirmMarkers] = useState(false);
  const markResolved = async (force: boolean) => {
    if (onMarkResolved === undefined) return;
    const outcome = await onMarkResolved(path, force);
    if (outcome === "MarkersRemain") setConfirmMarkers(true);
    else if (outcome === "Resolved") externalTools?.dismissMergeToolResult(path);
  };
  const confirmDialog = confirmMarkers ? (
    <ConfirmDialog
      ariaLabel="Mark resolved"
      message={`Conflict markers remain in ${path}. Mark it resolved anyway?`}
      confirmLabel="Mark resolved"
      onConfirm={() => {
        setConfirmMarkers(false);
        void markResolved(true);
      }}
      onCancel={() => setConfirmMarkers(false)}
    />
  ) : null;
  return { markResolved, confirmDialog };
}
