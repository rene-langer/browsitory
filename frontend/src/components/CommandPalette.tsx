import {
  Fragment,
  useId,
  useLayoutEffect,
  useMemo,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";
import { ListRow } from "./primitives/ListRow";
import {
  COMMAND_GROUPS,
  filterAndSortCommands,
  loadRecentCommandIds,
  recordCommandUsed,
  type Command,
  type PickerItem,
} from "../lib/commands";
import { matchItem, type MatchRange } from "../lib/fuzzyMatch";
import { shortcutLabel } from "../lib/keybindings";
import { useWindowedListbox } from "./useWindowedListbox";
import styles from "./CommandPalette.module.css";

/** How many recently used commands head the unfiltered list (#644). */
const RECENT_SHOWN = 5;
/** PageUp/PageDown's step when the rows have no measured height (jsdom). */
const FALLBACK_PAGE = 10;
const RECENT_HEADING = "Recent";
/** Heads the matches that can't run now, listed after the others for a typed query (#644). */
const UNAVAILABLE_HEADING = "Unavailable now";

type PaletteItem = Command | PickerItem;

export function CommandPalette({
  commands,
  onRun,
  initialPickerId = null,
}: {
  commands: Command[];
  onRun: () => void;
  /** Opens straight into this picker ("Switch branch…" from the toolbar's branch chip, #638). */
  initialPickerId?: string | null;
}) {
  const [query, setQuery] = useState("");
  const [highlightedIndex, setHighlightedIndex] = useState(0);
  // The picker command whose items are listed ("Go to branch…", #441), by id so it follows the
  // rebuilt `commands`; `null` lists the commands themselves.
  const [pickerId, setPickerId] = useState<string | null>(initialPickerId);
  const listboxId = useId();

  // Never an unavailable command's picker (#644): a blocked "Switch branch…" opens nothing.
  const findPicker = (id: string | null) =>
    id === null ? undefined : commands.find((command) => command.id === id && command.disabledReason === undefined);
  const pickerCommand = findPicker(pickerId);
  const picker = pickerCommand?.picker;

  const grouped = picker === undefined && query.trim() === "";
  // Unfiltered (#644): the recently used commands, then every other command under its kind
  // group (stable within a group). Filtered: one list in relevance order. Keyboard order always
  // equals displayed order, so the highlight index is over `results`; `headings` maps the index
  // of a group's first row to its heading.
  const { results, headings } = useMemo((): {
    results: readonly PaletteItem[];
    headings: ReadonlyMap<number, string>;
  } => {
    const items = commands.find((command) => command.id === pickerId && command.disabledReason === undefined)?.picker
      ?.items;
    if (items !== undefined) return { results: items(query), headings: new Map() };
    const ranked = filterAndSortCommands(
      commands.filter((command) => command.disabledReason === undefined),
      query,
    );
    if (query.trim() !== "") {
      // What can't run now follows the runnable matches, greyed out with its reason (#644).
      const unavailable = filterAndSortCommands(
        commands.filter((command) => command.disabledReason !== undefined),
        query,
      );
      const starts = new Map<number, string>();
      if (unavailable.length > 0) starts.set(ranked.length, UNAVAILABLE_HEADING);
      return { results: [...ranked, ...unavailable], headings: starts };
    }
    const byId = new Map(ranked.map((command) => [command.id, command]));
    const recent = loadRecentCommandIds()
      .map((id) => byId.get(id))
      .filter((command): command is Command => command !== undefined)
      .slice(0, RECENT_SHOWN);
    const recentIds = new Set(recent.map((command) => command.id));
    const sections: { heading: string; commands: Command[] }[] = [
      { heading: RECENT_HEADING, commands: recent },
      ...COMMAND_GROUPS.map((group) => ({
        heading: group,
        commands: ranked.filter((command) => command.group === group && !recentIds.has(command.id)),
      })),
    ];
    const flat: Command[] = [];
    const starts = new Map<number, string>();
    for (const section of sections) {
      if (section.commands.length === 0) continue;
      starts.set(flat.length, section.heading);
      flat.push(...section.commands);
    }
    return { results: flat, headings: starts };
  }, [commands, query, pickerId]);
  const clampedIndex = results.length === 0 ? 0 : Math.min(highlightedIndex, results.length - 1);
  const activeDescendantId =
    results.length === 0 ? undefined : `${listboxId}-option-${clampedIndex}`;

  // Only the options in view are rendered (#864): the top level lists a few commands per
  // branch, thousands with many branches. A state, not a ref, so the windowing sees the list.
  const [listElement, setListElement] = useState<HTMLUListElement | null>(null);
  const headingIndexes = useMemo(() => [...headings.keys()].sort((a, b) => a - b), [headings]);
  const windowing = useWindowedListbox(listElement, results.length, headingIndexes);
  const { scrollToIndex } = windowing;

  // Keep the highlighted row visible as the keys, paging or a new query move it (#643); before
  // paint, so it is rendered when `aria-activedescendant` names it.
  useLayoutEffect(() => {
    if (activeDescendantId === undefined) return;
    scrollToIndex(clampedIndex);
  }, [activeDescendantId, clampedIndex, results, scrollToIndex]);

  /** Rows per PageUp/PageDown: as many as the list shows at once. */
  function pageSize(): number {
    const list = listElement;
    const row = list?.querySelector<HTMLElement>('[role="option"]');
    if (list == null || row == null || row.offsetHeight === 0) return FALLBACK_PAGE;
    return Math.max(1, Math.floor(list.clientHeight / row.offsetHeight) - 1);
  }

  function moveHighlight(to: number) {
    setHighlightedIndex(Math.max(0, Math.min(to, results.length - 1)));
  }

  // The pointer moves the highlight (#643); a move, not an enter, so a row scrolling under a
  // resting pointer leaves the keyboard's highlight alone.
  function handleMouseMove(event: ReactMouseEvent<HTMLUListElement>) {
    const option = (event.target as Element).closest('[role="option"]');
    const prefix = `${listboxId}-option-`;
    if (option === null || !option.id.startsWith(prefix)) return;
    const index = Number(option.id.slice(prefix.length));
    if (index !== clampedIndex) setHighlightedIndex(index);
  }

  function runCommand(command: PaletteItem) {
    // An unavailable command stays listed with its reason; choosing it does nothing.
    if ("disabledReason" in command && command.disabledReason !== undefined) return;
    if (picker === undefined) recordCommandUsed(command.id);
    if (picker === undefined && "picker" in command && command.picker !== undefined) {
      setPickerId(command.id);
      setQuery("");
      setHighlightedIndex(0);
      return;
    }
    command.run();
    onRun();
  }

  function handleKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    // Home/End belong to the text's caret once something is typed; Ctrl+Home/End always jump.
    const jumpKey = (event.key === "Home" || event.key === "End") && (event.ctrlKey || event.metaKey || query === "");
    if (event.key === "ArrowDown") {
      event.preventDefault();
      moveHighlight(clampedIndex + 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      moveHighlight(clampedIndex - 1);
    } else if (event.key === "PageDown" || event.key === "PageUp") {
      event.preventDefault();
      moveHighlight(clampedIndex + (event.key === "PageDown" ? pageSize() : -pageSize()));
    } else if (jumpKey) {
      event.preventDefault();
      moveHighlight(event.key === "Home" ? 0 : results.length - 1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const command = results[clampedIndex];
      if (command !== undefined) runCommand(command);
    } else if (event.key === "Backspace" && picker !== undefined && query === "") {
      // Back from a picker to the command list.
      event.preventDefault();
      setPickerId(null);
      setHighlightedIndex(0);
    }
  }

  return (
    <div className={styles.palette}>
      <input
        type="text"
        className={styles.input}
        placeholder={picker?.placeholder ?? "Type a command…"}
        autoFocus
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setHighlightedIndex(0);
        }}
        onKeyDown={handleKeyDown}
        aria-label={pickerCommand !== undefined && picker !== undefined ? pickerCommand.label.replace(/…$/, "") : "Command palette"}
        role="combobox"
        aria-expanded={results.length > 0}
        aria-controls={listboxId}
        aria-activedescendant={activeDescendantId}
      />
      <ul
        ref={setListElement}
        className={styles.list}
        id={listboxId}
        role="listbox"
        // The options outside the window take their space as padding, on top of the list's own.
        style={
          windowing.windowed
            ? {
                paddingTop: `calc(var(--space-1) + ${windowing.padTop}px)`,
                paddingBottom: `calc(var(--space-1) + ${windowing.padBottom}px)`,
              }
            : undefined
        }
        onMouseMove={handleMouseMove} aria-label={picker === undefined ? "Command results" : "Results"}>
        {results.slice(windowing.start, windowing.end).map((command, offset) => {
          const index = windowing.start + offset;
          const heading = headings.get(index);
          const disabledReason = "disabledReason" in command ? command.disabledReason : undefined;
          const shortcut = "shortcut" in command ? command.shortcut : undefined;
          // A global shortcut's keys, or the keys the command's pane binds (#764).
          const keys = (shortcut !== undefined ? shortcutLabel(shortcut) : "keys" in command ? command.keys : undefined) || undefined;
          const optionId = `${listboxId}-option-${index}`;
          return (
            <Fragment key={command.id}>
              {heading !== undefined && (
                <li role="presentation" className={styles.groupHeading}>
                  {heading}
                </li>
              )}
              <ListRow
                id={optionId}
                className={disabledReason === undefined ? styles.option : `${styles.option} ${styles.unavailable}`}
                selected={index === clampedIndex}
                rowIndex={index}
                setSize={results.length}
                disabled={disabledReason !== undefined}
                describedBy={disabledReason === undefined ? undefined : `${optionId}-reason`}
                onClick={() => runCommand(command)}
              >
                <HighlightedLabel label={command.label} ranges={grouped ? [] : (matchItem(command, query)?.ranges ?? [])} />
                {disabledReason !== undefined && (
                  // Its description, not part of its name (`aria-hidden` still describes).
                  <span id={`${optionId}-reason`} className={styles.reason} aria-hidden="true">
                    {disabledReason}
                  </span>
                )}
                {keys !== undefined && (
                  // The command's shortcut (#646, #764), a hint beside its name.
                  <kbd className={styles.shortcut} aria-hidden="true">
                    {keys}
                  </kbd>
                )}
              </ListRow>
            </Fragment>
          );
        })}
        {results.length === 0 && (
          <li className={styles.empty}>{picker?.emptyText ?? "No matching commands"}</li>
        )}
      </ul>
      <div className={styles.footer} aria-hidden="true">
        {picker === undefined
          ? "↑↓ to move · Enter to run · Esc to close"
          : "↑↓ to move · Enter to go · Backspace to go back · Esc to close"}
      </div>
    </div>
  );
}

/**
 * `label` with the matched `ranges` marked (#644), in one span: the row is a flex container
 * with a gap, so loose text and `mark`s would each become a flex item, spaced apart.
 */
function HighlightedLabel({ label, ranges }: { label: string; ranges: readonly MatchRange[] }) {
  if (ranges.length === 0) return <span>{label}</span>;
  const parts: ReactNode[] = [];
  let position = 0;
  for (const [start, end] of ranges) {
    if (start > position) parts.push(label.slice(position, start));
    parts.push(
      <mark key={start} className={styles.match}>
        {label.slice(start, end)}
      </mark>,
    );
    position = end;
  }
  if (position < label.length) parts.push(label.slice(position));
  return <span>{parts}</span>;
}
