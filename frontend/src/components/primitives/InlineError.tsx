import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { copyText } from "../../lib/clipboard";
import styles from "./InlineError.module.css";

/**
 * Shared dismissible error display for both the global banner (`App.tsx`) and every component's
 * own inline error (`RemotePanel`'s `addError`, `DiffPane`'s fetch failures, …). Before this,
 * every call site rendered a bare `<p role="alert">{message}</p>` with no way to clear it short
 * of the next successful action of the same kind — a stale error could sit on screen
 * indefinitely once the user moved on to something else (see issue #30/UX-002). Centralizing the
 * dismiss button here means every error surface gets the same affordance for free, and the same
 * markup instead of a copy-pasted `<button>` per call site.
 *
 * The global banners also offer "Copy details" (`copyDetails`, #663): the message and hint as
 * text, for a bug report; and the errors an action replaced before this one was dismissed
 * (`earlier`), collapsed under it.
 */
export function InlineError({
  message,
  onDismiss,
  onRetry,
  hint,
  className,
  copyDetails = false,
  earlier = [],
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
  /** Earlier errors, newest first, that this one replaced (#663). */
  earlier?: readonly string[];
}) {
  return (
    <div role="alert" className={className === undefined ? styles.error : `${styles.error} ${className}`}>
      <div className={styles.row}>
        <span className={styles.message}>
          {message}
          {hint !== undefined && <span className={styles.hint}>{hint}</span>}
        </span>
        {onRetry !== undefined && (
          <button type="button" className={styles.action} onClick={onRetry}>
            Retry
          </button>
        )}
        {copyDetails && <CopyDetailsButton text={errorDetails(message, hint, earlier)} />}
        <button type="button" className={styles.dismiss} aria-label="Dismiss error" onClick={onDismiss}>
          <X size={12} aria-hidden="true" />
        </button>
      </div>
      {earlier.length > 0 && (
        <details className={styles.earlier}>
          <summary>
            {earlier.length} earlier {earlier.length === 1 ? "error" : "errors"}
          </summary>
          <ul>
            {earlier.map((text, index) => (
              <li key={index}>{text}</li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

/** What "Copy details" copies: the message, its hint, then the earlier errors. */
function errorDetails(message: string, hint: string | undefined, earlier: readonly string[] = []): string {
  const lines = [message];
  if (hint !== undefined) lines.push(hint);
  if (earlier.length > 0) lines.push("", "Earlier errors:", ...earlier.map((text) => `- ${text}`));
  return lines.join("\n");
}

/** Copies `text`, saying so on the button for a moment (the banner may sit outside a workspace). */
function CopyDetailsButton({ text }: { text: string }) {
  const [outcome, setOutcome] = useState<"copied" | "failed" | null>(null);
  useEffect(() => {
    if (outcome === null) return;
    const timer = setTimeout(() => setOutcome(null), 2000);
    return () => clearTimeout(timer);
  }, [outcome]);
  return (
    <button
      type="button"
      className={styles.action}
      onClick={() => {
        copyText(text).then(
          () => setOutcome("copied"),
          () => setOutcome("failed"),
        );
      }}
    >
      {outcome === "copied" ? "Copied" : outcome === "failed" ? "Couldn't copy" : "Copy details"}
    </button>
  );
}
