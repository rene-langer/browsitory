import { useState } from "react";
import { FormDialog } from "./primitives/FormDialog";
import { InlineError } from "./primitives/InlineError";
import styles from "./IdentityDialog.module.css";

/**
 * "Who are you?" (#689): a commit or amend was refused because git has no `user.name` and
 * `user.email`. Saves them for every repository (the global git config, as `git config --global`
 * does) or for this one only, then retries the commit when `retrying`.
 */
export function IdentityDialog({
  retrying,
  onSave,
  onCancel,
}: {
  /** Saving commits again (the commit that was refused); else it only saves. */
  retrying: boolean;
  /** `null` once saved, else the failure message, shown in the dialog. */
  onSave: (name: string, email: string, global: boolean) => Promise<string | null>;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [global, setGlobal] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const problem =
    name.trim() === "" || email.trim() === ""
      ? "Enter a name and an email address."
      : /[<>\n]/.test(name) || /[<>\s]/.test(email.trim())
        ? "The name and email can't contain < or >, and the email no spaces."
        : null;
  const submit = async () => {
    if (problem !== null || saving) return;
    setSaving(true);
    const failure = await onSave(name.trim(), email.trim(), global);
    setSaving(false);
    if (failure !== null) setError(failure);
  };
  return (
    <FormDialog ariaLabel="Set your git identity" onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <p className={styles.intro}>
          Git records a name and an email address with every commit, and none is set yet.
        </p>
        <label>
          Name
          <input value={name} autoComplete="name" onChange={(event) => setName(event.target.value)} />
        </label>
        <label>
          Email
          <input
            type="email"
            value={email}
            autoComplete="email"
            onChange={(event) => setEmail(event.target.value)}
          />
        </label>
        <fieldset className={styles.scope}>
          <legend>Use it for</legend>
          <label className={styles.choice}>
            <input type="radio" name="identity-scope" checked={global} onChange={() => setGlobal(true)} />
            All repositories (your global git config)
          </label>
          <label className={styles.choice}>
            <input type="radio" name="identity-scope" checked={!global} onChange={() => setGlobal(false)} />
            This repository only
          </label>
        </fieldset>
        {name !== "" && email !== "" && problem !== null && <p className={styles.problem}>{problem}</p>}
        <div className={styles.actions}>
          <button type="submit" disabled={problem !== null || saving}>
            {retrying ? "Save and commit" : "Save"}
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
        {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
      </form>
    </FormDialog>
  );
}
