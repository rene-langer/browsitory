import { useState } from "react";
import type { TransferOperation, TransferProgress } from "../ipc/RepoClient";
import { runningHookLabel } from "../lib/hookFailure";
import { type TransferSubject, transferTitle } from "../lib/transferCopy";
import { type Activity, ActivityIndicator } from "./ActivityIndicator";

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

/** A title for a transfer without a subject (every transfer the tab starts has one). */
const OPERATION_TITLE: Record<TransferOperation, string> = {
  Fetch: "Fetching",
  FetchAll: "Fetching from all remotes",
  BackgroundFetch: "Fetching",
  Pull: "Pulling",
  PushBranch: "Pushing",
  ForcePushBranch: "Force pushing",
  PushTags: "Pushing tags",
  DeleteRemoteBranch: "Deleting a remote branch",
  DeleteRemoteTag: "Deleting a remote tag",
  Clone: "Cloning",
};

const SENDS = new Set<TransferOperation>([
  "PushBranch",
  "ForcePushBranch",
  "PushTags",
  "DeleteRemoteBranch",
  "DeleteRemoteTag",
]);

/** Where the transfer got to, and how far (0 to 1) when it says. */
function transferDetail(progress: TransferProgress | null): { detail: string; progress: number | null } {
  if (progress === null || progress.phase === "Starting") return { detail: "Connecting…", progress: null };
  if (progress.phase === "RunningHook") {
    // A push's `pre-push` (nothing transferred yet), or a pull's `post-merge` after it
    // fast-forwarded (#185).
    return { detail: runningHookLabel(progress.operation === "Pull" ? "post-merge" : "pre-push"), progress: null };
  }
  if (progress.phase === "Updating") return { detail: "Updating references…", progress: 1 };
  if (progress.total > 0) {
    const verb = SENDS.has(progress.operation) ? "sent" : "received";
    return {
      detail: `${progress.current} / ${progress.total} objects, ${formatBytes(progress.receivedBytes)} ${verb}`,
      progress: progress.current / progress.total,
    };
  }
  // The remote's own status line ("Counting objects…"), sanitized by the backend.
  return { detail: progress.message ?? "Waiting for the remote…", progress: null };
}

/**
 * Cancel for one transfer. Keyed by its operation id where it is used, so it re-arms by itself
 * when the next transfer starts. Cancelling is a request, not an event: the transfer keeps
 * reporting progress until git reaches its next callback (or, still connecting, until the worker
 * gives up on it), so the button stays, latched.
 */
function CancelTransfer({ operationId, onCancel }: { operationId: string; onCancel: (operationId: string) => void }) {
  const [cancelling, setCancelling] = useState(false);
  return (
    <button
      type="button"
      disabled={cancelling}
      onClick={() => {
        setCancelling(true);
        onCancel(operationId);
      }}
    >
      {cancelling ? "Cancelling…" : "Cancel"}
    </button>
  );
}

/**
 * What the repository is busy with, in the toolbar (#659, #660): the fetch, pull or push this tab
 * started, named, with its progress and Cancel, or (quietly) any other action in flight. Never
 * modal: browsing goes on meanwhile, and the actions that must wait say why (`busyReason`).
 */
export function RepoActivity({
  subject,
  progress,
  pending,
  onCancel,
}: {
  /** The transfer this tab started (`AppState.transferSubject`), or `null`. */
  subject: TransferSubject | null;
  /** Its latest progress event (`AppState.transfer`); `null` until the first one arrives. */
  progress: TransferProgress | null;
  /** Any action in flight (`AppState.pending`). */
  pending: boolean;
  /** Cancels the transfer: the backend's `cancelTransfer`. */
  onCancel: (operationId: string) => void;
}) {
  let activity: Activity | null = null;
  if (subject !== null || progress !== null) {
    const label = subject !== null ? transferTitle(subject) : OPERATION_TITLE[progress!.operation];
    const { detail, progress: done } = transferDetail(progress);
    activity = {
      label,
      detail,
      progress: done,
      // No operation id before the first event or the start call's answer: nothing to cancel yet.
      action:
        progress === null ? undefined : (
          <CancelTransfer key={progress.operationId} operationId={progress.operationId} onCancel={onCancel} />
        ),
    };
  } else if (pending) {
    activity = { label: "Working…", quiet: true };
  }
  return <ActivityIndicator activity={activity} />;
}
