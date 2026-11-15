import { useId, useRef, useState } from "react";
import type { ForgeProvider } from "../ipc/RepoClient";
import { describedBy } from "../lib/ariaDescribedBy";
import { BlockedReason } from "./primitives/BlockedReason";
import { DialogActions } from "./primitives/DialogActions";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import { InlineError } from "./primitives/InlineError";
import styles from "./PushBranchDialog.module.css";

/**
 * "Connect to GitHub/Bitbucket" (#1012): saves a personal access token for an account in the
 * system's credential store. The token field is uncontrolled and cleared after every attempt, so
 * the token never becomes component state. A failure stays in the dialog; the caller closes it
 * once the token is saved.
 */
export function ConnectForgeDialog({
  provider,
  disabled,
  disabledReason,
  onConnect,
  onCancel,
}: {
  provider: ForgeProvider;
  /** A repository operation is running: saving waits until it ends. */
  disabled: boolean;
  disabledReason: string | null;
  /** Resolves to `null` once the token is saved, or to the failure, shown inline. */
  onConnect: (account: string, token: string) => Promise<string | null>;
  onCancel: () => void;
}) {
  const reasonId = useId();
  const tokenRef = useRef<HTMLInputElement>(null);
  const [account, setAccount] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const busyReason = disabled ? disabledReason : null;
  const blocked = disabled || submitting;

  return (
    <FormDialog
      title={`Connect to ${provider}`}
      description="Browsitory asks for this repository's pull requests with a personal access token."
      onCancel={onCancel}
    >
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          const name = account.trim();
          const token = tokenRef.current?.value ?? "";
          if (blocked || name === "" || token === "") return;
          setSubmitting(true);
          void onConnect(name, token).then((failure) => {
            if (tokenRef.current !== null) tokenRef.current.value = "";
            setSubmitting(false);
            setError(failure);
          });
        }}
      >
        <Field label="Account" hint={`Your ${provider} user name.`}>
          {(props) => (
            <input
              {...props}
              data-autofocus
              value={account}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => {
                setAccount(event.target.value);
                setError(null);
              }}
            />
          )}
        </Field>
        <Field
          label="Access token"
          hint={
            provider === "Bitbucket"
              ? "Requires a Bitbucket repository or workspace access token (not an app password)."
              : "A personal access token that can read and create pull requests."
          }
        >
          {(props) => <input {...props} ref={tokenRef} type="password" autoComplete="off" />}
        </Field>
        {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
        <BlockedReason id={reasonId} reason={busyReason} />
        <DialogActions>
          <button
            type="submit"
            data-tone="primary"
            disabled={blocked}
            title={busyReason ?? undefined}
            aria-describedby={describedBy(reasonId, busyReason)}
          >
            Connect
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </DialogActions>
      </form>
    </FormDialog>
  );
}
