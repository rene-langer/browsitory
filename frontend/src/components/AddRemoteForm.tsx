import { useId, useState } from "react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { remoteNameProblem } from "../lib/refName";
import { InlineError } from "./primitives/InlineError";
import styles from "./BranchTree.module.css";
import { BlockedReason } from "./primitives/BlockedReason";
import { describedBy } from "../lib/ariaDescribedBy";

/** The name a new remote gets from its fetch URL: `origin` while there is none, else the URL's last segment. */
function deriveRemoteName(fetchUrl: string, existingNames: string[]): string {
  if (!existingNames.includes("origin")) return "origin";
  const withoutGitSuffix = fetchUrl.replace(/\.git\/?$/, "");
  const slug = withoutGitSuffix.split(/[/:]/).filter((part) => part !== "").pop();
  return slug ?? "";
}

/**
 * The Branches section's inline "Add remote" form, shown while `open`. It owns its fields; stay
 * mounted while closed (it renders nothing then) so entered values survive closing and reopening.
 */
export function AddRemoteForm({
  open,
  remotes,
  operationDisabled,
  operationDisabledReason,
  onAddRemote,
  onClose,
}: {
  open: boolean;
  remotes: RemoteInfo[];
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  /** `null` = added; a string = the failure message to show inline. */
  onAddRemote: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<string | null>;
  onClose: () => void;
}) {
  const [newRemoteName, setNewRemoteName] = useState("");
  const [newFetchUrl, setNewFetchUrl] = useState("");
  const [newPushUrl, setNewPushUrl] = useState("");
  const [nameTouched, setNameTouched] = useState(false);
  const [showPushUrl, setShowPushUrl] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);
  const reasonId = useId();

  if (!open) return null;
  const busyReason = operationDisabled ? operationDisabledReason : null;

  const newRemoteProblem = remoteNameProblem(newRemoteName.trim());

  const submitAddRemote = async () => {
    setAddError(null);
    const fetchUrl = newFetchUrl.trim();
    const name = (newRemoteName.trim() || deriveRemoteName(fetchUrl, remotes.map((r) => r.name))).trim();
    if (name === "" || fetchUrl === "") return;
    // The name field shows the same check as you type; a name derived from the URL is checked here.
    const problem = remoteNameProblem(name);
    if (problem !== null) {
      setAddError(problem);
      return;
    }
    const failure = await onAddRemote(name, fetchUrl, newPushUrl.trim() || null);
    if (failure !== null) {
      setAddError(failure);
      return;
    }
    setNewRemoteName("");
    setNewFetchUrl("");
    setNewPushUrl("");
    setNameTouched(false);
    setShowPushUrl(false);
  };

  return (
    <form
      className={styles.form}
      aria-label="Add remote"
      onKeyDown={(event) => {
        if (event.key === "Escape") onClose();
      }}
      onSubmit={(event) => {
        event.preventDefault();
        void submitAddRemote();
      }}
    >
      <label className={styles.label}>
        Remote name
        <input
          autoFocus
          placeholder="origin"
          value={newRemoteName}
          aria-invalid={newRemoteProblem !== null}
          aria-describedby={newRemoteProblem !== null ? "new-remote-name-problem" : undefined}
          onChange={(event) => {
            setNameTouched(true);
            setNewRemoteName(event.target.value);
          }}
        />
      </label>
      {newRemoteProblem !== null && (
        <p id="new-remote-name-problem" className={styles.draftProblem}>
          {newRemoteProblem}
        </p>
      )}
      <label className={styles.label}>
        Fetch URL
        <input
          data-testid="add-remote-fetch-url"
          placeholder="git@github.com:user/repo.git"
          value={newFetchUrl}
          onChange={(event) => {
            const value = event.target.value;
            setNewFetchUrl(value);
            setAddError(null);
            if (!nameTouched) {
              setNewRemoteName(deriveRemoteName(value, remotes.map((r) => r.name)));
            }
          }}
        />
      </label>
      {addError !== null && <InlineError message={addError} onDismiss={() => setAddError(null)} />}
      {/* `showPushUrl` is the only source of truth: the summary's click handler cancels the native
          toggle, so no `onToggle` echo. Echoing the async `toggle` event back into state raced a
          pending update (a toggle queued by opening could land after submit and reopen it). */}
      <details open={showPushUrl}>
        <summary
          onClick={(event) => {
            event.preventDefault();
            setShowPushUrl((isOpen) => !isOpen);
          }}
        >
          Push URL (optional)
        </summary>
        {showPushUrl && (
          <label className={styles.label}>
            Push URL
            <input
              placeholder="git@github.com:user/repo.git"
              value={newPushUrl}
              onChange={(event) => setNewPushUrl(event.target.value)}
            />
          </label>
        )}
      </details>
      <BlockedReason id={reasonId} reason={busyReason} />
      <button
        type="submit"
        disabled={operationDisabled || newRemoteProblem !== null}
        title={busyReason ?? undefined}
        aria-describedby={describedBy(reasonId, busyReason)}
      >
        Add remote
      </button>
      <button type="button" onClick={onClose}>
        Cancel
      </button>
    </form>
  );
}
