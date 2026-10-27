import { useState } from "react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { branchNameProblem } from "../lib/refName";
import { Field } from "./primitives/Field";
import { FormDialog } from "./primitives/FormDialog";
import styles from "./PushBranchDialog.module.css";

/** The branch's upstream remote, else `origin` if it exists, else the first remote. */
function defaultRemote(remotes: RemoteInfo[], branchUpstream: string | null | undefined): string {
  const upstreamRemote =
    branchUpstream == null ? undefined : remotes.find((remote) => branchUpstream.startsWith(`${remote.name}/`));
  if (upstreamRemote !== undefined) return upstreamRemote.name;
  if (remotes.some((remote) => remote.name === "origin")) return "origin";
  return remotes[0]?.name ?? "";
}

/** `branchNameProblem` plus the names the backend refuses as a push target. */
function remoteBranchProblem(name: string): string | null {
  if (name === "HEAD") return "Branch name can't be HEAD.";
  if (name.startsWith("+")) return "Branch names can't start with +.";
  return branchNameProblem(name);
}

/** "Push to…": any local branch to any remote branch, optionally becoming its upstream. */
export function PushBranchDialog({
  localBranch,
  remotes,
  branchUpstream,
  onPush,
  onCancel,
}: {
  localBranch: string;
  remotes: RemoteInfo[];
  /** `BranchInfo.upstream` of `localBranch` ("origin/main"), or null/undefined without one. */
  branchUpstream: string | null | undefined;
  onPush: (remoteName: string, remoteBranch: string, setUpstream: boolean) => void;
  onCancel: () => void;
}) {
  const [remoteName, setRemoteName] = useState(() => defaultRemote(remotes, branchUpstream));
  const [remoteBranch, setRemoteBranch] = useState(localBranch);
  // Publishing a branch without an upstream is the common case; one that already tracks
  // something keeps it unless the user opts in.
  const [setUpstream, setSetUpstream] = useState(branchUpstream == null);
  const trimmed = remoteBranch.trim();
  const problem = remoteBranchProblem(trimmed);
  const blocked = trimmed === "" || problem !== null || remoteName === "";

  return (
    <FormDialog ariaLabel={`Push ${localBranch}`} onCancel={onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (!blocked) onPush(remoteName, trimmed, setUpstream);
        }}
      >
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
        <Field label="Remote branch" error={problem}>
          {(props) => (
            <input
              {...props}
              value={remoteBranch}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setRemoteBranch(event.target.value)}
            />
          )}
        </Field>
        <label className={styles.checkbox}>
          <input type="checkbox" checked={setUpstream} onChange={(event) => setSetUpstream(event.target.checked)} />
          Set as upstream
        </label>
        <div className={styles.actions}>
          <button type="submit" disabled={blocked}>
            Push
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
    </FormDialog>
  );
}
