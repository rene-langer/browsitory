import { FormDialog } from "./primitives/FormDialog";

/**
 * A pull whose branch and upstream have diverged: Merge, Rebase (the planner) or Cancel. Rendered
 * by `WorkspaceDialogs`, not the sidebar: the Branches section mounts its content only while open,
 * and the sidebar is `hidden` while collapsed, where a modal would be invisible (#636).
 */
export function PullDivergedDialog({
  upstreamRef,
  operationDisabled,
  operationDisabledReason,
  onMerge,
  onRebase,
  onCancel,
}: {
  upstreamRef: string;
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  onMerge: (upstreamRef: string) => void;
  onRebase: (upstreamRef: string) => void;
  onCancel: () => void;
}) {
  const title = operationDisabled ? (operationDisabledReason ?? undefined) : undefined;
  return (
    <FormDialog ariaLabel="Pull has diverged" onCancel={onCancel}>
      <p>The pull has diverged from {upstreamRef}.</p>
      <button type="button" disabled={operationDisabled} title={title} onClick={() => onMerge(upstreamRef)}>
        Merge
      </button>
      <button type="button" disabled={operationDisabled} title={title} onClick={() => onRebase(upstreamRef)}>
        Rebase
      </button>
      <button type="button" data-autofocus onClick={onCancel}>
        Cancel
      </button>
    </FormDialog>
  );
}
