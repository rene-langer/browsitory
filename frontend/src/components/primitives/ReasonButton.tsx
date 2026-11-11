import { useId, type ButtonHTMLAttributes, type ReactNode } from "react";
import styles from "./ReasonButton.module.css";

/**
 * A toolbar button that says why it can't run (#664, content guidelines: a disabled action says
 * why in visible text tied with `aria-describedby`, `title` alone is not enough). With a `reason`
 * it is `aria-disabled` rather than `disabled`, so it stays focusable: the reason shows next to
 * it while it is hovered or focused, and is its accessible description. A click does nothing.
 * Without a `reason` it is a plain button (its `title`, if any, kept).
 */
export function ReasonButton({
  reason,
  onClick,
  title,
  children,
  className,
  "aria-describedby": describedBy,
  ...rest
}: Omit<ButtonHTMLAttributes<HTMLButtonElement>, "disabled"> & {
  /** Why it can't run now; `null` enables it. */
  reason: string | null;
  children: ReactNode;
}) {
  const reasonId = useId();
  const blocked = reason !== null;
  return (
    <span className={styles.wrap}>
      <button
        type="button"
        {...rest}
        className={className}
        title={blocked ? undefined : title}
        aria-disabled={blocked ? true : undefined}
        aria-describedby={blocked ? [describedBy, reasonId].filter(Boolean).join(" ") : describedBy}
        onClick={(event) => {
          if (blocked) {
            event.preventDefault();
            return;
          }
          onClick?.(event);
        }}
      >
        {children}
      </button>
      {blocked && <DisabledReasonTip id={reasonId} reason={reason} />}
    </span>
  );
}

/**
 * The reason a control is disabled, shown below it while its wrapper (`reasonTipHost`) is hovered
 * or holds focus (`ReasonButton.module.css`'s `wrap`), and named by the control's `aria-describedby`.
 */
export function DisabledReasonTip({ id, reason }: { id: string; reason: string }) {
  return (
    <span id={id} role="tooltip" className={styles.tip}>
      {reason}
    </span>
  );
}
