import { describe, expect, it } from "vitest";
import type { TransferErrorKind, TransferOperation, TransferProgress, UpstreamInfo } from "../ipc/RepoClient";
import type { PushRequest } from "./hookFailure";
import {
  IDLE_TRANSFER,
  fetchAllFailureMessage,
  finishCompletedTransfer,
  onTransferProgress,
  onTransferRejected,
  onTransferStarted,
  requestTransfer,
  transferFailureMessage,
  transferRequest,
  type TransferSession,
} from "./transferSession";

function event(
  operationId: string,
  operation: TransferOperation,
  phase: TransferProgress["phase"],
  extra: Partial<TransferProgress> = {},
): TransferProgress {
  return {
    operationId,
    operation,
    phase,
    errorKind: null,
    current: 0,
    total: 0,
    receivedBytes: 0,
    message: null,
    hookOutput: null,
    ...extra,
  };
}

const UPSTREAM: UpstreamInfo = { localBranch: "main", remoteName: "origin", remoteBranch: "main" };

/** Feeds `events` in order, returning the session after the last and every step on the way. */
function feed(session: TransferSession, events: TransferProgress[], upstream: UpstreamInfo | null = null) {
  const steps = [];
  for (const progress of events) {
    const step = onTransferProgress(session, progress, upstream);
    session = step.session;
    steps.push(step.effect);
  }
  return { session, steps };
}

describe("transferSession lifecycle", () => {
  it("starts idle and ignores every event while idle", () => {
    const { session, steps } = feed(IDLE_TRANSFER, [
      event("op-1", "Fetch", "Starting"),
      event("op-1", "Fetch", "Receiving"),
      event("op-1", "Fetch", "Completed"),
    ]);
    expect(session).toEqual(IDLE_TRANSFER);
    expect(steps).toEqual([{ kind: "none" }, { kind: "none" }, { kind: "none" }]);
  });

  it("adopts the first Starting event after a request and shows its progress", () => {
    const requested = requestTransfer(transferRequest("Fetch"));
    expect(requested.status).toBe("requested");

    const { session, steps } = feed(requested, [
      event("op-1", "Fetch", "Starting"),
      event("op-1", "Fetch", "Receiving", { current: 1, total: 2 }),
    ]);
    expect(session).toMatchObject({ status: "active", operationId: "op-1" });
    expect(steps.map((step) => step.kind)).toEqual(["show", "show"]);
    expect(steps[1]).toMatchObject({ transfer: { phase: "Receiving", current: 1 } });
  });

  it("never adopts a background fetch", () => {
    const { session, steps } = feed(requestTransfer(transferRequest("Fetch")), [
      event("bg-1", "BackgroundFetch", "Starting"),
      event("bg-1", "BackgroundFetch", "Completed"),
    ]);
    expect(session.status).toBe("requested");
    expect(steps).toEqual([{ kind: "none" }, { kind: "none" }]);
  });

  it("ignores a second Starting event and events of other operations once active", () => {
    const { session, steps } = feed(requestTransfer(transferRequest("Fetch")), [
      event("op-1", "Fetch", "Starting"),
      event("op-2", "Fetch", "Starting"),
      event("op-2", "Fetch", "Receiving"),
      event("op-2", "Fetch", "Failed", { errorKind: "TransferFailed" }),
    ]);
    expect(session).toMatchObject({ status: "active", operationId: "op-1" });
    expect(steps.slice(1)).toEqual([{ kind: "none" }, { kind: "none" }, { kind: "none" }]);
  });

  it("adopts the operation id its start call resolves with when no Starting event came first", () => {
    const step = onTransferStarted(requestTransfer(transferRequest("PushTags")), "push-1");
    expect(step.session).toMatchObject({ status: "active", operationId: "push-1" });
    expect(step.transfer).toEqual(event("push-1", "PushTags", "Starting"));
  });

  it("keeps the Starting event's adoption when the start call resolves afterwards", () => {
    const { session } = feed(requestTransfer(transferRequest("Fetch")), [event("op-1", "Fetch", "Starting")]);
    const step = onTransferStarted(session, "op-1");
    expect(step.session).toBe(session);
    expect(step.transfer).toBeNull();
  });

  it("does not resurrect a transfer that completed before its start call resolved", () => {
    const { session } = feed(requestTransfer(transferRequest("Fetch")), [
      event("op-1", "Fetch", "Starting"),
      event("op-1", "Fetch", "Completed"),
    ]);
    expect(session.status).toBe("completing");
    expect(onTransferStarted(session, "op-1").transfer).toBeNull();
  });

  it("goes back to idle when the request is rejected", () => {
    expect(onTransferRejected()).toEqual(IDLE_TRANSFER);
  });

  it("drops the previous transfer's events once a new request starts", () => {
    const { session: active } = feed(requestTransfer(transferRequest("Fetch")), [event("op-1", "Fetch", "Starting")]);
    expect(active.status).toBe("active");
    const { session, steps } = feed(requestTransfer(transferRequest("Pull")), [
      event("op-1", "Fetch", "Failed", { errorKind: "TransferFailed" }),
      event("pull-1", "Pull", "Starting"),
    ]);
    expect(steps[0]).toEqual({ kind: "none" });
    expect(session).toMatchObject({ status: "active", operationId: "pull-1" });
  });
});

describe("transferSession completion", () => {
  it("completes a fetch, records it, and finishes once the refresh is done", () => {
    const { session, steps } = feed(requestTransfer(transferRequest("Fetch")), [
      event("op-1", "Fetch", "Starting"),
      event("op-1", "Fetch", "Completed"),
    ]);
    expect(steps[1]).toEqual({ kind: "completed", operationId: "op-1", recordFetch: true, error: null });
    expect(session).toMatchObject({ status: "completing", operationId: "op-1" });

    const finish = finishCompletedTransfer(session, "op-1");
    expect(finish.session).toEqual(IDLE_TRANSFER);
    expect(finish.finished).toBe(true);
    expect(finish.deleteLocalTag).toBeNull();
  });

  it("does not record a fetch for a completed push", () => {
    const { steps } = feed(requestTransfer(transferRequest("PushBranch")), [
      event("op-1", "PushBranch", "Starting"),
      event("op-1", "PushBranch", "Completed"),
    ]);
    expect(steps[1]).toMatchObject({ kind: "completed", recordFetch: false });
  });

  it("reports the remotes a partly failed Fetch all could not reach", () => {
    const { steps } = feed(requestTransfer(transferRequest("FetchAll")), [
      event("op-1", "FetchAll", "Starting"),
      event("op-1", "FetchAll", "RemoteFailed", { remoteName: "backup", errorKind: "MissingCredential" }),
      event("op-1", "FetchAll", "RemoteFailed", { remoteName: "mirror", errorKind: "TransferFailed" }),
      event("op-1", "FetchAll", "Completed"),
    ]);
    expect(steps[1]).toEqual({ kind: "none" });
    expect(steps[3]).toMatchObject({
      kind: "completed",
      error:
        "Could not fetch from backup, mirror. The other remotes were fetched. backup: Save an HTTPS token for this remote before retrying.",
    });
  });

  it("forgets failed remotes between transfers", () => {
    const first = feed(requestTransfer(transferRequest("FetchAll")), [
      event("op-1", "FetchAll", "Starting"),
      event("op-1", "FetchAll", "RemoteFailed", { remoteName: "backup", errorKind: "TransferFailed" }),
    ]);
    const { steps } = feed(requestTransfer(transferRequest("FetchAll")), [
      event("op-2", "FetchAll", "Starting"),
      event("op-2", "FetchAll", "Completed"),
    ]);
    expect(first.session).toMatchObject({ failedRemotes: [{ remoteName: "backup" }] });
    expect(steps[1]).toMatchObject({ kind: "completed", error: null });
  });

  it("hands back the local tag of a landed remote tag delete", () => {
    const request = transferRequest("DeleteRemoteTag", {
      push: { operation: "DeleteRemoteTag", remoteName: "origin", tagName: "v1", deleteLocal: true },
      deleteLocalTag: "v1",
    });
    const { session } = feed(requestTransfer(request), [
      event("op-1", "DeleteRemoteTag", "Starting"),
      event("op-1", "DeleteRemoteTag", "Completed"),
    ]);
    expect(finishCompletedTransfer(session, "op-1")).toMatchObject({ finished: true, deleteLocalTag: "v1" });
  });

  it("does not finish a completed transfer a new request replaced during the refresh", () => {
    const { session } = feed(requestTransfer(transferRequest("Fetch")), [
      event("op-1", "Fetch", "Starting"),
      event("op-1", "Fetch", "Completed"),
    ]);
    expect(session.status).toBe("completing");
    const replaced = requestTransfer(transferRequest("Fetch"));
    const finish = finishCompletedTransfer(replaced, "op-1");
    expect(finish).toEqual({ session: replaced, finished: false, deleteLocalTag: null });
  });

  it("ignores further events of a transfer that is completing", () => {
    const { session, steps } = feed(requestTransfer(transferRequest("Fetch")), [
      event("op-1", "Fetch", "Starting"),
      event("op-1", "Fetch", "Completed"),
      event("op-2", "Fetch", "Starting"),
    ]);
    expect(steps[2]).toEqual({ kind: "none" });
    expect(session.status).toBe("completing");
  });
});

describe("transferSession failure", () => {
  function fail(
    operation: TransferOperation,
    errorKind: TransferErrorKind,
    options: Parameters<typeof transferRequest>[1] = {},
    upstream: UpstreamInfo | null = UPSTREAM,
    extra: Partial<TransferProgress> = {},
  ) {
    const { session, steps } = feed(
      requestTransfer(transferRequest(operation, options)),
      [event("op-1", operation, "Starting"), event("op-1", operation, "Failed", { errorKind, ...extra })],
      upstream,
    );
    return { session, effect: steps[1] };
  }

  it("ends the transfer with its failure message", () => {
    const { session, effect } = fail("Fetch", "TransferFailed");
    expect(session).toEqual(IDLE_TRANSFER);
    expect(effect).toEqual({
      kind: "failed",
      patch: { transfer: null, error: "Fetch failed", pending: false, forcePushOffer: null, hookFailure: null },
    });
  });

  it("offers a force push for a rejected push that targets what a force push would", () => {
    const offered = fail("PushBranch", "NonFastForward", { forcePushOfferRemote: "origin" });
    expect(offered.effect).toMatchObject({ patch: { forcePushOffer: { remoteName: "origin" } } });
    const notOffered = fail("PushBranch", "NonFastForward");
    expect(notOffered.effect).toMatchObject({ patch: { forcePushOffer: null } });
    const otherKind = fail("PushBranch", "TransferFailed", { forcePushOfferRemote: "origin" });
    expect(otherKind.effect).toMatchObject({ patch: { forcePushOffer: null } });
  });

  it("offers a retry without hooks for a failed pre-push hook", () => {
    const push: PushRequest = { operation: "PushTags", remoteName: "origin", names: ["v1"] };
    const { effect } = fail("PushTags", "HookFailed", { push }, UPSTREAM, { hookOutput: "lint failed" });
    expect(effect).toMatchObject({
      patch: {
        error: "The pre-push hook failed.",
        hookFailure: { hook: "pre-push", output: "lint failed", retry: { kind: "push", push }, dialogOpen: true },
      },
    });
  });

  it("names the force push's lease target in a stale-lease failure", () => {
    const leased = fail("ForcePushBranch", "StaleLease", { forcePushTarget: { remoteName: "fork", branch: "topic" } });
    expect(leased.effect).toMatchObject({
      patch: { error: "fork/topic changed since your last fetch. Fetch and review before force pushing." },
    });
    const upstreamOnly = fail("ForcePushBranch", "StaleLease", { forcePushTarget: { remoteName: "fork", branch: null } });
    expect(upstreamOnly.effect).toMatchObject({ patch: { error: expect.stringMatching(/^origin\/main changed/) } });
    const neither = fail("ForcePushBranch", "StaleLease", {}, null);
    expect(neither.effect).toMatchObject({ patch: { error: expect.stringMatching(/^The remote branch changed/) } });
  });

  it("lists the remotes a fully failed Fetch all could not reach, unless it was cancelled", () => {
    const run = (errorKind: TransferErrorKind) =>
      feed(requestTransfer(transferRequest("FetchAll")), [
        event("op-1", "FetchAll", "Starting"),
        event("op-1", "FetchAll", "RemoteFailed", { remoteName: "origin", errorKind: "SshAgentFailure" }),
        event("op-1", "FetchAll", "Failed", { errorKind }),
      ]).steps[2];
    expect(run("TransferFailed")).toMatchObject({
      patch: { error: "Could not fetch from origin. origin: Load a key into your SSH agent and try again." },
    });
    expect(run("Cancelled")).toMatchObject({ patch: { error: "Fetch cancelled." } });
  });
});

describe("transferFailureMessage", () => {
  const label = "origin/main";
  it.each<[TransferOperation, TransferErrorKind | null, string]>([
    ["Pull", "Cancelled", "Pull cancelled."],
    ["Fetch", "Cancelled", "Fetch cancelled."],
    ["FetchAll", "Cancelled", "Fetch cancelled."],
    ["DeleteRemoteBranch", "Cancelled", "Delete cancelled."],
    ["DeleteRemoteTag", "Cancelled", "Delete cancelled."],
    ["PushBranch", "Cancelled", "Push cancelled."],
    ["ForcePushBranch", "Cancelled", "Push cancelled."],
    ["PushTags", "Cancelled", "Push cancelled."],
    ["PushBranch", "HookFailed", "The pre-push hook failed."],
    ["Fetch", "MissingCredential", "Save an HTTPS token for this remote before retrying."],
    ["Pull", "CredentialStoreFailure", "The operating-system credential store is unavailable. Unlock it and try again."],
    ["PushTags", "SshAgentFailure", "Load a key into your SSH agent and try again."],
    ["ForcePushBranch", "StaleLease", "origin/main changed since your last fetch. Fetch and review before force pushing."],
    ["DeleteRemoteBranch", "RejectedRemoteRef", "The remote refused to delete the branch."],
    ["DeleteRemoteBranch", "TransferFailed", "Deleting the remote branch failed"],
    ["DeleteRemoteTag", "RejectedRemoteRef", "The remote refused to delete the tag."],
    ["DeleteRemoteTag", "NonFastForward", "Deleting the remote tag failed"],
    ["ForcePushBranch", "NonFastForward", "The remote refused the force push."],
    [
      "PushBranch",
      "NonFastForward",
      "Push was rejected because the remote has newer commits. Pull or reconcile history, then try again.",
    ],
    [
      "PushTags",
      "NonFastForward",
      "Push was rejected because the remote has newer commits. Pull or reconcile history, then try again.",
    ],
    ["PushBranch", "RejectedRemoteRef", "The remote rejected the pushed reference."],
    ["ForcePushBranch", "RejectedRemoteRef", "The remote rejected the pushed reference."],
    ["PushTags", "TransferFailed", "Push failed"],
    ["PushBranch", null, "Push failed"],
    ["Pull", "TransferFailed", "Pull failed"],
    ["Pull", "NonFastForward", "Pull failed"],
    ["Fetch", "TransferFailed", "Fetch failed"],
    ["FetchAll", "RejectedRemoteRef", "Fetch failed"],
    ["Clone", "TransferFailed", "Transfer failed"],
  ])("%s / %s → %s", (operation, errorKind, message) => {
    expect(transferFailureMessage(operation, errorKind, label)).toBe(message);
  });
});

describe("fetchAllFailureMessage", () => {
  it("names every failed remote and adds the credential hints it has", () => {
    expect(
      fetchAllFailureMessage(
        [
          { remoteName: "a", errorKind: "TransferFailed" },
          { remoteName: "b", errorKind: "CredentialStoreFailure" },
        ],
        false,
      ),
    ).toBe(
      "Could not fetch from a, b. b: The operating-system credential store is unavailable. Unlock it and try again.",
    );
  });
});
