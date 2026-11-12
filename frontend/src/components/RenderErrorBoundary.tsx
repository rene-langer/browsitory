import { Component, Fragment, type ErrorInfo, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { copyText } from "../lib/clipboard";
import { renderErrorDetails, type CaughtRenderError } from "../lib/renderErrorDetails";
import styles from "./RenderErrorBoundary.module.css";

export type { CaughtRenderError };

interface BoundaryState {
  caught: CaughtRenderError | null;
  /** Bumped by a reset, as the children's key: they mount again from scratch. */
  generation: number;
}

/**
 * Catches an exception thrown while rendering its children (#846), so a bug in one view doesn't
 * blank the whole window: React would otherwise unmount everything. The app puts one around each
 * repository tab's workspace and one around the whole app (`App.tsx`).
 *
 * `reset` mounts the children again from scratch, which clears the state that threw: a
 * workspace reads its repository again.
 */
export class RenderErrorBoundary extends Component<
  {
    /** Called once per caught error, to log it. */
    onError: (caught: CaughtRenderError) => void;
    fallback: (caught: CaughtRenderError, reset: () => void) => ReactNode;
    children: ReactNode;
  },
  BoundaryState
> {
  state: BoundaryState = { caught: null, generation: 0 };

  static getDerivedStateFromError(error: unknown): Partial<BoundaryState> {
    return { caught: { error, componentStack: "" } };
  }

  componentDidCatch(error: unknown, info: ErrorInfo): void {
    const caught = { error, componentStack: info.componentStack ?? "" };
    this.setState({ caught });
    this.props.onError(caught);
  }

  private readonly reset = () => {
    this.setState((state) => ({ caught: null, generation: state.generation + 1 }));
  };

  render(): ReactNode {
    if (this.state.caught !== null) return this.props.fallback(this.state.caught, this.reset);
    return <Fragment key={this.state.generation}>{this.props.children}</Fragment>;
  }
}

/** The error's own text: an `Error`'s message, or the thrown value as text. */
function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * What shows in place of a view that threw (#846): what failed, the error, the next step, and
 * Reload, Copy details and (for a tab) Close tab.
 */
export function RenderErrorFallback({
  title,
  caught,
  hint,
  reloadLabel,
  onReload,
  onClose,
  getAppVersion,
  focusOnShow,
}: {
  /** What failed, as the heading: "Couldn't show …". */
  title: string;
  caught: CaughtRenderError;
  /** The next step, in plain words. */
  hint: string;
  reloadLabel: string;
  onReload: () => void;
  /** Renders "Close tab" when given. */
  onClose?: () => void;
  getAppVersion: () => Promise<string>;
  /**
   * Whether to move the focus to Reload when it was lost with the failed view (it fell back to
   * the page). False for a tab in the background.
   */
  focusOnShow: boolean;
}) {
  const headingId = useId();
  const reloadRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!focusOnShow) return;
    const focused = document.activeElement;
    if (focused === null || focused === document.body || !focused.isConnected) reloadRef.current?.focus();
    // Only when the fallback appears.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return (
    <div role="alert" aria-labelledby={headingId} className={styles.fallback}>
      <h2 id={headingId} className={styles.title}>
        {title}
      </h2>
      <p className={styles.message}>{errorMessage(caught.error)}</p>
      <p className={styles.hint}>{hint}</p>
      <div className={styles.actions}>
        <button ref={reloadRef} type="button" onClick={onReload}>
          {reloadLabel}
        </button>
        <CopyDetailsButton caught={caught} getAppVersion={getAppVersion} />
        {onClose !== undefined && (
          <button type="button" onClick={onClose}>
            Close tab
          </button>
        )}
      </div>
    </div>
  );
}

/** Copies {@link renderErrorDetails}, saying so on the button for a moment. */
function CopyDetailsButton({
  caught,
  getAppVersion,
}: {
  caught: CaughtRenderError;
  getAppVersion: () => Promise<string>;
}) {
  const [outcome, setOutcome] = useState<"copied" | "failed" | null>(null);
  useEffect(() => {
    if (outcome === null) return;
    const timer = setTimeout(() => setOutcome(null), 2000);
    return () => clearTimeout(timer);
  }, [outcome]);
  return (
    <button
      type="button"
      onClick={() => {
        getAppVersion()
          .catch(() => null)
          .then((version) => copyText(renderErrorDetails(caught, version)))
          .then(
            () => setOutcome("copied"),
            () => setOutcome("failed"),
          );
      }}
    >
      {outcome === "copied" ? "Copied" : outcome === "failed" ? "Couldn't copy" : "Copy details"}
    </button>
  );
}
