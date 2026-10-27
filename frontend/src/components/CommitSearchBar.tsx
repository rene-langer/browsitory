import { useEffect, useId, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { CircleHelp, Search, X } from "lucide-react";
import { SEARCH_SYNTAX, SEARCH_SYNTAX_FOOTER } from "../lib/searchSyntax";
import styles from "./CommitSearchBar.module.css";

/** The always-visible search field above the commit graph. */
export function CommitSearchBar({
  query,
  onQueryChange,
  onClose,
  onFocusResults,
  focusRequested,
  onFocusHandled,
}: {
  query: string;
  onQueryChange: (text: string) => void;
  onClose: () => void;
  onFocusResults: () => void;
  /** Set by Ctrl/Cmd+F or the palette; the bar focuses its field and acknowledges. */
  focusRequested: boolean;
  onFocusHandled: () => void;
}) {
  const [helpOpen, setHelpOpen] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const helpId = useId();

  useEffect(() => {
    if (!focusRequested) return;
    inputRef.current?.focus();
    inputRef.current?.select();
    onFocusHandled();
  }, [focusRequested, onFocusHandled]);

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
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
        onChange={(event) => onQueryChange(event.target.value)}
        onKeyDown={handleKeyDown}
      />
      {query !== "" && (
        <button type="button" className={styles.iconButton} aria-label="Close search" onClick={onClose}>
          <X size={14} aria-hidden="true" />
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
