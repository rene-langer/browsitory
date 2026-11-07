import type { ReactNode } from "react";
import { Overlay } from "./primitives/Overlay";
import { Panel } from "./primitives/Panel";
import styles from "./ReleaseNotesModal.module.css";

/** The kind a changelog bullet's prefix ("- Fixed: ...") gives it. */
export type ReleaseNotesKind = "added" | "changed" | "fixed" | "removed" | "deprecated" | "security";

export interface ReleaseNotesItem {
  kind?: ReleaseNotesKind;
  text: string;
  /** Indented bullets under this one. */
  children?: ReleaseNotesItem[];
}

/** A paragraph or a bullet list, in changelog order. */
export type ReleaseNotesBlock = { paragraph: string } | { items: ReleaseNotesItem[] };

/** One `### Heading` of a release: a topic ("Commit signing") or a kind ("Fixed"). */
export interface ReleaseNotesGroup {
  title: string;
  blocks: ReleaseNotesBlock[];
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

// `code`, or **bold** (which may hold code). Matched left to right, so a code span wins over
// a bold marker that starts inside it; markers without a partner stay as text.
const INLINE = /`([^`]+)`|\*\*(.+?)\*\*/g;

/** Renders the changelog's inline `code` and **bold** markup; everything else stays text. */
function renderInline(text: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  let last = 0;
  for (const match of text.matchAll(INLINE)) {
    const start = match.index;
    if (start > last) nodes.push(text.slice(last, start));
    if (match[1] !== undefined) {
      nodes.push(<code key={start}>{match[1]}</code>);
    } else {
      nodes.push(<strong key={start}>{renderInline(match[2])}</strong>);
    }
    last = start + match[0].length;
  }
  if (last < text.length) nodes.push(text.slice(last));
  return nodes;
}

function ItemList({ items }: { items: ReleaseNotesItem[] }) {
  return (
    <ul>
      {items.map((item, index) => (
        <li key={index}>
          {item.kind !== undefined && (
            <>
              <span className={`${styles.kind} ${styles[item.kind]}`}>
                {KIND_LABELS[item.kind]}
                <span className={styles.srOnly}>:</span>
              </span>{" "}
            </>
          )}
          <span>{renderInline(item.text)}</span>
          {item.children !== undefined && item.children.length > 0 && <ItemList items={item.children} />}
        </li>
      ))}
    </ul>
  );
}

export function ReleaseNotesModal({
  entries,
  onClose,
  onOpenThirdPartyLicences,
  onOpenUserGuide,
}: {
  entries: ReleaseNotesEntry[];
  onClose: () => void;
  /** Shows the "Third-party licences" button; absent where the app can't show them. */
  onOpenThirdPartyLicences?: () => void;
  /** Shows the "User guide" button, which opens the user guide site in the browser. */
  onOpenUserGuide?: () => void;
}) {
  // A release with only developer-only changes has no groups; it isn't shown.
  const shown = entries.filter((entry) => entry.groups.length > 0);
  // The buttons sit in the panel's footer, which stays in view while the notes scroll: the
  // user can close at any time (#379), and the dialog's initial focus doesn't scroll the
  // notes to the end (#378).
  const footer = (
    <>
      <button type="button" onClick={onClose}>
        Close
      </button>
      {onOpenUserGuide !== undefined && (
        <button type="button" onClick={onOpenUserGuide}>
          User guide
        </button>
      )}
      {onOpenThirdPartyLicences !== undefined && (
        <button type="button" onClick={onOpenThirdPartyLicences}>
          Third-party licences
        </button>
      )}
    </>
  );
  return (
    <Overlay onClose={onClose} ariaLabel="Release notes" closeOnBackdropClick>
      <Panel title="Release notes" ariaLabel="Release notes" footer={footer} bodyLabel="Release notes text">
        {shown.length === 0 ? (
          <p className={styles.empty}>No release notes yet.</p>
        ) : (
          shown.map((entry) => (
            <section key={entry.version} className={styles.entry}>
              <h3>
                {entry.version} <span className={styles.date}>{entry.date}</span>
              </h3>
              {entry.groups.map((group) => (
                <div key={group.title} className={styles.group}>
                  <h4>{group.title}</h4>
                  {group.blocks.map((block, index) =>
                    "paragraph" in block ? (
                      <p key={index} className={styles.paragraph}>
                        {renderInline(block.paragraph)}
                      </p>
                    ) : (
                      <ItemList key={index} items={block.items} />
                    ),
                  )}
                </div>
              ))}
            </section>
          ))
        )}
      </Panel>
    </Overlay>
  );
}
