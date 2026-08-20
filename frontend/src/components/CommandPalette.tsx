import { useId, useMemo, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { ListRow } from "./primitives/ListRow";
import { filterAndSortCommands, recordCommandUsed, type Command } from "../lib/commands";
import styles from "./CommandPalette.module.css";

export function CommandPalette({ commands, onRun }: { commands: Command[]; onRun: () => void }) {
  const [query, setQuery] = useState("");
  const [highlightedIndex, setHighlightedIndex] = useState(0);
  const listboxId = useId();

  const results = useMemo(() => filterAndSortCommands(commands, query), [commands, query]);
  const clampedIndex = results.length === 0 ? 0 : Math.min(highlightedIndex, results.length - 1);
  const activeDescendantId =
    results.length === 0 ? undefined : `${listboxId}-option-${clampedIndex}`;

  function runCommand(command: Command) {
    recordCommandUsed(command.id);
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
    }
  }

  return (
    <div className={styles.palette}>
      <input
        type="text"
        className={styles.input}
        placeholder="Type a command…"
        autoFocus
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setHighlightedIndex(0);
        }}
        onKeyDown={handleKeyDown}
        aria-label="Command palette"
        role="combobox"
        aria-expanded={results.length > 0}
        aria-controls={listboxId}
        aria-activedescendant={activeDescendantId}
      />
      <ul className={styles.list} id={listboxId} role="listbox" aria-label="Command results">
        {results.map((command, index) => (
          <ListRow
            key={command.id}
            id={`${listboxId}-option-${index}`}
            selected={index === clampedIndex}
            onClick={() => runCommand(command)}
          >
            {command.label}
          </ListRow>
        ))}
        {results.length === 0 && <li className={styles.empty}>No matching commands</li>}
      </ul>
    </div>
  );
}
