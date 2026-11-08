import { useState } from "react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import { InlineError } from "./primitives/InlineError";
import styles from "./PushBranchDialog.module.css";

/** `origin` if it exists, else the first remote, else "". */
function defaultRemote(remotes: RemoteInfo[]): string {
  if (remotes.some((remote) => remote.name === "origin")) return "origin";
  return remotes[0]?.name ?? "";
}

/**
 * Creates a lightweight or annotated tag on one commit, fixed when the dialog opens (a commit's
 * menu, the palette's "Create tag at HEAD…"/"Create tag at selected commit…"). Optionally pushes
 * the new tag to a remote once it exists.
 */
export function CreateTagDialog({
  shortId,
  summary,
  remotes,
  disabled,
  disabledReason,
  onCreate,
  onCancel,
}: {
  /** The target commit, as shown to the user. */
  shortId: string;
  summary: string | null;
  remotes: RemoteInfo[];
  /** A repository operation is running: creating is blocked until it ends. */
  disabled: boolean;
  disabledReason: string | null;
  /**
   * Resolves to `null` once the tag exists (the caller closes the dialog and starts the push
   * when `pushTo` is set), or to the failure message, shown inline with the input kept.
   */
  onCreate: (name: string, message: string | null, pushTo: string | null) => Promise<string | null>;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"lightweight" | "annotated">("lightweight");
  const [message, setMessage] = useState("");
  const [push, setPush] = useState(false);
  const [remoteName, setRemoteName] = useState(() => defaultRemote(remotes));
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const trimmedName = name.trim();
  const trimmedMessage = message.trim();
  const pushTo = push && remoteName !== "" ? remoteName : null;
  const blocked =
    disabled || submitting || trimmedName === "" || (kind === "annotated" && trimmedMessage === "");

  return (
    <FormDialog ariaLabel="Create tag" onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (blocked) return;
          setSubmitting(true);
          void onCreate(trimmedName, kind === "annotated" ? trimmedMessage : null, pushTo).then((failure) => {
            setSubmitting(false);
            setError(failure);
          });
        }}
      >
        <p className={styles.message}>
          Tag commit <code>{shortId}</code>
          {summary !== null && summary !== "" && <> {summary}</>}
        </p>
        <Field label="Tag name">
          {(props) => (
            <input
              {...props}
              data-autofocus
              value={name}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => {
                setName(event.target.value);
                setError(null);
              }}
            />
          )}
        </Field>
        <fieldset className={styles.choices}>
          <legend className={styles.legend}>Kind</legend>
          <label className={styles.checkbox}>
            <input type="radio" name="tag-kind" checked={kind === "lightweight"} onChange={() => setKind("lightweight")} />
            Lightweight tag
          </label>
          <label className={styles.checkbox}>
            <input type="radio" name="tag-kind" checked={kind === "annotated"} onChange={() => setKind("annotated")} />
            Annotated tag
          </label>
        </fieldset>
        {kind === "annotated" && (
          <Field label="Tag message">
            {(props) => <textarea {...props} value={message} onChange={(event) => setMessage(event.target.value)} />}
          </Field>
        )}
        {remotes.length > 0 && (
          <>
            <label className={styles.checkbox}>
              <input type="checkbox" checked={push} onChange={(event) => setPush(event.target.checked)} />
              Push the tag after creating it
            </label>
            {push && remotes.length > 1 && (
              <Field label="Remote">
                {(props) => (
                  <select {...props} value={remoteName} onChange={(event) => setRemoteName(event.target.value)}>
                    {remotes.map((remote) => (
                      <option key={remote.name} value={remote.name}>
                        {remote.name}
                      </option>
                    ))}
                  </select>
                )}
              </Field>
            )}
          </>
        )}
        {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
        <div className={styles.actions}>
          <button type="submit" disabled={blocked} title={disabled ? (disabledReason ?? undefined) : undefined}>
            {pushTo === null ? "Create tag" : `Create and push to ${pushTo}`}
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
    </FormDialog>
  );
}
