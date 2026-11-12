import { useEffect, useId, useState, type KeyboardEvent } from "react";
import type { FontFamily } from "../ipc/RepoClient";
import { Field } from "./primitives/Field";
import styles from "./PreferencesDialog.module.css";

/** Most suggestions shown at once; typing more narrows the list. */
const MAX_SUGGESTIONS = 100;

/**
 * The installed families whose name contains `query` (ignoring case), monospaced ones first,
 * each group in the order given (by name, from the backend).
 */
function matchingFamilies(families: readonly FontFamily[], query: string): FontFamily[] {
  const needle = query.trim().toLowerCase();
  if (needle === "") return [];
  const matches = families.filter((family) => family.name.toLowerCase().includes(needle));
  return [...matches.filter((f) => f.monospace), ...matches.filter((f) => !f.monospace)].slice(0, MAX_SUGGESTIONS);
}

/** `text` split at its last comma: what comes before it (with the comma), and the family typed after it. */
function lastFamily(text: string): { head: string; family: string } {
  const comma = text.lastIndexOf(",");
  return comma === -1 ? { head: "", family: text } : { head: `${text.slice(0, comma + 1)} `, family: text.slice(comma + 1) };
}

/**
 * Preferences → Appearance → Code font (#858): a text field that suggests installed font
 * families while typing (the WAI-ARIA combobox pattern with a list popup and manual selection).
 * The arrow keys move through the suggestions, Enter or a click picks one, Escape closes them.
 * Free text stays allowed (a font installed later, or a list such as `Fira Code, monospace`);
 * in a list the suggestions are for its last family, and picking one replaces only that family.
 *
 * The families load when the field is first focused (`loadFamilies`, cached by the caller); if
 * they can't be listed, the field works as plain text. Saved on blur or Enter, like the other
 * text settings; `onCommit` gets the trimmed text, `""` when cleared.
 */
export function CodeFontField({
  value,
  onCommit,
  loadFamilies,
}: {
  value: string;
  onCommit: (value: string) => void;
  loadFamilies: () => Promise<FontFamily[]>;
}) {
  // Local text while typing; `null` shows the saved value.
  const [draft, setDraft] = useState<string | null>(null);
  const [families, setFamilies] = useState<FontFamily[]>([]);
  const [loadRequested, setLoadRequested] = useState(false);
  const [open, setOpen] = useState(false);
  // Index into the suggestions; `-1` while none is picked (Enter then saves the typed text).
  const [active, setActive] = useState(-1);
  const listId = useId();

  useEffect(() => {
    if (!loadRequested) return;
    let cancelled = false;
    loadFamilies().then(
      (loaded) => {
        if (!cancelled) setFamilies(loaded);
      },
      () => {}, // no suggestions; the field still takes typed text
    );
    return () => {
      cancelled = true;
    };
  }, [loadRequested, loadFamilies]);

  const text = draft ?? value;
  const { head, family } = lastFamily(text);
  const suggestions = open ? matchingFamilies(families, family) : [];
  const expanded = suggestions.length > 0;
  const activeIndex = active < suggestions.length ? active : -1;

  const close = () => {
    setOpen(false);
    setActive(-1);
  };
  const commit = (next: string) => {
    close();
    setDraft(null);
    if (next.trim() !== value) onCommit(next.trim());
  };
  const pick = (picked: FontFamily) => {
    const next = head + picked.name;
    commit(next);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) {
        setOpen(true);
        return;
      }
      if (suggestions.length === 0) return;
      const step = event.key === "ArrowDown" ? 1 : -1;
      const from = activeIndex === -1 && step === -1 ? 0 : activeIndex;
      setActive((from + step + suggestions.length) % suggestions.length);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const picked = activeIndex === -1 ? undefined : suggestions[activeIndex];
      if (picked !== undefined) pick(picked);
      else commit(text);
    } else if (event.key === "Escape" && expanded) {
      // Closes the suggestions only, not the dialog.
      event.preventDefault();
      event.stopPropagation();
      close();
    }
  };

  return (
    <Field label="Code font">
      {(props) => (
        <div className={styles.fontField}>
          <input
            {...props}
            type="text"
            role="combobox"
            aria-autocomplete="list"
            aria-expanded={expanded}
            aria-controls={expanded ? listId : undefined}
            aria-activedescendant={activeIndex === -1 ? undefined : `${listId}-${activeIndex}`}
            placeholder="Default monospace font"
            spellCheck={false}
            autoComplete="off"
            value={text}
            onFocus={() => setLoadRequested(true)}
            onChange={(event) => {
              setDraft(event.target.value);
              setOpen(true);
              setActive(-1);
            }}
            onBlur={() => commit(text)}
            onKeyDown={onKeyDown}
          />
          {expanded && (
            <ul id={listId} role="listbox" aria-label="Installed fonts" className={styles.fontOptions}>
              {suggestions.map((suggestion, index) => (
                <li
                  key={suggestion.name}
                  id={`${listId}-${index}`}
                  role="option"
                  aria-selected={index === activeIndex}
                  aria-label={suggestion.monospace ? `${suggestion.name}, monospace` : suggestion.name}
                  data-family={suggestion.name}
                  className={styles.fontOption}
                  // Keeps the focus in the field, so its blur doesn't save the typed text first.
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={() => pick(suggestion)}
                >
                  <span data-preview="" style={{ fontFamily: `"${suggestion.name.replaceAll('"', '\\"')}"` }}>
                    {suggestion.name}
                  </span>
                  {suggestion.monospace && (
                    <span className={styles.fontMonospace} aria-hidden="true">
                      monospace
                    </span>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </Field>
  );
}
