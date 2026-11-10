import { Fragment, useId, useMemo, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { ListRow } from "./primitives/ListRow";
import {
  COMMAND_GROUPS,
  filterAndSortCommands,
  recordCommandUsed,
  type Command,
  type CommandGroup,
  type PickerItem,
} from "../lib/commands";
import styles from "./CommandPalette.module.css";

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

  const pickerCommand = pickerId === null ? undefined : commands.find((command) => command.id === pickerId);
  const picker = pickerCommand?.picker;

  const grouped = picker === undefined && query.trim() === "";
  // Unfiltered: reorder into kind groups (stable within a group). Filtered: keep relevance order.
  // Keyboard order always equals displayed order, so the highlight index is over this list.
  const results = useMemo((): readonly (Command | PickerItem)[] => {
    const items = commands.find((command) => command.id === pickerId)?.picker?.items;
    if (items !== undefined) return items(query);
    const ranked = filterAndSortCommands(commands, query);
    if (query.trim() !== "") return ranked;
    return COMMAND_GROUPS.flatMap((group) => ranked.filter((command) => command.group === group));
  }, [commands, query, pickerId]);
  const clampedIndex = results.length === 0 ? 0 : Math.min(highlightedIndex, results.length - 1);
  const activeDescendantId =
    results.length === 0 ? undefined : `${listboxId}-option-${clampedIndex}`;

  function runCommand(command: Command | PickerItem) {
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
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setHighlightedIndex((prev) => Math.min(prev + 1, Math.max(results.length - 1, 0)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setHighlightedIndex((prev) => Math.max(prev - 1, 0));
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
      <ul className={styles.list} id={listboxId} role="listbox" aria-label={picker === undefined ? "Command results" : "Results"}>
        {results.map((command, index) => {
          const group = groupOf(command);
          const startsGroup = grouped && (index === 0 || groupOf(results[index - 1]) !== group);
          return (
            <Fragment key={command.id}>
              {startsGroup && (
                <li role="presentation" className={styles.groupHeading}>
                  {group}
                </li>
              )}
              <ListRow
                id={`${listboxId}-option-${index}`}
                className={styles.option}
                selected={index === clampedIndex}
                onClick={() => runCommand(command)}
              >
                {command.label}
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

/** A command's group; a picker's items have none. */
function groupOf(item: Command | PickerItem): CommandGroup | undefined {
  return "group" in item ? item.group : undefined;
}
