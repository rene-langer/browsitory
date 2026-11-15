import { useId, useState } from "react";
import type { BranchInfo } from "../ipc/RepoClient";
import { describedBy } from "../lib/ariaDescribedBy";
import { BlockedReason } from "./primitives/BlockedReason";
import { DialogActions } from "./primitives/DialogActions";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import { InlineError } from "./primitives/InlineError";
import styles from "./PushBranchDialog.module.css";

// Rendered once outside the fields: a `<datalist>` inside a label's text breaks WebKitWebDriver's
// text reads (see `PullRequestPanel`).
const BRANCH_SUGGESTIONS_ID = "new-worktree-branches";

/**
 * The Worktrees section's "+" (#1011): checks a branch out in a folder of its own. An existing
 * branch is checked out as it is; a new one starts at the start point. A failure stays in the
 * dialog with what was typed (#30/UX-002); the caller closes it once the worktree exists.
 */
export function NewWorktreeDialog({
  branches,
  disabled,
  disabledReason,
  onCreate,
  onCancel,
}: {
  /** Suggested in the Branch field. */
  branches: BranchInfo[];
  /** A repository operation is running: creating waits until it ends. */
  disabled: boolean;
  disabledReason: string | null;
  /** Resolves to `null` once created (the dialog closes), or to the failure, shown inline. */
  onCreate: (name: string, path: string, branch: string, startPoint: string | null) => Promise<string | null>;
  onCancel: () => void;
}) {
  const reasonId = useId();
  const [branch, setBranch] = useState("");
  const [startPoint, setStartPoint] = useState("");
  const [path, setPath] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const busyReason = disabled ? disabledReason : null;

  const values = { branch: branch.trim(), path: path.trim(), name: name.trim() };
  const missing = values.branch === "" || values.path === "" || values.name === "";
  const blocked = disabled || submitting || missing;
  // Why Create is off, in view under the form (#1014).
  const reason = busyReason ?? (missing ? "Enter a branch, a worktree path and a worktree name." : null);
  const edited = (set: (value: string) => void) => (value: string) => {
    set(value);
    setError(null);
  };

  return (
    <FormDialog title="New worktree" onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (blocked) return;
          setSubmitting(true);
          void onCreate(values.name, values.path, values.branch, startPoint.trim() || null).then((failure) => {
            setSubmitting(false);
            setError(failure);
          });
        }}
      >
        <Field label="Branch" hint="An existing branch, or the name of a new one.">
          {(props) => (
            <input
              {...props}
              data-autofocus
              list={BRANCH_SUGGESTIONS_ID}
              value={branch}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => edited(setBranch)(event.target.value)}
            />
          )}
        </Field>
        <Field label="Start point" hint="Where a new branch starts, such as HEAD or main. An existing branch doesn't need one.">
          {(props) => (
            <input
              {...props}
              value={startPoint}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => edited(setStartPoint)(event.target.value)}
            />
          )}
        </Field>
        <Field
          label="Worktree path"
          hint="The folder the worktree's files go in. A relative path starts at this repository's folder."
        >
          {(props) => (
            <input
              {...props}
              value={path}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => edited(setPath)(event.target.value)}
            />
          )}
        </Field>
        <Field label="Worktree name" hint="Git's name for the worktree, usually the folder's name.">
          {(props) => (
            <input
              {...props}
              value={name}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => edited(setName)(event.target.value)}
            />
          )}
        </Field>
        {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
        <BlockedReason id={reasonId} reason={reason} />
        <DialogActions>
          <button
            type="submit"
            data-tone="primary"
            disabled={blocked}
            title={busyReason ?? undefined}
            aria-describedby={describedBy(reasonId, reason)}
          >
            Create worktree
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </DialogActions>
      </form>
      <datalist id={BRANCH_SUGGESTIONS_ID}>
        {branches.map((item) => (
          <option key={item.name} value={item.name} />
        ))}
      </datalist>
    </FormDialog>
  );
}
