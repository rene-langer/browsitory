import { Overlay } from "./primitives/Overlay";
import { Panel } from "./primitives/Panel";
import styles from "./ReleaseNotesModal.module.css";

export type ReleaseNotesSection = "added" | "changed" | "fixed" | "removed";

export interface ReleaseNotesEntry {
  version: string;
  date: string;
  sections: Partial<Record<ReleaseNotesSection, string[]>>;
}

const SECTION_LABELS: Record<ReleaseNotesSection, string> = {
  added: "Added",
  changed: "Changed",
  fixed: "Fixed",
  removed: "Removed",
};

const SECTION_ORDER: ReleaseNotesSection[] = ["added", "changed", "fixed", "removed"];

export function ReleaseNotesModal({
  entries,
  onClose,
  onOpenThirdPartyLicences,
}: {
  entries: ReleaseNotesEntry[];
  onClose: () => void;
  /** Shows the "Third-party licences" button; absent where the app can't show them. */
  onOpenThirdPartyLicences?: () => void;
}) {
  return (
    <Overlay onClose={onClose}>
      <Panel title="Release Notes" ariaLabel="Release notes">
        {entries.length === 0 ? (
          <p className={styles.empty}>No release notes yet.</p>
        ) : (
          entries.map((entry) => (
            <section key={entry.version} className={styles.entry}>
              <h3>
                {entry.version} <span className={styles.date}>{entry.date}</span>
              </h3>
              {SECTION_ORDER.filter((key) => (entry.sections[key]?.length ?? 0) > 0).map((key) => (
                <div key={key}>
                  <h4>{SECTION_LABELS[key]}</h4>
                  <ul>
                    {(entry.sections[key] ?? []).map((line) => (
                      <li key={line}>{line}</li>
                    ))}
                  </ul>
                </div>
              ))}
            </section>
          ))
        )}
        <div className={styles.footer}>
          <button type="button" onClick={onClose}>
            Close
          </button>
          {onOpenThirdPartyLicences !== undefined && (
            <button type="button" onClick={onOpenThirdPartyLicences}>
              Third-party licences
            </button>
          )}
        </div>
      </Panel>
    </Overlay>
  );
}
