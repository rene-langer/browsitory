import styles from "./EditStopPanel.module.css";

/**
 * An edit stop's part of the commit dock (Edit commit… #882, Split commit… #884): the message the
 * commit is recommitted with and, for a split, Commit part with what to do. Its status, Continue
 * and Abort are the operation status strip's alone (#995).
 */
export function EditStopPanel({
  label,
  splitting,
  message,
  commitPart,
}: {
  /** "Editing 1a2b3c4" / "Splitting 1a2b3c4": the region's name. */
  label: string;
  /** A split: says how to make the parts (#994). */
  splitting: boolean;
  /** The message for the recommitted commit, editable. */
  message: { value: string; onChange: (value: string) => void };
  /**
   * "Commit part" commits what is staged as a part, under the message box's text; disabled while
   * the box is empty.
   */
  commitPart?: { onCommit: () => void; disabled: boolean };
}) {
  return (
    <section aria-label={label} className={styles.panel}>
      {splitting && (
        <p className={styles.instruction}>
          Unstage what belongs in a later part, write its message, then Commit part. Finish split commits the rest.
        </p>
      )}
      <label className={styles.message}>
        Commit message
        <textarea value={message.value} rows={4} spellCheck onChange={(event) => message.onChange(event.target.value)} />
      </label>
      {commitPart !== undefined && (
        <div>
          <button type="button" onClick={commitPart.onCommit} disabled={commitPart.disabled}>
            Commit part
          </button>
        </div>
      )}
    </section>
  );
}
