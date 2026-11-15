import { Filter } from "lucide-react";
import styles from "./GraphFilterStrip.module.css";

/** What the strip says about a saved selection, `null` while every local branch is shown. */
function filterText(selection: { local: string[] | null; remote: string[] | null }): string | null {
  if (selection.local === null) return null;
  const shown = [...selection.local, ...(selection.remote ?? [])];
  if (shown.length === 0) return "No branches shown";
  if (shown.length === 1) return `Showing ${shown[0]} only`;
  return `Showing ${shown.length} selected branches`;
}

/**
 * The strip above the commit graph while it shows only some branches (#985): "Isolate branch" and
 * the sidebar's swatches are saved per repository, so without it a filtered graph looks like the
 * whole history. Shown while a local selection is saved (some local branch is hidden); a remote
 * selection alone only adds commits.
 */
export function GraphFilterStrip({
  selection,
  onShowAll,
}: {
  /** The saved graph selection (`AppState.graphSelection`): `null` lists are "nothing saved". */
  selection: { local: string[] | null; remote: string[] | null };
  /** Forgets both selections: every branch again. */
  onShowAll: () => void;
}) {
  const text = filterText(selection);
  if (text === null) return null;
  return (
    <div className={styles.strip}>
      <Filter size={14} aria-hidden="true" className={styles.icon} />
      <span role="status" aria-label="Graph filter" className={styles.text} title={text}>
        {text}
      </span>
      <button type="button" className={styles.showAll} onClick={onShowAll}>
        Show all branches
      </button>
    </div>
  );
}
