import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { formatChord, globalShortcut, type KeyChord, type ShortcutId } from "../lib/keybindings";
import {
  addShortcutChord,
  chordFromEvent,
  effectiveChords,
  isCustomized,
  isRebindable,
  rebindableShortcuts,
  removeShortcutChord,
  resetAllShortcuts,
  resetShortcut,
  useKeymap,
  type BindableId,
  type RebindableShortcut,
} from "../lib/keymap";
import { isApplePlatform } from "../lib/platform";
import { PreferencesSection } from "./PreferencesDialog";
import styles from "./KeyboardShortcutsSection.module.css";

/** Why a pressed key wasn't added: a message, and for another command's key, Replace. */
type Problem =
  | { kind: "refused"; id: BindableId; text: string }
  | { kind: "conflict"; id: BindableId; text: string; chord: KeyChord };

const GROUPS: RebindableShortcut["group"][] = ["Global", "Merge editor"];

function describe(id: BindableId): string {
  return rebindableShortcuts().find((shortcut) => shortcut.id === id)?.description ?? globalShortcut(id as ShortcutId).description;
}

/**
 * Preferences > Keyboard (#899): every shortcut the user can rebind with its keys, by group. Add
 * key records the next chord pressed (Esc cancels, Tab moves on); a key another command has is
 * named, with Replace; each key can be removed, and a shortcut or all of them reset. Keys are a
 * per-machine preference (`lib/keymap.ts`).
 */
export function KeyboardShortcutsSection({
  apple = isApplePlatform(),
  vscode = false,
  onOpenHostKeybindings,
}: {
  apple?: boolean;
  /** In VSCode, whose own Keyboard Shortcuts rebind the keys (#900, owner decision). */
  vscode?: boolean;
  /** Opens VSCode's Keyboard Shortcuts on Browsitory's commands. */
  onOpenHostKeybindings?: () => void;
}) {
  if (vscode) return <HostKeyboardShortcuts onOpen={onOpenHostKeybindings} />;
  return <KeymapEditor apple={apple} />;
}

/** In VSCode (#900): the keys are VSCode keybindings, changed in VSCode's own editor. */
function HostKeyboardShortcuts({ onOpen }: { onOpen?: () => void }) {
  return (
    <PreferencesSection title="Keyboard shortcuts">
      <p className={styles.hint}>
        In VSCode, change Browsitory&apos;s keys in VSCode&apos;s Keyboard Shortcuts. The keys VSCode handles itself
        (F5, F6, the tab keys and others) are listed there as &quot;browsitory.shortcut&quot; commands; the
        shortcut sheet shows the keys that work.
      </p>
      {onOpen !== undefined && (
        <div>
          <button type="button" onClick={onOpen}>
            Open Keyboard Shortcuts
          </button>
        </div>
      )}
    </PreferencesSection>
  );
}

function KeymapEditor({ apple }: { apple: boolean }) {
  useKeymap();
  const baseId = useId();
  const [query, setQuery] = useState("");
  const [recording, setRecording] = useState<BindableId | null>(null);
  const [problem, setProblem] = useState<Problem | null>(null);
  const addButtons = useRef(new Map<BindableId, HTMLButtonElement>());
  const replaceButton = useRef<HTMLButtonElement | null>(null);
  // The Add key button to focus once it is back.
  const focusAdd = useRef<BindableId | null>(null);

  useEffect(() => {
    const id = focusAdd.current;
    if (id === null) return;
    focusAdd.current = null;
    addButtons.current.get(id)?.focus();
  });

  useEffect(() => {
    if (problem?.kind === "conflict") replaceButton.current?.focus();
  }, [problem]);

  const finish = (id: BindableId) => {
    setRecording(null);
    setProblem(null);
    focusAdd.current = id;
  };

  const onRecorderKeyDown = (id: BindableId, event: KeyboardEvent<HTMLInputElement>) => {
    // Tab and Shift+Tab move on, which ends the recording.
    if (event.key === "Tab") return;
    // The keys belong to the recording, not to the app's shortcuts or the dialog.
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape" && !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey) {
      finish(id);
      return;
    }
    const chord = chordFromEvent(event, apple);
    if (chord === null) return;
    const keys = formatChord(chord, apple);
    const result = addShortcutChord(id, chord, {}, apple);
    if (result.ok) {
      finish(id);
    } else if (result.reason === "conflict") {
      const other = describe(result.conflictWith);
      if (isRebindable(result.conflictWith)) {
        setRecording(null);
        setProblem({ kind: "conflict", id, chord, text: `${keys} is already the shortcut for ${other}.` });
      } else {
        setProblem({ kind: "refused", id, text: `${keys} is already the shortcut for ${other}, which can't be changed.` });
      }
    } else {
      setProblem({ kind: "refused", id, text: `${keys} is reserved and can't be a shortcut.` });
    }
  };

  const needle = query.trim().toLowerCase();
  const shown = rebindableShortcuts().filter(
    (shortcut) =>
      needle === "" ||
      shortcut.description.toLowerCase().includes(needle) ||
      effectiveChords(shortcut.id, apple).some((chord) => formatChord(chord, apple).toLowerCase().includes(needle)),
  );
  const anyCustomized = rebindableShortcuts().some((shortcut) => isCustomized(shortcut.id));

  const renderRow = (shortcut: RebindableShortcut) => {
    const { id, description } = shortcut;
    const descriptionId = `${baseId}-${id}`;
    const chords = effectiveChords(id, apple);
    return (
      <li key={id} className={styles.row} aria-labelledby={descriptionId}>
        <span id={descriptionId} className={styles.description}>
          {description}
        </span>
        <span className={styles.keys}>
          {chords.length === 0 && <span className={styles.none}>No keys</span>}
          {chords.map((chord) => {
            const keys = formatChord(chord, apple);
            return (
              <span key={keys} className={styles.key}>
                <kbd>{keys}</kbd>
                <button
                  type="button"
                  className={styles.removeKey}
                  aria-label={`Remove ${keys} from ${description}`}
                  title={`Remove ${keys}`}
                  onClick={() => {
                    removeShortcutChord(id, chord, apple);
                    focusAdd.current = id;
                  }}
                >
                  ×
                </button>
              </span>
            );
          })}
          {recording === id ? (
            <input
              className={styles.recorder}
              readOnly
              autoFocus
              aria-label={`New key for ${description}`}
              placeholder="Press the keys…"
              onKeyDown={(event) => onRecorderKeyDown(id, event)}
              onBlur={() => {
                setRecording((current) => (current === id ? null : current));
                setProblem((current) => (current?.kind === "refused" && current.id === id ? null : current));
              }}
            />
          ) : (
            <button
              type="button"
              ref={(element) => {
                if (element === null) addButtons.current.delete(id);
                else addButtons.current.set(id, element);
              }}
              aria-label={`Add a key for ${description}`}
              onClick={() => {
                setProblem(null);
                setRecording(id);
              }}
            >
              Add key
            </button>
          )}
          {isCustomized(id) && (
            <button
              type="button"
              aria-label={`Reset ${description}`}
              title="Back to the built-in keys"
              onClick={() => {
                resetShortcut(id);
                focusAdd.current = id;
              }}
            >
              Reset
            </button>
          )}
        </span>
        {problem !== null && problem.id === id && (
          <p role="alert" className={styles.problem}>
            {problem.text}
            {problem.kind === "conflict" && (
              <>
                <button
                  type="button"
                  ref={replaceButton}
                  onClick={() => {
                    addShortcutChord(id, problem.chord, { replace: true }, apple);
                    finish(id);
                  }}
                >
                  Replace
                </button>
                <button type="button" onClick={() => finish(id)}>
                  Cancel
                </button>
              </>
            )}
          </p>
        )}
      </li>
    );
  };

  return (
    <PreferencesSection title="Keyboard shortcuts">
      <p className={styles.hint}>
        Add key, then press the new keys. A command can have several keys, or none. Reserved keys such as
        copy, paste, Esc, Tab and the arrow keys can&apos;t be used.
      </p>
      <input
        type="search"
        className={styles.search}
        aria-label="Search shortcuts"
        placeholder="Search shortcuts"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
      />
      {GROUPS.map((group, index) => {
        const rows = shown.filter((shortcut) => shortcut.group === group);
        if (rows.length === 0) return null;
        const titleId = `${baseId}-group-${index}`;
        return (
          <section key={group}>
            <h4 id={titleId} className={styles.groupTitle}>
              {group}
            </h4>
            <ul className={styles.list} aria-labelledby={titleId}>
              {rows.map(renderRow)}
            </ul>
          </section>
        );
      })}
      {shown.length === 0 && <p className={styles.hint}>No shortcut matches.</p>}
      <div className={styles.footer}>
        <button
          type="button"
          disabled={!anyCustomized}
          onClick={() => {
            resetAllShortcuts();
            setProblem(null);
            setRecording(null);
          }}
        >
          Reset all shortcuts
        </button>
      </div>
    </PreferencesSection>
  );
}
