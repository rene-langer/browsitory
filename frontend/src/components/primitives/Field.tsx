import { useId, type ReactNode } from "react";
import styles from "./Field.module.css";

export interface FieldControlProps {
  id: string;
  required?: true;
  "aria-invalid"?: true;
  "aria-describedby"?: string;
}

/**
 * Labelled form field that wires its hint and error text to the control programmatically:
 * `<label for>`, `aria-describedby` (hint then error), `aria-invalid` while an error is shown and
 * `required`. The control is supplied as a render prop so any input/select/textarea (or a custom
 * control) can spread the returned props onto itself.
 */
export function Field({
  label,
  hint,
  error,
  required = false,
  className,
  children,
}: {
  label: ReactNode;
  hint?: ReactNode;
  /** Error text for this field; `null`/undefined when valid. */
  error?: string | null;
  required?: boolean;
  className?: string;
  children: (controlProps: FieldControlProps) => ReactNode;
}) {
  const id = useId();
  const hintId = hint !== undefined ? `${id}-hint` : undefined;
  const hasError = error !== null && error !== undefined && error !== "";
  const errorId = hasError ? `${id}-error` : undefined;
  const describedBy = [hintId, errorId].filter((value) => value !== undefined).join(" ");

  const controlProps: FieldControlProps = { id };
  if (required) controlProps.required = true;
  if (hasError) controlProps["aria-invalid"] = true;
  if (describedBy !== "") controlProps["aria-describedby"] = describedBy;

  return (
    <div className={className === undefined ? styles.field : `${styles.field} ${className}`}>
      <label htmlFor={id} className={styles.label}>
        {label}
        {required && (
          <span className={styles.required} aria-hidden="true">
            {" "}
            *
          </span>
        )}
      </label>
      {children(controlProps)}
      {hint !== undefined && (
        <span id={hintId} className={styles.hint}>
          {hint}
        </span>
      )}
      {hasError && (
        <span id={errorId} className={styles.error}>
          {error}
        </span>
      )}
    </div>
  );
}
