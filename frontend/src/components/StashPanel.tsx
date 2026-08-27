import { useState, type MouseEvent } from "react";
import { Archive } from "lucide-react";
import type { StashEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./StashPanel.module.css";

export function StashPanel({
  stashes,
  onSelectRow,
  onApplyStash,
  onDropStash,
  operationDisabled,
  operationDisabledReason,
}: {
  stashes: StashEntry[];
  onSelectRow: (row: SelectedRow) => void;
  onApplyStash: (index: number) => void;
  onDropStash: (index: number) => void;
  operationDisabled: boolean;
  // Human-readable reason `operationDisabled` is true, shown as a `title` on the buttons it
  // disables so they don't just go inert with no explanation (issue #31/UX-003). `null` when
  // nothing is blocking.
  operationDisabledReason: string | null;
}) {
  const [pendingDrop, setPendingDrop] = useState<StashEntry | null>(null);

  return (
    <AccordionSection title="Stashes" storageKey="sidebar-stashes" icon={Archive} count={stashes.length}>
      {stashes.length === 0 ? (
        <p className={styles.empty}>No stashes. Set changes aside from the commit box to see them here.</p>
      ) : (
        <ul className={styles.list}>
          {stashes.map((stash) => (
            <ListRow key={stash.commitId} onClick={() => onSelectRow({ commitId: stash.commitId })}>
              <Archive size={14} aria-hidden="true" className={styles.rowIcon} />
              <span className={styles.message}>{stash.message}</span>
              <Toolbar>
                <button
                  type="button"
                  disabled={operationDisabled}
                  title={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}
                  onClick={(event: MouseEvent<HTMLButtonElement>) => {
                    event.stopPropagation();
                    onApplyStash(stash.index);
                  }}
                >
                  Apply
                </button>
                <button
                  type="button"
                  disabled={operationDisabled}
                  title={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}
                  onClick={(event: MouseEvent<HTMLButtonElement>) => {
                    event.stopPropagation();
                    setPendingDrop(stash);
                  }}
                >
                  Drop
                </button>
              </Toolbar>
            </ListRow>
          ))}
        </ul>
      )}
      {pendingDrop !== null && (
        <ConfirmDialog
          ariaLabel={`Drop stash ${pendingDrop.message}`}
          message={<p>Drop "{pendingDrop.message}"? This cannot be undone.</p>}
          confirmLabel="Drop stash"
          confirmDisabled={operationDisabled}
          confirmTitle={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}
          onConfirm={() => {
            onDropStash(pendingDrop.index);
            setPendingDrop(null);
          }}
          onCancel={() => setPendingDrop(null)}
        />
      )}
    </AccordionSection>
  );
}
