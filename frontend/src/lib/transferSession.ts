// The lifecycle of the one fetch / pull / push a repository tab shows at a time (#490), kept free
// of React so its states and transitions can be read, and tested, in one place.
// `useRemoteTransferActions` holds the session in a ref, feeds it the request, the start call's
// result and the `subscribeTransferProgress` events, and applies the effect each step returns.
//
//   idle ──request──▶ requested ──Starting event / start call resolves──▶ active
//                         │                                               │  │
//                         └────────── start call rejects ──▶ idle ◀───────┘  │ Failed event
//                                                             ▲              │ Completed event
//                                                             │              ▼
//                                                             └─ refreshed ─ completing
//
// A new request replaces whatever the session holds, so a transfer it replaced is never
// reported. The background fetch of "Fetch automatically" (`useAutoFetch`) is never adopted.

import type { TransferErrorKind, TransferOperation, TransferProgress, UpstreamInfo } from "../ipc/RepoClient";
import type { AppState } from "../state/useAppState";
import { credentialRemediation } from "./errorMessages";
import { hookFailedMessage, type PushRequest } from "./hookFailure";
import { tagsLabel, type TransferSubject } from "./transferCopy";

/** One remote that failed during a Fetch all, as its `RemoteFailed` event reported it. */
export interface FailedRemote {
  remoteName: string;
  errorKind: TransferErrorKind | null;
}

/** What the user asked for, kept with the transfer so its terminal event can be reported. */
export interface TransferRequest {
  operation: TransferOperation;
  /** What it works on, for its running title and its closing toast (#659, #661). */
  subject: TransferSubject | null;
  /** The request a failed `pre-push` hook replays without hooks; `null` for a fetch or pull. */
  push: PushRequest | null;
  /**
   * The remote a non-fast-forward rejection offers to force-push the current branch to: set only
   * when the push targeted exactly what a force push would.
   */
  forcePushOfferRemote: string | null;
  /** The `<remote>/<branch>` a force push leases, so a stale-lease failure can name it. */
  forcePushTarget: { remoteName: string; branch: string | null } | null;
  /** The local tag a "Delete from remote" also deletes once it has landed ("also delete locally"). */
  deleteLocalTag: string | null;
}

export function transferRequest(
  operation: TransferOperation,
  options: Partial<Omit<TransferRequest, "operation">> = {},
): TransferRequest {
  return { operation, subject: null, push: null, forcePushOfferRemote: null, forcePushTarget: null, deleteLocalTag: null, ...options };
}

export type TransferSession =
  | { status: "idle" }
  /** Started; no operation id adopted yet. */
  | { status: "requested"; request: TransferRequest }
  | { status: "active"; request: TransferRequest; operationId: string; failedRemotes: FailedRemote[] }
  /** The Completed event arrived; the refresh it triggers is still running. */
  | { status: "completing"; request: TransferRequest; operationId: string };

export const IDLE_TRANSFER: TransferSession = { status: "idle" };

/** What a terminal failure sets on the tab's state. */
export type TransferFailurePatch = Pick<
  AppState,
  "transfer" | "transferSubject" | "error" | "pending" | "forcePushOffer" | "hookFailure"
>;

export type TransferEffect =
  | { kind: "none" }
  /** Show `transfer` as the tab's running transfer. */
  | { kind: "show"; transfer: TransferProgress }
  /**
   * Refresh, then call `finishCompletedTransfer`. `error` (a partly failed Fetch all) is shown
   * once the refresh is done; `null` keeps the current error.
   */
  | { kind: "completed"; operationId: string; recordFetch: boolean; error: string | null }
  | { kind: "failed"; patch: TransferFailurePatch };

export interface TransferStep {
  session: TransferSession;
  effect: TransferEffect;
}

const NONE: TransferEffect = { kind: "none" };

export function requestTransfer(request: TransferRequest): TransferSession {
  return { status: "requested", request };
}

/** The request failed before the transfer began (or a pull failed without a terminal event). */
export function onTransferRejected(): TransferSession {
  return IDLE_TRANSFER;
}

/**
 * The start call resolved with `operationId`. Adopted only if no Starting event was adopted
 * first and the request is still waiting; `transfer` is then the progress to show.
 */
export function onTransferStarted(
  session: TransferSession,
  operationId: string,
): { session: TransferSession; transfer: TransferProgress | null } {
  if (session.status !== "requested") return { session, transfer: null };
  return {
    session: { status: "active", request: session.request, operationId, failedRemotes: [] },
    transfer: {
      operationId,
      operation: session.request.operation,
      phase: "Starting",
      errorKind: null,
      current: 0,
      total: 0,
      receivedBytes: 0,
      message: null,
      hookOutput: null,
    },
  };
}

/** One `subscribeTransferProgress` event. `upstream` is the current branch's, for failure text. */
export function onTransferProgress(
  session: TransferSession,
  progress: TransferProgress,
  upstream: UpstreamInfo | null,
): TransferStep {
  if (progress.operation === "BackgroundFetch") return { session, effect: NONE };
  if (progress.phase === "Starting") {
    if (session.status !== "requested") return { session, effect: NONE };
    return {
      session: { status: "active", request: session.request, operationId: progress.operationId, failedRemotes: [] },
      effect: { kind: "show", transfer: progress },
    };
  }
  if (session.status !== "active" || progress.operationId !== session.operationId) return { session, effect: NONE };

  switch (progress.phase) {
    case "RemoteFailed":
      if (progress.remoteName === undefined) return { session, effect: NONE };
      return {
        session: {
          ...session,
          failedRemotes: [...session.failedRemotes, { remoteName: progress.remoteName, errorKind: progress.errorKind }],
        },
        effect: NONE,
      };
    case "Completed":
      return {
        session: { status: "completing", request: session.request, operationId: session.operationId },
        effect: {
          kind: "completed",
          operationId: session.operationId,
          recordFetch: progress.operation === "Fetch" || progress.operation === "FetchAll",
          error: session.failedRemotes.length > 0 ? fetchAllFailureMessage(session.failedRemotes, true) : null,
        },
      };
    case "Failed":
      return { session: IDLE_TRANSFER, effect: { kind: "failed", patch: failurePatch(session, progress, upstream) } };
    default:
      return { session, effect: { kind: "show", transfer: progress } };
  }
}

/**
 * The refresh after `operationId`'s Completed event is done. `finished` is false when a new
 * request replaced the transfer meanwhile, which then owns the tab's state.
 */
export function finishCompletedTransfer(
  session: TransferSession,
  operationId: string,
): { session: TransferSession; finished: boolean; deleteLocalTag: string | null; subject: TransferSubject | null } {
  if (session.status !== "completing" || session.operationId !== operationId) {
    return { session, finished: false, deleteLocalTag: null, subject: null };
  }
  const deleteLocalTag = session.request.operation === "DeleteRemoteTag" ? session.request.deleteLocalTag : null;
  return { session: IDLE_TRANSFER, finished: true, deleteLocalTag, subject: session.request.subject };
}

function failurePatch(
  session: Extract<TransferSession, { status: "active" }>,
  progress: TransferProgress,
  upstream: UpstreamInfo | null,
): TransferFailurePatch {
  const { request, failedRemotes } = session;
  const target = progress.operation === "ForcePushBranch" ? request.forcePushTarget : null;
  const upstreamLabel =
    target !== null && target.branch !== null
      ? `${target.remoteName}/${target.branch}`
      : upstream !== null
        ? `${upstream.remoteName}/${upstream.remoteBranch}`
        : "The remote branch";
  const error =
    progress.operation === "FetchAll" && progress.errorKind !== "Cancelled" && failedRemotes.length > 0
      ? fetchAllFailureMessage(failedRemotes, false)
      : transferFailureMessage(progress.operation, progress.errorKind, upstreamLabel, request.subject);
  const offerForcePush =
    progress.operation === "PushBranch" &&
    progress.errorKind === "NonFastForward" &&
    request.forcePushOfferRemote !== null;
  return {
    transfer: null,
    transferSubject: null,
    error,
    pending: false,
    forcePushOffer: offerForcePush ? { remoteName: request.forcePushOfferRemote as string } : null,
    hookFailure:
      progress.errorKind === "HookFailed" && request.push !== null
        ? { hook: "pre-push", output: progress.hookOutput ?? "", retry: { kind: "push", push: request.push }, dialogOpen: true }
        : null,
  };
}

type OperationFamily = "fetch" | "pull" | "push" | "forcePush" | "deleteRemoteBranch" | "deleteRemoteTag" | "other";

const FAMILY: Record<TransferOperation, OperationFamily> = {
  Fetch: "fetch",
  FetchAll: "fetch",
  BackgroundFetch: "fetch",
  Pull: "pull",
  PushBranch: "push",
  PushTags: "push",
  ForcePushBranch: "forcePush",
  DeleteRemoteBranch: "deleteRemoteBranch",
  DeleteRemoteTag: "deleteRemoteTag",
  Clone: "other",
};

/**
 * What failed, for an error kind with no more specific text (#997): the operation, and what it
 * worked on when the transfer's subject is known ("Couldn't fetch from origin").
 */
function whatFailed(family: OperationFamily, subject: TransferSubject | null): string {
  switch (family) {
    case "fetch":
      if (subject?.kind === "fetch") return `Couldn't fetch from ${subject.remoteName}`;
      return subject?.kind === "fetchAll" ? "Couldn't fetch from the remotes" : "Couldn't fetch";
    case "pull":
      return subject?.kind === "pull" ? `Couldn't pull from ${subject.target}` : "Couldn't pull";
    case "push":
    case "forcePush": {
      const verb = family === "forcePush" ? "force push" : "push";
      if (subject?.kind === "push") return `Couldn't ${verb} ${subject.localBranch} to ${subject.target}`;
      if (subject?.kind === "pushTags") return `Couldn't push ${tagsLabel(subject.names)} to ${subject.remoteName}`;
      return `Couldn't ${verb}`;
    }
    case "deleteRemoteBranch":
      return subject?.kind === "deleteRemoteBranch"
        ? `Couldn't delete ${subject.remoteName}/${subject.branchName}`
        : "Couldn't delete the remote branch";
    case "deleteRemoteTag":
      return subject?.kind === "deleteRemoteTag"
        ? `Couldn't delete tag ${subject.tagName} from ${subject.remoteName}`
        : "Couldn't delete the remote tag";
    case "other":
      return "Couldn't finish the transfer";
  }
}

/**
 * The next step after a failure with no more specific text. The cause is in the operation log
 * (the banner's Details): the transfer event carries only the error kind, never the remote's
 * text, which may hold credentials.
 */
const NEXT_STEP: Record<OperationFamily, string> = {
  fetch: "Check the remote's URL and your connection, then try again.",
  pull: "Check the remote and your connection, then pull again.",
  push: "Check the remote and your connection, then try again.",
  forcePush: "Check the remote and your connection, then try again.",
  deleteRemoteBranch: "Check the remote and your connection, then try again.",
  deleteRemoteTag: "Check the remote and your connection, then try again.",
  other: "Try again.",
};

function generic(family: OperationFamily, subject: TransferSubject | null): string {
  return `${whatFailed(family, subject)}. ${NEXT_STEP[family]}`;
}

// A cancellation reaches the UI as a `Failed` terminal event (the transfer really did stop
// short), but the user pressed Cancel themselves, so it must not read as "Fetch failed".
// Reporting it as an error (rather than as a completion) also keeps `state.error` non-null,
// which is what stops `App`'s success toast from announcing "Fetch complete".
const CANCELLED: Record<OperationFamily, string> = {
  fetch: "Fetch cancelled.",
  pull: "Pull cancelled.",
  push: "Push cancelled.",
  forcePush: "Push cancelled.",
  deleteRemoteBranch: "Delete cancelled.",
  deleteRemoteTag: "Delete cancelled.",
  other: "Transfer cancelled.",
};

/** Kinds that read the same whatever the operation. */
const BY_KIND: Partial<Record<TransferErrorKind, (upstreamLabel: string) => string>> = {
  HookFailed: () => hookFailedMessage("pre-push"),
  MissingCredential: () => credentialRemediation("MissingCredential"),
  CredentialStoreFailure: () => credentialRemediation("CredentialStoreFailure"),
  SshAgentFailure: () => credentialRemediation("SshAgentFailure"),
  StaleLease: (upstreamLabel) => `${upstreamLabel} changed since your last fetch. Fetch and review before force pushing.`,
};

// A lock file another Git program holds (#1033). The transfer event carries no text; the
// operation log (Details) has git's, which names the file.
const LOCK_HELD =
  "Another Git program is using the repository. Wait until it finishes, then try again. If none is running, delete the leftover lock file shown in Details.";

const NON_FAST_FORWARD_PUSH =
  "Push was rejected because the remote has newer commits. Pull or reconcile history, then try again.";
const REJECTED_PUSHED_REF = "The remote rejected the pushed reference.";

/** Per-operation text for the remaining kinds. */
const OVERRIDES: Partial<Record<OperationFamily, Partial<Record<TransferErrorKind, string>>>> = {
  push: { NonFastForward: NON_FAST_FORWARD_PUSH, RejectedRemoteRef: REJECTED_PUSHED_REF },
  forcePush: { NonFastForward: "The remote refused the force push.", RejectedRemoteRef: REJECTED_PUSHED_REF },
  deleteRemoteBranch: { RejectedRemoteRef: "The remote refused to delete the branch." },
  deleteRemoteTag: { RejectedRemoteRef: "The remote refused to delete the tag." },
};

/** What a single-remote transfer's `Failed` event shows; `subject` is what it worked on. */
export function transferFailureMessage(
  operation: TransferOperation,
  errorKind: TransferErrorKind | null,
  upstreamLabel: string,
  subject: TransferSubject | null = null,
): string {
  const family = FAMILY[operation];
  if (errorKind === null) return generic(family, subject);
  if (errorKind === "Cancelled") return CANCELLED[family];
  if (errorKind === "LockHeld") return `${whatFailed(family, subject)}. ${LOCK_HELD}`;
  return BY_KIND[errorKind]?.(upstreamLabel) ?? OVERRIDES[family]?.[errorKind] ?? generic(family, subject);
}

// The same fixed hints `transferFailureMessage` gives a single-remote transfer.
function remoteHint(errorKind: TransferErrorKind | null): string | null {
  if (errorKind === "MissingCredential" || errorKind === "CredentialStoreFailure" || errorKind === "SshAgentFailure") {
    return credentialRemediation(errorKind);
  }
  return errorKind === "LockHeld" ? LOCK_HELD : null;
}

/** What a Fetch all with failed remotes shows; `othersFetched` when it still completed. */
export function fetchAllFailureMessage(failed: FailedRemote[], othersFetched: boolean): string {
  const parts = [`Could not fetch from ${failed.map((remote) => remote.remoteName).join(", ")}.`];
  if (othersFetched) parts.push("The other remotes were fetched.");
  for (const remote of failed) {
    const hint = remoteHint(remote.errorKind);
    if (hint !== null) parts.push(`${remote.remoteName}: ${hint}`);
  }
  return parts.join(" ");
}
