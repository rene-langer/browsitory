import { useState } from "react";
import { History } from "lucide-react";
import type { ReflogEntry } from "../ipc/RepoClient";
import { AccordionSection } from "./primitives/AccordionSection";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./ReflogPanel.module.css";

export function ReflogPanel({
  references,
  selectedReference,
  entries,
  onSelectReference,
  onRestore,
  operationDisabled,
}: {
  references: string[];
  selectedReference: string | null;
  entries: ReflogEntry[];
  onSelectReference: (reference: string) => Promise<void>;
  onRestore: (reference: string, newId: string) => Promise<void>;
  operationDisabled: boolean;
}) {
  const [restoreConfirmation, setRestoreConfirmation] = useState<ReflogEntry | null>(null);
  const selectableReferences = references.filter(
    (reference) => reference === "HEAD" || reference.startsWith("refs/heads/"),
  );

  return (
    <AccordionSection title="Reflog" storageKey="sidebar-reflog" icon={History} count={entries.length}>
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
            <option key={reference} value={reference}>{reference}</option>
          ))}
        </select>
      </label>
      <ul className={styles.entryList}>
        {entries.map((entry, ordinal) => (
          <ListRow key={`${entry.reference}:${ordinal}`}>
            <History size={14} aria-hidden="true" className={styles.rowIcon} />
            <span className={styles.entryMeta}>Old ID: {entry.oldId}</span>
            <span className={styles.entryMeta}>New ID: {entry.newId}</span>
            <span>{entry.committerName} &lt;{entry.committerEmail}&gt;</span>
            <time dateTime={new Date(entry.timestamp * 1000).toISOString()}>
              {new Date(entry.timestamp * 1000).toLocaleString()}
            </time>
            <span>{entry.message}</span>
            {entry.summary !== null && <span>{entry.summary}</span>}
            <Toolbar>
              <button
                type="button"
                disabled={operationDisabled}
                aria-label={`Restore ${entry.reference} to ${entry.newId}`}
                onClick={() => setRestoreConfirmation(entry)}
              >
                Restore {entry.reference}
              </button>
            </Toolbar>
          </ListRow>
        ))}
      </ul>
      {restoreConfirmation !== null && (
        <dialog open aria-label={`Restore ${restoreConfirmation.reference}`}>
          <p>Restore {restoreConfirmation.reference} to {restoreConfirmation.newId}?</p>
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
