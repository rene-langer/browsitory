// What a fetch, pull or push says while it runs and once it is done (#659, #661). The subject is
// fixed when the user starts the transfer (`useRemoteTransferActions`), so the running title and
// the closing toast name the same remote, branch or tags.

import type { UpstreamInfo } from "../ipc/RepoClient";

/** What one transfer the user started works on. */
export type TransferSubject =
  | { kind: "fetch"; remoteName: string }
  | { kind: "fetchAll"; remoteCount: number }
  | { kind: "pull"; target: string }
  /**
   * `target` is `<remote>/<branch>`, or the remote alone when `push.default` picks the branch.
   * `ahead`: the commits to push, when the push goes to the branch's upstream (else `null`).
   */
  | { kind: "push"; localBranch: string; target: string; ahead: number | null; setUpstream: boolean; force: boolean }
  /** `names` empty: every tag. */
  | { kind: "pushTags"; remoteName: string; names: string[] }
  | { kind: "deleteRemoteBranch"; remoteName: string; branchName: string }
  | { kind: "deleteRemoteTag"; remoteName: string; tagName: string; deleteLocal: boolean };

function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

function tagsLabel(names: string[]): string {
  if (names.length === 0) return "all tags";
  if (names.length === 1) return `tag ${names[0]}`;
  return plural(names.length, "tag");
}

/** The running transfer's title: "Pushing main to origin/main". */
export function transferTitle(subject: TransferSubject): string {
  switch (subject.kind) {
    case "fetch":
      return `Fetching from ${subject.remoteName}`;
    case "fetchAll":
      return subject.remoteCount === 1 ? "Fetching from the remote" : "Fetching from all remotes";
    case "pull":
      return `Pulling from ${subject.target}`;
    case "push":
      return `${subject.force ? "Force pushing" : "Pushing"} ${subject.localBranch} to ${subject.target}`;
    case "pushTags":
      return `Pushing ${tagsLabel(subject.names)} to ${subject.remoteName}`;
    case "deleteRemoteBranch":
      return `Deleting ${subject.remoteName}/${subject.branchName}`;
    case "deleteRemoteTag":
      return `Deleting tag ${subject.tagName} from ${subject.remoteName}`;
  }
}

/** Why repository actions wait while a transfer runs (`busyReason`). */
export function transferBusyReason(subject: TransferSubject): string {
  return `${transferTitle(subject)}. Wait for it to finish, or cancel it.`;
}

export interface TransferToast {
  message: string;
  tone: "success" | "info";
  /** The toast offers Pull: a fetch left the current branch behind its upstream. */
  offerPull: boolean;
}

/**
 * The toast once a transfer completed. `upstream` is the current branch's after the refresh that
 * followed, so a fetch can say how far behind it left the branch. `null` for a pull: its outcome
 * says whether anything moved (`pullToast`).
 */
export function transferToast(subject: TransferSubject, upstream: UpstreamInfo | null): TransferToast | null {
  const done = (message: string, tone: TransferToast["tone"] = "success"): TransferToast => ({
    message,
    tone,
    offerPull: false,
  });
  switch (subject.kind) {
    case "fetch":
    case "fetchAll": {
      const fetched =
        subject.kind === "fetch"
          ? `Fetched from ${subject.remoteName}.`
          : `Fetched from ${plural(subject.remoteCount, "remote")}.`;
      const covers = upstream !== null && (subject.kind === "fetchAll" || upstream.remoteName === subject.remoteName);
      if (!covers || upstream.behind == null) return done(fetched);
      const tracking = `${upstream.remoteName}/${upstream.remoteBranch}`;
      if (upstream.behind === 0) return done(`${fetched} ${upstream.localBranch} is up to date with ${tracking}.`);
      return {
        message: `${fetched} ${upstream.localBranch} is ${plural(upstream.behind, "commit")} behind ${tracking}.`,
        tone: "success",
        offerPull: true,
      };
    }
    case "pull":
      return null;
    case "push": {
      if (subject.force) return done(`Force pushed ${subject.localBranch} to ${subject.target}.`);
      if (subject.ahead === 0) return done(`Nothing to push: ${subject.target} already has ${subject.localBranch}.`, "info");
      const what = subject.ahead === null ? subject.localBranch : plural(subject.ahead, "commit");
      return done(`Pushed ${what} to ${subject.target}${subject.setUpstream ? " and set it as the upstream" : ""}.`);
    }
    case "pushTags":
      return done(`Pushed ${tagsLabel(subject.names)} to ${subject.remoteName}.`);
    case "deleteRemoteBranch":
      return done(`Deleted ${subject.remoteName}/${subject.branchName}.`);
    case "deleteRemoteTag":
      return done(`Deleted tag ${subject.tagName} from ${subject.remoteName}${subject.deleteLocal ? " and locally" : ""}.`);
  }
}

/**
 * The toast for a pull that didn't stop to ask (a diverged pull asks first). `upstreamRef` is the
 * outcome's (`refs/remotes/origin/main`), `upstream` the branch's tracking info after the pull.
 */
export function pullToast(
  outcome: { kind: "UpToDate" } | { kind: "FastForwarded"; upstreamRef: string },
  upstream: UpstreamInfo | null,
): TransferToast {
  const tracking = upstream === null ? null : `${upstream.remoteName}/${upstream.remoteBranch}`;
  if (outcome.kind === "UpToDate") {
    return {
      message: upstream === null ? "Already up to date." : `${upstream.localBranch} is already up to date with ${tracking}.`,
      tone: "info",
      offerPull: false,
    };
  }
  const from = outcome.upstreamRef.replace(/^refs\/(remotes|heads)\//, "");
  return {
    message: upstream === null ? `Pulled ${from}.` : `Fast-forwarded ${upstream.localBranch} to ${from}.`,
    tone: "success",
    offerPull: false,
  };
}
