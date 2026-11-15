import { useId, useState } from "react";
import type { CreatePullRequest } from "../ipc/RepoClient";
import { describedBy } from "../lib/ariaDescribedBy";
import { BlockedReason } from "./primitives/BlockedReason";
import { DialogActions } from "./primitives/DialogActions";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import { InlineError } from "./primitives/InlineError";
import styles from "./PushBranchDialog.module.css";

/** `main` or `master` when the repository has one, else "". */
function defaultTargetBranch(branches: { name: string }[]): string {
  return ["main", "master"].find((candidate) => branches.some((branch) => branch.name === candidate)) ?? "";
}

/**
 * A connected pull request section's "New pull request…" (#1012): the source branch starts as
 * the current branch, the target as `main` or `master`. A failure stays in the dialog with the
 * typed fields; the caller closes it once the pull request exists.
 */
export function NewPullRequestDialog({
  remoteName,
  branches,
  branchSuggestionsId,
  disabled,
  disabledReason,
  onCreate,
  onCancel,
}: {
  remoteName: string;
  branches: { name: string; isCurrent: boolean }[];
  /** The `<datalist>` of known branches, rendered by the panel. */
  branchSuggestionsId: string;
  /** A repository operation is running: creating waits until it ends. */
  disabled: boolean;
  disabledReason: string | null;
  /** Resolves to `null` once created, or to the failure, shown inline. */
  onCreate: (pullRequest: CreatePullRequest) => Promise<string | null>;
  onCancel: () => void;
}) {
  const reasonId = useId();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [sourceBranch, setSourceBranch] = useState(() => branches.find((branch) => branch.isCurrent)?.name ?? "");
  const [targetBranch, setTargetBranch] = useState(() => defaultTargetBranch(branches));
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const busyReason = disabled ? disabledReason : null;

  const values = { title: title.trim(), sourceBranch: sourceBranch.trim(), targetBranch: targetBranch.trim() };
  const missing = values.title === "" || values.sourceBranch === "" || values.targetBranch === "";
  const blocked = disabled || submitting || missing;
  // Why Create is off, in view under the form (#1014).
  const reason = busyReason ?? (missing ? "Enter a title, a source branch and a target branch." : null);
  const edited = (set: (value: string) => void) => (value: string) => {
    set(value);
    setError(null);
  };

  return (
    <FormDialog title="New pull request" description={`On ${remoteName}`} onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (blocked) return;
          setSubmitting(true);
          void onCreate({
            title: values.title,
            description: description.trim() === "" ? null : description.trim(),
            sourceBranch: values.sourceBranch,
            targetBranch: values.targetBranch,
          }).then((failure) => {
            setSubmitting(false);
            setError(failure);
          });
        }}
      >
        <Field label="Title">
          {(props) => (
            <input {...props} data-autofocus value={title} onChange={(event) => edited(setTitle)(event.target.value)} />
          )}
        </Field>
        <Field label="Description">
          {(props) => (
            <textarea {...props} value={description} onChange={(event) => edited(setDescription)(event.target.value)} />
          )}
        </Field>
        <Field label="Source branch" hint="The branch with the changes.">
          {(props) => (
            <input
              {...props}
              list={branchSuggestionsId}
              value={sourceBranch}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => edited(setSourceBranch)(event.target.value)}
            />
          )}
        </Field>
        <Field label="Target branch" hint="The branch to merge them into.">
          {(props) => (
            <input
              {...props}
              list={branchSuggestionsId}
              value={targetBranch}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => edited(setTargetBranch)(event.target.value)}
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
            Create pull request
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </DialogActions>
      </form>
    </FormDialog>
  );
}
