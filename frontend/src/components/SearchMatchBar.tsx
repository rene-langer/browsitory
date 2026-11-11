import { ChevronDown, ChevronUp, List } from "lucide-react";
import type { SearchMatches } from "../state/useSearchMatches";
import type { SearchView } from "../lib/searchViewPreference";
import styles from "./SearchMatchBar.module.css";

/**
 * Under the search field while a search is open (#687): the match count, Previous / Next match
 * (also Shift+Enter / Enter in the field, Shift+F3 / F3), and the List toggle between matches
 * highlighted in the graph and the flat list of matching commits.
 */
export function SearchMatchBar({
  matches,
  view,
  onToggleList,
}: {
  matches: SearchMatches;
  view: SearchView;
  onToggleList: () => void;
}) {
  return (
    <div className={styles.bar} role="toolbar" aria-label="Search matches">
      <span className={styles.count} role="status">
        {matches.text}
      </span>
      <button
        type="button"
        className={styles.iconButton}
        aria-label="Previous match"
        title="Previous match (Shift+Enter, Shift+F3)"
        aria-keyshortcuts="Shift+F3"
        disabled={!matches.canGoPrevious}
        onClick={() => void matches.step(-1)}
      >
        <ChevronUp size={14} aria-hidden="true" />
      </button>
      <button
        type="button"
        className={styles.iconButton}
        aria-label="Next match"
        title="Next match (Enter, F3)"
        aria-keyshortcuts="F3"
        disabled={!matches.canGoNext}
        onClick={() => void matches.step(1)}
      >
        <ChevronDown size={14} aria-hidden="true" />
      </button>
      <button
        type="button"
        className={styles.toggle}
        aria-pressed={view === "list"}
        title="Show only the matching commits, as a list"
        onClick={onToggleList}
      >
        <List size={14} aria-hidden="true" />
        List
      </button>
    </div>
  );
}
