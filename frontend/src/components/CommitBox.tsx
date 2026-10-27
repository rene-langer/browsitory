import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Toolbar } from "./primitives/Toolbar";
import { runningHookLabel } from "../lib/hookFailure";
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
  showAbort = initialMessage !== undefined,
  initialDraft,
  onDraftChange,
  amend,
  focusMessage = false,
  onFocusMessageHandled,
  hooks,
  clearMessage = false,
  onClearMessageHandled,
}: {
  /**
   * Resolves `true` once the commit landed. The message is cleared only then (and only if it was
   * not edited meanwhile), so a failed commit — e.g. a failing pre-commit hook — keeps it.
   */
  onCommit: (message: string) => Promise<boolean>;
  disabled: boolean;
  /** Visible explanation shown while `disabled` blocks Commit. */
  disabledReason?: string;
  onAbortMerge: () => void;
  abortLabel?: string;
  initialMessage?: string;
  /** Whether to offer `abortLabel`; defaults to "a message was seeded" (a merge/pick/revert). */
  showAbort?: boolean;
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
  /** Commit hooks: the "Skip hooks" toggle (only when installed) and the running hook's status. */
  hooks?: {
    installed: boolean;
    skip: boolean;
    onToggleSkip: (on: boolean) => void;
    running: { operationId: string; hook: string } | null;
    onCancel: () => void;
  };
  /**
   * One-shot request to clear the message (a commit retried from the hook-failure dialog landed);
   * `onClearMessageHandled` then lets the owner reset it.
   */
  clearMessage?: boolean;
  onClearMessageHandled?: () => void;
  /** Amend-last-commit mode; omit to hide the toggle entirely. */
  amend?: {
    /** headId !== null && no operation in progress. */
    available: boolean;
    on: boolean;
    /** HEAD's message, loaded when `on` flips true; null while loading. */
    headMessage: string | null;
    pushed: boolean;
    onToggle: (on: boolean) => void;
    onAmend: (message: string) => unknown;
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

  useEffect(() => {
    if (!clearMessage) return;
    // One-shot clear requested by the owner, acknowledged right away.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setMessage("");
    setLastSeeded("");
    onClearMessageHandled?.();
  }, [clearMessage, onClearMessageHandled]);

  // True while a commit (and its hooks) is in flight, so Commit cannot be pressed twice now that
  // the message stays in the field until the commit lands.
  const [submitting, setSubmitting] = useState(false);
  // Keyed by operation id (like `TransferPanel`), so Cancel re-arms for the next hook run.
  const [cancellingId, setCancellingId] = useState<string | null>(null);

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
    if (disabled || submitting || message.trim() === "") {
      return;
    }
    const submitted = message;
    setSubmitting(true);
    void onCommit(submitted)
      .then((ok) => {
        if (!ok) return;
        // Only clear what was committed: text typed while the commit (and its hooks) ran stays.
        setMessage((current) => (current === submitted ? "" : current));
        setLastSeeded("");
      })
      .finally(() => setSubmitting(false));
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
      {hooks?.installed === true && (
        <label
          className={styles.amendToggle}
          title="Skip the pre-commit and commit-msg hooks for the next commit (--no-verify)"
        >
          <input type="checkbox" checked={hooks.skip} onChange={(event) => hooks.onToggleSkip(event.target.checked)} />
          Skip hooks
        </label>
      )}
      {amendOn && amend?.pushed === true && (
        <p className={styles.reason}>Already pushed — you'll need to force push.</p>
      )}
      {hooks?.running != null && (
        <p role="status" className={styles.hookStatus}>
          {runningHookLabel(hooks.running.hook)}{" "}
          <button
            type="button"
            disabled={cancellingId === hooks.running.operationId}
            onClick={() => {
              setCancellingId(hooks.running?.operationId ?? null);
              hooks.onCancel();
            }}
          >
            {cancellingId === hooks.running.operationId ? "Cancelling…" : "Cancel"}
          </button>
        </p>
      )}
      <Toolbar>
        <button
          className={styles.commitButton}
          onClick={commitIfReady}
          disabled={amendOn ? message.trim() === "" : disabled || submitting || message.trim() === ""}
          aria-describedby={
            !amendOn && disabled && disabledReason !== undefined ? "commit-disabled-reason" : undefined
          }
        >
          {amendOn ? "Amend" : "Commit"}
        </button>
        {showAbort && <button onClick={onAbortMerge}>{abortLabel}</button>}
      </Toolbar>
    </div>
  );
}
