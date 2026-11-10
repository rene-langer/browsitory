import { useState } from "react";
import { repoNameOf } from "../lib/repoName";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import { InlineError } from "./primitives/InlineError";
import styles from "./PushBranchDialog.module.css";

/** The longest tab name the dialog takes. */
export const MAX_TAB_NAME_LENGTH = 80;

/**
 * Names a repository's tab (#625): the tab menu's and the palette's "Rename tab…". An empty name
 * restores the folder name. The name is kept per repository path, so it comes back with the tab.
 */
export function RenameTabDialog({
  path,
  alias,
  onRename,
  onCancel,
}: {
  path: string;
  /** The current custom name; `null` when the tab shows its folder name. */
  alias: string | null;
  /** Resolves once the name is stored (the caller closes the dialog); rejects with the failure. */
  onRename: (alias: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [name, setName] = useState(alias ?? "");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const folderName = repoNameOf(path);

  return (
    <FormDialog ariaLabel="Rename tab" onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (saving) return;
          setSaving(true);
          onRename(name).catch((failure: unknown) => {
            setSaving(false);
            setError(`Couldn't save the tab name: ${failure instanceof Error ? failure.message : String(failure)}`);
          });
        }}
      >
        <p className={styles.message} title={path}>
          {path}
        </p>
        <Field label="Tab name" hint={`Leave empty to show the folder name, ${folderName}.`}>
          {(props) => (
            <input
              {...props}
              data-autofocus
              value={name}
              placeholder={folderName}
              maxLength={MAX_TAB_NAME_LENGTH}
              autoComplete="off"
              spellCheck={false}
              onFocus={(event) => event.currentTarget.select()}
              onChange={(event) => {
                setName(event.target.value);
                setError(null);
              }}
            />
          )}
        </Field>
        {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
        <div className={styles.actions}>
          <button type="submit" disabled={saving}>
            Rename
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
    </FormDialog>
  );
}
