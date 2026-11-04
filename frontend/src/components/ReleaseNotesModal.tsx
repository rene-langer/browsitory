import type { ReactNode } from "react";
import { Overlay } from "./primitives/Overlay";
import { Panel } from "./primitives/Panel";
import styles from "./ReleaseNotesModal.module.css";

/** The kind a changelog bullet's prefix ("- Fixed: ...") gives it. */
export type ReleaseNotesKind = "added" | "changed" | "fixed" | "removed" | "deprecated" | "security";

export interface ReleaseNotesItem {
  kind?: ReleaseNotesKind;
  text: string;
}

/** One `### Heading` of a release: a topic ("Commit signing") or a kind ("Fixed"). */
export interface ReleaseNotesGroup {
  title: string;
  intro?: string[];
  items: ReleaseNotesItem[];
}

/** The shape `scripts/generateReleaseNotes.mjs` writes to `src/generated/releaseNotes.json`. */
export interface ReleaseNotesEntry {
  version: string;
  date: string;
  groups: ReleaseNotesGroup[];
}

const KIND_LABELS: Record<ReleaseNotesKind, string> = {
  added: "Added",
  changed: "Changed",
  fixed: "Fixed",
  removed: "Removed",
  deprecated: "Deprecated",
  security: "Security",
};

/** Renders the changelog's inline `code` and **bold** markup; everything else stays text. */
function renderInline(text: string): ReactNode[] {
  return text.split(/(`[^`]+`|\*\*[^*]+\*\*)/).map((part, index) => {
    if (part.length > 2 && part.startsWith("`") && part.endsWith("`")) {
      return <code key={index}>{part.slice(1, -1)}</code>;
    }
    if (part.length > 4 && part.startsWith("**") && part.endsWith("**")) {
      return <strong key={index}>{part.slice(2, -2)}</strong>;
    }
    return part;
  });
}

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
    <Overlay onClose={onClose} ariaLabel="Release notes">
      <Panel title="Release Notes" ariaLabel="Release notes">
        {entries.length === 0 ? (
          <p className={styles.empty}>No release notes yet.</p>
        ) : (
          entries.map((entry) => (
            <section key={entry.version} className={styles.entry}>
              <h3>
                {entry.version} <span className={styles.date}>{entry.date}</span>
              </h3>
              {entry.groups.map((group) => (
                <div key={group.title} className={styles.group}>
                  <h4>{group.title}</h4>
                  {group.intro?.map((paragraph, index) => (
                    <p key={index} className={styles.intro}>
                      {renderInline(paragraph)}
                    </p>
                  ))}
                  <ul>
                    {group.items.map((item, index) => (
                      <li key={index}>
                        {item.kind !== undefined && (
                          <span className={`${styles.kind} ${styles[item.kind]}`}>{KIND_LABELS[item.kind]}</span>
                        )}
                        <span>{renderInline(item.text)}</span>
                      </li>
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
