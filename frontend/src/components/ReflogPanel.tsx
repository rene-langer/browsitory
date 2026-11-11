import { useState } from "react";
import { formatShortDate } from "../lib/formatDate";
import { shortRefName } from "../lib/shortRefName";
import { History } from "lucide-react";
import type { ReflogEntry } from "../ipc/RepoClient";
import { AccordionSection } from "./primitives/AccordionSection";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./ReflogPanel.module.css";
import { useVirtualRows } from "./useVirtualRows";

export function ReflogPanel({
  references,
  selectedReference,
  entries,
  onSelectReference,
  onRestore,
  operationDisabled,
  revealRequest,
}: {
  references: string[];
  selectedReference: string | null;
  entries: ReflogEntry[];
  onSelectReference: (reference: string) => Promise<void>;
  onRestore: (reference: string, newId: string) => Promise<void>;
  operationDisabled: boolean;
  /** Expands and scrolls to the section (`useSidebarReveal`, #494). */
  revealRequest?: number;
}) {
  const [restoreConfirmation, setRestoreConfirmation] = useState<ReflogEntry | null>(null);
  // Only the rows in view render (#775), as in the commit graph (#685); each row is two fixed
  // lines for that.
  const [listElement, setListElement] = useState<HTMLUListElement | null>(null);
  const virtual = useVirtualRows(listElement, entries.length);
  const selectableReferences = references.filter(
    (reference) => reference === "HEAD" || reference.startsWith("refs/heads/"),
  );

  return (
    <AccordionSection
      title="Reflog"
      storageKey="sidebar-reflog"
      revealRequest={revealRequest}
      icon={History}
      count={selectedReference === null ? undefined : entries.length}
    >
      <label>
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
            const time = new Date(entry.timestamp * 1000);
            // The full details, cut short on the second line (#775).
            const details = [
              `Old ID: ${entry.oldId}`,
              `New ID: ${entry.newId}`,
              `${entry.committerName} <${entry.committerEmail}>`,
              time.toLocaleString(),
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
                    {formatShortDate(entry.timestamp)}
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
      {restoreConfirmation !== null && (
        <dialog open aria-label={`Restore ${shortRefName(restoreConfirmation.reference)}`}>
          <p>Restore {shortRefName(restoreConfirmation.reference)} to {restoreConfirmation.newId}?</p>
          <p>Your files and staged changes stay as they are.</p>
          <Toolbar>
            <button
              type="button"
              disabled={operationDisabled}
              onClick={() => {
                void onRestore(restoreConfirmation.reference, restoreConfirmation.newId)
                  .then(() => setRestoreConfirmation(null));
              }}
            >
              Restore reflog entry
            </button>
            <button type="button" onClick={() => setRestoreConfirmation(null)}>Cancel</button>
          </Toolbar>
        </dialog>
      )}
    </AccordionSection>
  );
}
