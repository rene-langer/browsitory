import { useEffect, useRef, useState, type KeyboardEvent } from "react";
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
  /**
   * The pre-amend draft while amend mode has swapped it out for HEAD's message (null/absent
   * otherwise). Persisted so a remount while amending can't lose it.
   */
  stashed?: string | null;
}

export function CommitBox({
  onCommit,
  disabled,
  disabledReason,
  onAbortMerge,
  abortLabel = "Abort merge",
  initialMessage,
  initialDraft,
  onDraftChange,
  amend,
  focusMessage = false,
  onFocusMessageHandled,
}: {
  onCommit: (message: string) => void;
  disabled: boolean;
  /** Visible explanation shown while `disabled` blocks Commit. */
  disabledReason?: string;
  onAbortMerge: () => void;
  abortLabel?: string;
  initialMessage?: string;
  /**
   * Draft to start from on mount (read once, like `useState`'s initial value). `App` keeps one per
   * repo so switching tabs — which unmounts the inactive tab's whole workspace (PERF-001) — or
   * viewing a commit and coming back doesn't throw away a half-typed message.
   */
  initialDraft?: CommitDraft;
  /** Called with the current draft whenever it changes, so the owner can hand it back on remount. */
  onDraftChange?: (draft: CommitDraft) => void;
  /**
   * One-shot request to focus the message field (e.g. "Edit commit message…"). Honored once the
   * amend prefill, if amending, is in; `onFocusMessageHandled` then lets the owner clear it.
   */
  focusMessage?: boolean;
  onFocusMessageHandled?: () => void;
  /** Amend-last-commit mode; omit to hide the toggle entirely. */
  amend?: {
    /** headId !== null && no operation in progress. */
    available: boolean;
    on: boolean;
    /** HEAD's message, loaded when `on` flips true; null while loading. */
    headMessage: string | null;
    pushed: boolean;
    onToggle: (on: boolean) => void;
    onAmend: (message: string) => void;
  };
}) {
  const [message, setMessage] = useState(initialDraft?.message ?? "");
  // Tracks the last `initialMessage` value that was auto-seeded into `message`, so we can tell
  // "user hasn't touched the field since it was seeded" (message === lastSeeded) apart from "user
  // edited it" (message !== lastSeeded) — and only ever overwrite the former.
  const [lastSeeded, setLastSeeded] = useState(initialDraft?.lastSeeded ?? "");

  const [stashedDraft, setStashedDraft] = useState<string | null>(initialDraft?.stashed ?? null);

  // Write-through rather than lifting the state itself: the owner only needs the value back on
  // the next mount, and making it controlled from `App` would re-render the whole workspace on
  // every keystroke.
  useEffect(() => {
    onDraftChange?.({ message, lastSeeded, stashed: stashedDraft });
  }, [message, lastSeeded, stashedDraft, onDraftChange]);

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

  // Entering amend mode swaps the typed draft out for HEAD's message; leaving it puts the draft
  // back untouched, so toggling never loses a half-typed message.
  // Amend mode is only effective while it's actually available (e.g. not mid-merge).
  const amendOn = amend !== undefined && amend.on && amend.available;
  const headMessage = amend?.headMessage ?? null;
  useEffect(() => {
    if (amendOn && stashedDraft === null && headMessage !== null) {
      // Deliberate seed write: entering amend mode swaps the draft out for HEAD's message.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setStashedDraft(message);
      setMessage(headMessage);
    } else if (!amendOn && stashedDraft !== null) {
      setMessage(stashedDraft);
      setStashedDraft(null);
    }
  }, [amendOn, headMessage, stashedDraft, message]);

  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const focusReady = !amendOn || stashedDraft !== null;
  useEffect(() => {
    if (!focusMessage || !focusReady) return;
    textareaRef.current?.focus();
    onFocusMessageHandled?.();
  }, [focusMessage, focusReady, onFocusMessageHandled]);

  const commitIfReady = () => {
    if (amendOn) {
      if (message.trim() === "") {
        return;
      }
      // Leave message and stash alone: on success amend mode turns off and the stashed draft
      // is restored; on failure the typed message stays put.
      amend.onAmend(message);
      return;
    }
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
        ref={textareaRef}
        className={styles.textarea}
        value={message}
        onChange={(event) => setMessage(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Commit message"
        aria-label="Commit message"
      />
      {!amendOn && disabled && disabledReason !== undefined && (
        <p id="commit-disabled-reason" className={styles.reason}>
          {disabledReason}
        </p>
      )}
      {amend?.available === true && (
        <label className={styles.amendToggle}>
          <input
            type="checkbox"
            checked={amendOn}
            onChange={(event) => amend.onToggle(event.target.checked)}
          />
          Amend last commit
        </label>
      )}
      {amendOn && amend?.pushed === true && (
        <p className={styles.reason}>Already pushed — you'll need to force push.</p>
      )}
      <Toolbar>
        <button
          onClick={commitIfReady}
          disabled={amendOn ? message.trim() === "" : disabled || message.trim() === ""}
          aria-describedby={
            !amendOn && disabled && disabledReason !== undefined ? "commit-disabled-reason" : undefined
          }
        >
          {amendOn ? "Amend" : "Commit"}
        </button>
        {initialMessage !== undefined && <button onClick={onAbortMerge}>{abortLabel}</button>}
      </Toolbar>
    </div>
  );
}
