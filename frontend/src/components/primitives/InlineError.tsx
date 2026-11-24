import { useEffect, useState, type ReactNode } from "react";
import { X } from "lucide-react";
import { copyText } from "../../lib/clipboard";
import { withoutLibgit2Codes } from "../../lib/errorMessages";
import { redactCredentials } from "../../lib/redact";
import styles from "./InlineError.module.css";

/** A button in the banner's row, before Details and dismiss (#984). */
export interface InlineErrorAction {
  label: string;
  onClick: () => void;
  /** The next step the banner suggests first: drawn filled. */
  primary?: boolean;
}

/**
 * Shared dismissible error display for both the global banner (`App.tsx`) and every component's
 * own inline error (`RemotePanel`'s `addError`, `DiffPane`'s fetch failures, …). Before this,
 * every call site rendered a bare `<p role="alert">{message}</p>` with no way to clear it short
 * of the next successful action of the same kind — a stale error could sit on screen
 * indefinitely once the user moved on to something else (see issue #30/UX-002). Centralizing the
 * dismiss button here means every error surface gets the same affordance for free, and the same
 * markup instead of a copy-pasted `<button>` per call site.
 *
 * One row (#984): the message, then the buttons (`actions`, Retry, Details, Copy details), then
 * dismiss. libgit2's class and code are left out of the shown message (#997); Copy details keeps
 * them. The global banners also offer "Copy details" (`copyDetails`, #663): the message and hint
 * as text, for a bug report; and the errors an action replaced before this one was dismissed
 * (`earlier`), collapsed under it. `children` go under the row (the repositories a folder holds,
 * #973).
 */
export function InlineError({
  message,
  onDismiss,
  onRetry,
  hint,
  className,
  copyDetails = false,
  details,
  earlier = [],
  actions = [],
  onDetails,
  children,
}: {
  message: string;
  onDismiss: () => void;
  /** When given, renders a Retry button that re-runs the failed action. */
  onRetry?: () => void;
  /** Plain-language next step shown under the (often technical) message. */
  hint?: string;
  className?: string;
  /** Renders a "Copy details" button that copies the message and hint (#663). */
  copyDetails?: boolean;
  /** What Copy details adds after them: Git's own message where `message` rephrases it. */
  details?: string;
  /** Earlier errors, newest first, that this one replaced (#663). */
  earlier?: readonly string[];
  /** Next steps, in order, before Retry and Details (#984). */
  actions?: readonly InlineErrorAction[];
  /** Renders a "Details" button: the operation log, which has the full text (#984). */
  onDetails?: () => void;
  children?: ReactNode;
}) {
  return (
    <div role="alert" className={className === undefined ? styles.error : `${styles.error} ${className}`}>
      <div className={styles.row}>
        <span className={styles.message}>
          {withoutLibgit2Codes(message)}
          {hint !== undefined && <span className={styles.hint}>{hint}</span>}
        </span>
        {actions.map((action) => (
          <button
            key={action.label}
            type="button"
            className={action.primary === true ? `${styles.action} ${styles.primary}` : styles.action}
            onClick={action.onClick}
          >
            {action.label}
          </button>
        ))}
        {onRetry !== undefined && (
          <button type="button" className={styles.action} onClick={onRetry}>
            Retry
          </button>
        )}
        {onDetails !== undefined && (
          <button type="button" className={styles.action} onClick={onDetails}>
            Details
          </button>
        )}
        {copyDetails && <CopyDetailsButton text={errorDetails(message, hint, earlier, details)} />}
        <button type="button" className={styles.dismiss} aria-label="Dismiss error" onClick={onDismiss}>
          <X size={12} aria-hidden="true" />
        </button>
      </div>
      {children}
      {earlier.length > 0 && (
        <details className={styles.earlier}>
          <summary>
            {earlier.length} earlier {earlier.length === 1 ? "error" : "errors"}
          </summary>
          <ul>
            {earlier.map((text, index) => (
              <li key={index}>{withoutLibgit2Codes(text)}</li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

/** What "Copy details" copies: the message, its hint, the details, then the earlier errors. */
function errorDetails(
  message: string,
  hint: string | undefined,
  earlier: readonly string[] = [],
  details?: string,
): string {
  const lines = [message];
  if (hint !== undefined) lines.push(hint);
  if (details !== undefined) lines.push(details);
  if (earlier.length > 0) lines.push("", "Earlier errors:", ...earlier.map((text) => `- ${text}`));
  return lines.join("\n");
}

/**
 * Copies `text` without credentials (#1056: it goes into bug reports), saying so on the button
 * for a moment (the banner may sit outside a workspace).
 */
export function CopyDetailsButton({ text, className = styles.action }: { text: string; className?: string }) {
  const [outcome, setOutcome] = useState<"copied" | "failed" | null>(null);
  useEffect(() => {
    if (outcome === null) return;
    const timer = setTimeout(() => setOutcome(null), 2000);
    return () => clearTimeout(timer);
  }, [outcome]);
  return (
    <button
      type="button"
      className={className}
      onClick={() => {
        copyText(redactCredentials(text)).then(
          () => setOutcome("copied"),
          () => setOutcome("failed"),
        );
      }}
    >
      {outcome === "copied" ? "Copied" : outcome === "failed" ? "Couldn't copy" : "Copy details"}
    </button>
  );
}
