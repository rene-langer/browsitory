import { useEffect, useState, type KeyboardEvent } from "react";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./CommitBox.module.css";

/**
 * Everything `CommitBox` needs to pick up where it left off after a remount: the typed message
 * and the merge pre-fill it last auto-seeded (see `lastSeeded` below — restoring only the message
 * would make a restored merge pre-fill look like a user edit, so it would never be cleared).
 */
export interface CommitDraft {
  message: string;
  lastSeeded: string;
}

export function CommitBox({
  onCommit,
  disabled,
  disabledReason,
  onAbortMerge,
  initialMessage,
  initialDraft,
  onDraftChange,
}: {
  onCommit: (message: string) => void;
  disabled: boolean;
  /** Visible explanation shown while `disabled` blocks Commit. */
  disabledReason?: string;
  onAbortMerge: () => void;
  initialMessage?: string;
  /**
   * Draft to start from on mount (read once, like `useState`'s initial value). `App` keeps one per
   * repo so switching tabs — which unmounts the inactive tab's whole workspace (PERF-001) — or
   * viewing a commit and coming back doesn't throw away a half-typed message.
   */
  initialDraft?: CommitDraft;
  /** Called with the current draft whenever it changes, so the owner can hand it back on remount. */
  onDraftChange?: (draft: CommitDraft) => void;
}) {
  const [message, setMessage] = useState(initialDraft?.message ?? "");
  // Tracks the last `initialMessage` value that was auto-seeded into `message`, so we can tell
  // "user hasn't touched the field since it was seeded" (message === lastSeeded) apart from "user
  // edited it" (message !== lastSeeded) — and only ever overwrite the former.
  const [lastSeeded, setLastSeeded] = useState(initialDraft?.lastSeeded ?? "");

  // Write-through rather than lifting the state itself: the owner only needs the value back on
  // the next mount, and making it controlled from `App` would re-render the whole workspace on
  // every keystroke.
  useEffect(() => {
    onDraftChange?.({ message, lastSeeded });
  }, [message, lastSeeded, onDraftChange]);

  // Seeds the field when a merge starts or when the pre-fill message changes to a different
  // merge's message, and clears it when a merge ends — but in every case only if the field still
  // holds exactly what was last auto-seeded, so a user's own edit is never clobbered.
  useEffect(() => {
    if (
      initialMessage !== undefined &&
      message === lastSeeded &&
      initialMessage !== lastSeeded
    ) {
      // Deliberate seed write, not a synchronization loop.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setMessage(initialMessage);
      setLastSeeded(initialMessage);
    } else if (initialMessage === undefined && message === lastSeeded && lastSeeded !== "") {
      // Merge ended (abort or otherwise) — clear the pre-fill, but only if the user hasn't typed
      // something different in the meantime.
      setMessage("");
      setLastSeeded("");
    }
  }, [initialMessage, message, lastSeeded]);

  const commitIfReady = () => {
    if (disabled || message.trim() === "") {
      return;
    }
    onCommit(message);
    setMessage("");
    setLastSeeded("");
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      commitIfReady();
    }
  };

  return (
    <div>
      <textarea
        className={styles.textarea}
        value={message}
        onChange={(event) => setMessage(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Commit message"
        aria-label="Commit message"
      />
      {disabled && disabledReason !== undefined && (
        <p id="commit-disabled-reason" className={styles.reason}>
          {disabledReason}
        </p>
      )}
      <Toolbar>
        <button
          onClick={commitIfReady}
          disabled={disabled || message.trim() === ""}
          aria-describedby={disabled && disabledReason !== undefined ? "commit-disabled-reason" : undefined}
        >
          Commit
        </button>
        {initialMessage !== undefined && <button onClick={onAbortMerge}>Abort merge</button>}
      </Toolbar>
    </div>
  );
}
