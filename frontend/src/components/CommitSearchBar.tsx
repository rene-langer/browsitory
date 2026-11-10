import { useEffect, useId, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { CircleHelp, Diff, Search, X } from "lucide-react";
import { SEARCH_SYNTAX, SEARCH_SYNTAX_FOOTER } from "../lib/searchSyntax";
import styles from "./CommitSearchBar.module.css";

const NO_RECENT_SEARCHES: string[] = [];

/**
 * The always-visible search field above the commit graph. Focused and empty, it offers the
 * repository's recent searches (#540) as a combobox list: ↓/↑ move through them, Enter runs one,
 * Delete (or its ×) forgets it, Escape closes the list.
 */
export function CommitSearchBar({
  query,
  onQueryChange,
  onClose,
  onFocusResults,
  focusRequested,
  onFocusHandled,
  error = null,
  recentSearches = NO_RECENT_SEARCHES,
  onRemoveRecentSearch,
  showLineStats,
  onToggleLineStats,
}: {
  query: string;
  onQueryChange: (text: string) => void;
  onClose: () => void;
  onFocusResults: () => void;
  /** Set by Ctrl/Cmd+F or the palette; the bar focuses its field and acknowledges. */
  focusRequested: boolean;
  onFocusHandled: () => void;
  /** Why the query is invalid (#450), shown under the field; `null` when it parses. */
  error?: string | null;
  /** This repository's recent searches, newest first; offered while the field is empty. */
  recentSearches?: string[];
  onRemoveRecentSearch?: (query: string) => void;
  /** The commit list's lines-changed badges (#620): the toggle shows with `onToggleLineStats`. */
  showLineStats?: boolean;
  onToggleLineStats?: () => void;
}) {
  const [helpOpen, setHelpOpen] = useState(false);
  const [focused, setFocused] = useState(false);
  // Escape closed the list; typing or focusing the field again reopens it.
  const [dismissed, setDismissed] = useState(false);
  // The highlighted recent search, -1 for none (the list opens with nothing highlighted).
  const [active, setActive] = useState(-1);
  const inputRef = useRef<HTMLInputElement>(null);
  const helpId = useId();
  const errorId = useId();
  const listId = useId();

  const recentOpen = focused && query === "" && !dismissed && !helpOpen && recentSearches.length > 0;
  const activeIndex = Math.min(active, recentSearches.length - 1);

  const pickRecent = (search: string) => {
    setActive(-1);
    onQueryChange(search);
  };
  const removeRecent = (search: string) => {
    onRemoveRecentSearch?.(search);
    inputRef.current?.focus();
  };

  useEffect(() => {
    if (!focusRequested) return;
    inputRef.current?.focus();
    inputRef.current?.select();
    onFocusHandled();
  }, [focusRequested, onFocusHandled]);

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (recentOpen) {
      const count = recentSearches.length;
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const step = event.key === "ArrowDown" ? 1 : -1;
        setActive(activeIndex < 0 ? (step === 1 ? 0 : count - 1) : (activeIndex + step + count) % count);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        setDismissed(true);
        setActive(-1);
        return;
      }
      const highlighted = activeIndex >= 0 ? recentSearches[activeIndex] : undefined;
      if (highlighted !== undefined && event.key === "Enter") {
        event.preventDefault();
        pickRecent(highlighted);
        return;
      }
      if (highlighted !== undefined && event.key === "Delete" && onRemoveRecentSearch !== undefined) {
        event.preventDefault();
        removeRecent(highlighted);
        return;
      }
    } else if (event.key === "ArrowDown" && query === "" && dismissed && recentSearches.length > 0) {
      // Back into a list Escape closed.
      event.preventDefault();
      setDismissed(false);
      setActive(0);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      if (helpOpen) setHelpOpen(false);
      else onClose();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      onFocusResults();
    }
  };

  return (
    <div className={styles.bar} role="search">
      <Search size={14} aria-hidden="true" className={styles.icon} />
      <input
        ref={inputRef}
        type="search"
        className={styles.input}
        aria-label="Search commits"
        placeholder="Search commits — author:, path:, contents:…"
        value={query}
        spellCheck={false}
        autoComplete="off"
        role="combobox"
        aria-autocomplete="none"
        aria-expanded={recentOpen}
        aria-controls={recentOpen ? listId : undefined}
        aria-activedescendant={recentOpen && activeIndex >= 0 ? `${listId}-${activeIndex}` : undefined}
        aria-invalid={error !== null}
        aria-describedby={error !== null ? errorId : undefined}
        onChange={(event) => {
          setDismissed(false);
          setActive(-1);
          onQueryChange(event.target.value);
        }}
        onFocus={() => {
          setFocused(true);
          setDismissed(false);
          setActive(-1);
        }}
        onBlur={() => setFocused(false)}
        onKeyDown={handleKeyDown}
      />
      {query !== "" && (
        <button type="button" className={styles.iconButton} aria-label="Close search" onClick={onClose}>
          <X size={14} aria-hidden="true" />
        </button>
      )}
      {onToggleLineStats !== undefined && (
        <button
          type="button"
          className={styles.iconButton}
          aria-label="Show lines changed"
          aria-pressed={showLineStats === true}
          title="Show lines changed per commit"
          onClick={onToggleLineStats}
        >
          <Diff size={14} aria-hidden="true" />
        </button>
      )}
      <button
        type="button"
        className={styles.iconButton}
        aria-label="Search syntax"
        aria-expanded={helpOpen}
        aria-controls={helpId}
        onClick={() => setHelpOpen((open) => !open)}
      >
        <CircleHelp size={14} aria-hidden="true" />
      </button>
      {error !== null && (
        <p id={errorId} className={styles.error} role="alert">
          {error}
        </p>
      )}
      {recentOpen && (
        <div className={styles.recent}>
          <p className={styles.recentTitle} aria-hidden="true">
            Recent searches
          </p>
          <ul id={listId} role="listbox" aria-label="Recent searches" className={styles.recentList}>
            {recentSearches.map((search, index) => (
              <li
                key={search}
                id={`${listId}-${index}`}
                role="option"
                aria-selected={index === activeIndex}
                className={styles.recentOption}
                // Keeps focus (and the list) in the field.
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => pickRecent(search)}
              >
                <span className={styles.recentQuery}>{search}</span>
                {onRemoveRecentSearch !== undefined && (
                  <button
                    type="button"
                    tabIndex={-1}
                    className={styles.recentRemove}
                    aria-label={`Remove “${search}” from recent searches`}
                    title="Remove from recent searches (Delete)"
                    onClick={(event) => {
                      event.stopPropagation();
                      removeRecent(search);
                    }}
                  >
                    <X size={12} aria-hidden="true" />
                  </button>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}
      {helpOpen && (
        <div id={helpId} className={styles.help} role="region" aria-label="Search syntax">
          <dl className={styles.syntax}>
            {SEARCH_SYNTAX.map((row) => (
              <div key={row.syntax} className={styles.syntaxRow}>
                <dt>
                  <code>{row.syntax}</code>
                </dt>
                <dd>{row.meaning}</dd>
              </div>
            ))}
          </dl>
          <p className={styles.footer}>{SEARCH_SYNTAX_FOOTER}</p>
        </div>
      )}
    </div>
  );
}
