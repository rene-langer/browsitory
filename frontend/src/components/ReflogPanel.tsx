import { useEffect, useState } from "react";
import { formatAbsoluteDate, formatShortDate } from "../lib/formatDate";
import { shortRefName } from "../lib/shortRefName";
import { useDateFormat } from "../state/useDateFormat";
import { History } from "lucide-react";
import type { ReflogEntry } from "../ipc/RepoClient";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ListRow } from "./primitives/ListRow";
import styles from "./ReflogPanel.module.css";
import { useVirtualRows } from "./useVirtualRows";

export function ReflogPanel({
  references,
  selectedReference,
  entries,
  hasMore = false,
  onLoadMore,
  onSelectReference,
  onRestore,
  operationDisabled,
  operationDisabledReason = null,
  revealRequest,
}: {
  references: string[];
  selectedReference: string | null;
  /** The newest entries of the selected reference's reflog (#1161). */
  entries: ReflogEntry[];
  /** Older entries follow; `onLoadMore` reads them. */
  hasMore?: boolean;
  onLoadMore?: () => void;
  onSelectReference: (reference: string) => Promise<void>;
  onRestore: (reference: string, newId: string) => Promise<void>;
  operationDisabled: boolean;
  /** Why `operationDisabled`, which the Restore confirmation says (#1133). */
  operationDisabledReason?: string | null;
  /** Expands and scrolls to the section (`useSidebarReveal`, #494). */
  revealRequest?: number;
}) {
  const dateFormat = useDateFormat();
  const [restoreConfirmation, setRestoreConfirmation] = useState<ReflogEntry | null>(null);
  // Only the rows in view render (#775), as in the commit graph (#685); each row is two fixed
  // lines for that.
  const [listElement, setListElement] = useState<HTMLUListElement | null>(null);
  const virtual = useVirtualRows(listElement, entries.length);
  const selectableReferences = references.filter(
    (reference) => reference === "HEAD" || reference.startsWith("refs/heads/"),
  );
  // HEAD's reflog shows without a pick first (#1026); the selection then stays across refreshes.
  const headAvailable = references.includes("HEAD");
  useEffect(() => {
    if (selectedReference === null && headAvailable) void onSelectReference("HEAD");
    // Only a missing selection (a first open, a reference that went away) picks HEAD again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedReference, headAvailable]);

  return (
    <AccordionSection
      title="Reflog"
      storageKey="sidebar-reflog"
      revealRequest={revealRequest}
      icon={History}
      count={selectedReference === null ? undefined : entries.length}
    >
      <label className={styles.reference}>
        Reflog reference
        <select
          value={selectedReference ?? ""}
          aria-label="Reflog reference"
          disabled={operationDisabled}
          onChange={(event) => void onSelectReference(event.target.value)}
        >
          <option value="" disabled>Select a reference</option>
          {selectableReferences.map((reference) => (
            <option key={reference} value={reference}>{shortRefName(reference)}</option>
          ))}
        </select>
      </label>
      {selectedReference === null ? (
        <p className={styles.empty}>Select a reference above to see its reflog.</p>
      ) : entries.length === 0 ? (
        <p className={styles.empty}>No reflog entries for {shortRefName(selectedReference)}.</p>
      ) : (
        <ul
          ref={setListElement}
          className={styles.entryList}
          style={virtual.windowed ? { paddingTop: virtual.padTop, paddingBottom: virtual.padBottom } : undefined}
        >
          {entries.slice(virtual.start, virtual.end).map((entry, offset) => {
            const ordinal = virtual.start + offset;
            // The full details, cut short on the second line (#775).
            const details = [
              `Old ID: ${entry.oldId}`,
              `New ID: ${entry.newId}`,
              `${entry.committerName} <${entry.committerEmail}>`,
              formatAbsoluteDate(entry.timestamp, dateFormat),
              entry.message,
              ...(entry.summary === null ? [] : [entry.summary]),
            ].join("\n");
            return (
              <ListRow key={`${entry.reference}:${ordinal}`} rowIndex={ordinal} setSize={entries.length}>
                <History size={14} aria-hidden="true" className={styles.rowIcon} />
                <span className={styles.text} title={details}>
                  <span className={styles.message}>{entry.message}</span>
                  <span className={styles.details}>
                    {entry.oldId.slice(0, 7)} → {entry.newId.slice(0, 7)} · {entry.committerName} ·{" "}
                    {formatShortDate(entry.timestamp, dateFormat)}
                  </span>
                </span>
                <button
                  type="button"
                  className={styles.restoreButton}
                  disabled={operationDisabled}
                  aria-label={`Restore ${shortRefName(entry.reference)} to ${entry.newId}`}
                  onClick={() => setRestoreConfirmation(entry)}
                >
                  Restore {shortRefName(entry.reference)}
                </button>
              </ListRow>
            );
          })}
        </ul>
      )}
      {selectedReference !== null && hasMore && (
        <div className={styles.loadMore}>
          <span>
            Showing the latest {entries.length} {entries.length === 1 ? "entry" : "entries"}
          </span>
          <button type="button" onClick={onLoadMore}>
            Load more
          </button>
        </div>
      )}
      {restoreConfirmation !== null && (
        // The shared confirmation (#987): the title, what moves and what doesn't, and Undo.
        <ConfirmDialog
          title={`Restore ${shortRefName(restoreConfirmation.reference)}`}
          message={
            <>
              <p>
                Restore {shortRefName(restoreConfirmation.reference)} to {restoreConfirmation.newId}?
              </p>
              <p>Your files and staged changes stay as they are.</p>
              <p>Undo can bring it back.</p>
            </>
          }
          confirmLabel={`Restore ${shortRefName(restoreConfirmation.reference)}`}
          confirmDisabled={operationDisabled}
          // The repository can become busy while the confirmation is open.
          confirmReason={operationDisabled ? (operationDisabledReason ?? "Another operation is in progress.") : null}
          onConfirm={() => {
            if (operationDisabled) return;
            void onRestore(restoreConfirmation.reference, restoreConfirmation.newId).then(() =>
              setRestoreConfirmation(null),
            );
          }}
          onCancel={() => setRestoreConfirmation(null)}
        />
      )}
    </AccordionSection>
  );
}
