import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import {
  DEFAULT_COMMIT_MESSAGE_SETTINGS,
  type AmendOptions,
  type CommitMessageSettings,
  type SignatureInput,
  type Trailer,
} from "../ipc/RepoClient";
import { coAuthorTrailers, formatPerson, parsePerson, sameEmail } from "../lib/people";
import { CoAuthorPicker } from "./CoAuthorPicker";
import { Field } from "./primitives/Field";
import { Toolbar } from "./primitives/Toolbar";
import { HookStatus } from "./HookStatus";
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
  /** Co-author chips; they become `Co-authored-by:` trailers on commit/amend. */
  coAuthors?: SignatureInput[];
  /** The pre-amend chips while amend mode has swapped them out (null/absent otherwise). */
  stashedCoAuthors?: SignatureInput[] | null;
  /** The amend Author field's text once edited (null/absent: HEAD's author). */
  amendAuthor?: string | null;
}

const NO_PEOPLE: SignatureInput[] = [];

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
  clearMessage = null,
  onClearMessageHandled,
  messageSettings = DEFAULT_COMMIT_MESSAGE_SETTINGS,
  loadRecentAuthors,
}: {
  /**
   * Resolves `true` once the commit landed. The message (and the co-author chips) are cleared
   * only then (and only if not edited meanwhile), so a failed commit — e.g. a failing pre-commit
   * hook — keeps them. `trailers` are the chips as `Co-authored-by` trailers.
   */
  onCommit: (message: string, trailers: Trailer[]) => Promise<boolean>;
  /** Recent authors for "Add co-author"; omit to hide co-authors. */
  loadRecentAuthors?: () => Promise<SignatureInput[]>;
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
    /** The commit concludes a clean merge and runs `pre-merge-commit` instead of `pre-commit`. */
    merging?: boolean;
    skip: boolean;
    onToggleSkip: (on: boolean) => void;
    running: { operationId: string; hook: string } | null;
    onCancel: () => void;
  };
  /**
   * One-shot request to clear the message, carrying the committed text (a commit retried from the
   * hook-failure dialog landed). Only a field (or stashed pre-amend draft) still holding exactly
   * that text is cleared, so text typed meanwhile survives. `onClearMessageHandled` then lets the
   * owner reset it.
   */
  clearMessage?: string | null;
  onClearMessageHandled?: () => void;
  /** The summary counter's soft limit and the body ruler (Preferences → Commit message). */
  messageSettings?: CommitMessageSettings;
  /** Amend-last-commit mode; omit to hide the toggle entirely. */
  amend?: {
    /** headId !== null && no operation in progress. */
    available: boolean;
    on: boolean;
    /** HEAD's message, loaded when `on` flips true; null while loading. */
    headMessage: string | null;
    pushed: boolean;
    onToggle: (on: boolean) => void;
    /** Resolves `true` once amended; Amend is disabled while it runs. */
    onAmend: (message: string, options: AmendOptions) => Promise<boolean>;
    /** HEAD's author, prefilled in the Author field; null when unknown (not in the graph). */
    headAuthor?: SignatureInput | null;
  };
}) {
  const [message, setMessage] = useState(initialDraft?.message ?? "");
  // Tracks the last `initialMessage` value that was auto-seeded into `message`, so we can tell
  // "user hasn't touched the field since it was seeded" (message === lastSeeded) apart from "user
  // edited it" (message !== lastSeeded) — and only ever overwrite the former.
  const [lastSeeded, setLastSeeded] = useState(initialDraft?.lastSeeded ?? "");

  const [stashedDraft, setStashedDraft] = useState<string | null>(initialDraft?.stashed ?? null);
  const [coAuthors, setCoAuthors] = useState<SignatureInput[]>(initialDraft?.coAuthors ?? NO_PEOPLE);
  const [stashedCoAuthors, setStashedCoAuthors] = useState<SignatureInput[] | null>(
    initialDraft?.stashedCoAuthors ?? null,
  );
  // The amend Author field's text; null shows HEAD's author.
  const [amendAuthor, setAmendAuthor] = useState<string | null>(initialDraft?.amendAuthor ?? null);
  const [authorError, setAuthorError] = useState(false);

  // Write-through rather than lifting the state itself: the owner only needs the value back on
  // the next mount, and making it controlled from `App` would re-render the whole workspace on
  // every keystroke.
  useEffect(() => {
    onDraftChange?.({ message, lastSeeded, stashed: stashedDraft, coAuthors, stashedCoAuthors, amendAuthor });
  }, [message, lastSeeded, stashedDraft, coAuthors, stashedCoAuthors, amendAuthor, onDraftChange]);

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
  // The co-author chips are swapped the same way: amend starts with none.
  useEffect(() => {
    if (amendOn && stashedDraft === null && headMessage !== null) {
      // Deliberate seed write: entering amend mode swaps the draft out for HEAD's message.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setStashedDraft(message);
      setMessage(headMessage);
      setStashedCoAuthors(coAuthors);
      setCoAuthors(NO_PEOPLE);
    } else if (!amendOn && stashedDraft !== null) {
      setMessage(stashedDraft);
      setStashedDraft(null);
      setCoAuthors(stashedCoAuthors ?? NO_PEOPLE);
      setStashedCoAuthors(null);
      setAmendAuthor(null);
      setAuthorError(false);
    }
  }, [amendOn, headMessage, stashedDraft, message, coAuthors, stashedCoAuthors]);

  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const focusReady = !amendOn || stashedDraft !== null;
  useEffect(() => {
    if (!focusMessage || !focusReady) return;
    textareaRef.current?.focus();
    onFocusMessageHandled?.();
  }, [focusMessage, focusReady, onFocusMessageHandled]);

  // Read by the clear request below, which must not re-run on every keystroke.
  const latest = useRef({ message, stashedDraft });
  useEffect(() => {
    latest.current = { message, stashedDraft };
  }, [message, stashedDraft]);

  useEffect(() => {
    if (clearMessage === null) return;
    // One-shot clear requested by the owner, acknowledged right away. The chips that went with
    // the committed message go too.
    const { message: shown, stashedDraft: stashed } = latest.current;
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setMessage((current) => (current === clearMessage ? "" : current));
    setStashedDraft((draft) => (draft === clearMessage ? "" : draft));
    if (shown === clearMessage) setCoAuthors(NO_PEOPLE);
    if (stashed === clearMessage) setStashedCoAuthors(NO_PEOPLE);
    setLastSeeded("");
    onClearMessageHandled?.();
  }, [clearMessage, onClearMessageHandled]);

  // True while a commit (and its hooks) is in flight, so Commit cannot be pressed twice now that
  // the message stays in the field until the commit lands.
  const [submitting, setSubmitting] = useState(false);

  // A merge/pick/revert (or squash) message is git's: no co-authors are added to it.
  const showCoAuthors = loadRecentAuthors !== undefined && initialMessage === undefined;
  const headAuthor = amend?.headAuthor ?? null;
  const headAuthorText = headAuthor === null ? "" : formatPerson(headAuthor);
  const authorText = amendAuthor ?? headAuthorText;

  const commitIfReady = () => {
    const submittedCoAuthors = coAuthors;
    const trailers = showCoAuthors ? coAuthorTrailers(submittedCoAuthors) : [];
    if (amendOn) {
      if (submitting || message.trim() === "") {
        return;
      }
      // Blank or unchanged keeps HEAD's author; anything else must be `Name <email>`.
      let author: SignatureInput | null = null;
      if (authorText.trim() !== "" && authorText.trim() !== headAuthorText) {
        author = parsePerson(authorText);
        if (author === null) {
          setAuthorError(true);
          return;
        }
      }
      // Leave message and stash alone: on success amend mode turns off and the stashed draft
      // is restored; on failure the typed message stays put.
      setSubmitting(true);
      void amend.onAmend(message, { author, trailers }).finally(() => setSubmitting(false));
      return;
    }
    if (disabled || submitting || message.trim() === "") {
      return;
    }
    const submitted = message;
    setSubmitting(true);
    void onCommit(submitted, trailers)
      .then((ok) => {
        if (!ok) return;
        // Only clear what was committed: text typed (or chips changed) while the commit (and its
        // hooks) ran stays.
        setMessage((current) => (current === submitted ? "" : current));
        setCoAuthors((current) => (current === submittedCoAuthors ? NO_PEOPLE : current));
        setLastSeeded("");
      })
      .finally(() => setSubmitting(false));
  };

  // Code points of the first line, as most tools count a summary.
  const summaryLength = [...message.split(/\r?\n/, 1)[0]].length;
  const summaryLimit = messageSettings.summaryLimit;
  const summaryOver = summaryLength > summaryLimit;
  // A column ruler only lines up in a monospace font, so the field switches to one while shown.
  const ruler = messageSettings.showRuler
    ? { "data-ruler": "true", style: { "--ruler-column": String(messageSettings.bodyRuler) } as CSSProperties }
    : {};

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
        {...ruler}
      />
      <span
        aria-label="Summary length"
        className={styles.counter}
        data-over={summaryOver ? "true" : undefined}
        title={summaryOver ? `Many tools cut summaries longer than ${summaryLimit} characters.` : undefined}
      >
        Summary {summaryLength}/{summaryLimit}
      </span>
      {showCoAuthors && (
        <div className={styles.coAuthors}>
          {coAuthors.length > 0 && (
            <ul className={styles.chips} aria-label="Co-authors">
              {coAuthors.map((person) => (
                <li key={person.email.toLowerCase()} className={styles.chip}>
                  <span>{formatPerson(person)}</span>
                  <button
                    type="button"
                    className={styles.chipRemove}
                    aria-label={`Remove co-author ${person.name}`}
                    onClick={() => setCoAuthors((current) => current.filter((p) => !sameEmail(p, person)))}
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
          )}
          <CoAuthorPicker
            load={loadRecentAuthors}
            exclude={coAuthors}
            onPick={(person) =>
              setCoAuthors((current) => (current.some((p) => sameEmail(p, person)) ? current : [...current, person]))
            }
          />
        </div>
      )}
      {amendOn && (
        <Field label="Author" error={authorError ? "Use the form Name <email>." : null} className={styles.authorField}>
          {(props) => (
            <input
              {...props}
              type="text"
              spellCheck={false}
              placeholder="Keep original author"
              value={authorText}
              onChange={(event) => {
                setAmendAuthor(event.target.value);
                setAuthorError(false);
              }}
            />
          )}
        </Field>
      )}
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
          title={`Skip the ${hooks.merging === true ? "pre-merge-commit" : "pre-commit"} and commit-msg hooks for the next commit (--no-verify)`}
        >
          <input type="checkbox" checked={hooks.skip} onChange={(event) => hooks.onToggleSkip(event.target.checked)} />
          Skip hooks
        </label>
      )}
      {amendOn && amend?.pushed === true && (
        <p className={styles.reason}>Already pushed — you'll need to force push.</p>
      )}
      {hooks?.running != null && <HookStatus running={hooks.running} onCancel={hooks.onCancel} />}
      <Toolbar>
        <button
          className={styles.commitButton}
          onClick={commitIfReady}
          disabled={submitting || message.trim() === "" || (!amendOn && disabled)}
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
