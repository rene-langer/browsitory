import { describe, expect, it } from "vitest";
import type { UpstreamInfo } from "../ipc/RepoClient";
import { pullToast, transferBusyReason, transferTitle, transferToast, type TransferSubject } from "./transferCopy";

const upstream = (behind: number | null, ahead: number | null = 0): UpstreamInfo => ({
  localBranch: "main",
  remoteName: "origin",
  remoteBranch: "main",
  ahead,
  behind,
});

const push = (overrides: Partial<Extract<TransferSubject, { kind: "push" }>> = {}): TransferSubject => ({
  kind: "push",
  localBranch: "main",
  target: "origin/main",
  ahead: 2,
  setUpstream: false,
  force: false,
  ...overrides,
});

describe("transferTitle", () => {
  it.each<[TransferSubject, string]>([
    [{ kind: "fetch", remoteName: "origin" }, "Fetching from origin"],
    [{ kind: "fetchAll", remoteCount: 3 }, "Fetching from all remotes"],
    [{ kind: "fetchAll", remoteCount: 1 }, "Fetching from the remote"],
    [{ kind: "pull", target: "origin/main" }, "Pulling from origin/main"],
    [push(), "Pushing main to origin/main"],
    [push({ force: true }), "Force pushing main to origin/main"],
    [{ kind: "pushTags", remoteName: "origin", names: ["v1"] }, "Pushing tag v1 to origin"],
    [{ kind: "pushTags", remoteName: "origin", names: ["v1", "v2"] }, "Pushing 2 tags to origin"],
    [{ kind: "pushTags", remoteName: "origin", names: [] }, "Pushing all tags to origin"],
    [{ kind: "deleteRemoteBranch", remoteName: "origin", branchName: "old" }, "Deleting origin/old"],
    [{ kind: "deleteRemoteTag", remoteName: "origin", tagName: "v1", deleteLocal: false }, "Deleting tag v1 from origin"],
  ])("names %o", (subject, title) => {
    expect(transferTitle(subject)).toBe(title);
  });

  it("says what a blocked action waits for", () => {
    expect(transferBusyReason(push())).toBe("Pushing main to origin/main. Wait for it to finish, or cancel it.");
  });
});

describe("transferToast", () => {
  it("says how far behind a fetch left the branch, and offers Pull", () => {
    expect(transferToast({ kind: "fetch", remoteName: "origin" }, upstream(3))).toEqual({
      message: "Fetched from origin. main is 3 commits behind origin/main.",
      tone: "success",
      offerPull: true,
    });
    expect(transferToast({ kind: "fetchAll", remoteCount: 2 }, upstream(1))?.message).toBe(
      "Fetched from 2 remotes. main is 1 commit behind origin/main.",
    );
  });

  it("says a fetch left the branch up to date, without Pull", () => {
    expect(transferToast({ kind: "fetch", remoteName: "origin" }, upstream(0))).toEqual({
      message: "Fetched from origin. main is up to date with origin/main.",
      tone: "success",
      offerPull: false,
    });
  });

  it("names only the remote when the fetch didn't cover the upstream, or there is none", () => {
    expect(transferToast({ kind: "fetch", remoteName: "fork" }, upstream(3))?.message).toBe("Fetched from fork.");
    expect(transferToast({ kind: "fetch", remoteName: "origin" }, null)?.message).toBe("Fetched from origin.");
    expect(transferToast({ kind: "fetch", remoteName: "origin" }, upstream(null))?.message).toBe("Fetched from origin.");
  });

  it("counts a push's commits when it went to the upstream", () => {
    expect(transferToast(push(), null)?.message).toBe("Pushed 2 commits to origin/main.");
    expect(transferToast(push({ ahead: 1 }), null)?.message).toBe("Pushed 1 commit to origin/main.");
    expect(transferToast(push({ ahead: null, localBranch: "feature", target: "origin/feature", setUpstream: true }), null)?.message).toBe(
      "Pushed feature to origin/feature and set it as the upstream.",
    );
  });

  it("says when there was nothing to push, as information", () => {
    expect(transferToast(push({ ahead: 0 }), null)).toEqual({
      message: "Nothing to push: origin/main already has main.",
      tone: "info",
      offerPull: false,
    });
  });

  it("names force pushes, tags and remote deletes", () => {
    expect(transferToast(push({ force: true }), null)?.message).toBe("Force pushed main to origin/main.");
    expect(transferToast({ kind: "pushTags", remoteName: "origin", names: ["v1"] }, null)?.message).toBe("Pushed tag v1 to origin.");
    expect(transferToast({ kind: "deleteRemoteBranch", remoteName: "origin", branchName: "old" }, null)?.message).toBe(
      "Deleted origin/old.",
    );
    expect(
      transferToast({ kind: "deleteRemoteTag", remoteName: "origin", tagName: "v1", deleteLocal: true }, null)?.message,
    ).toBe("Deleted tag v1 from origin and locally.");
  });

  it("leaves a pull to its outcome", () => {
    expect(transferToast({ kind: "pull", target: "origin/main" }, upstream(0))).toBeNull();
  });
});

describe("pullToast", () => {
  it("names the branch and what it fast-forwarded to", () => {
    expect(pullToast({ kind: "FastForwarded", upstreamRef: "refs/remotes/origin/main" }, upstream(0))).toEqual({
      message: "Fast-forwarded main to origin/main.",
      tone: "success",
      offerPull: false,
    });
    expect(pullToast({ kind: "FastForwarded", upstreamRef: "refs/remotes/origin/main" }, null).message).toBe(
      "Pulled origin/main.",
    );
  });

  it("says up to date as information", () => {
    expect(pullToast({ kind: "UpToDate" }, upstream(0))).toEqual({
      message: "main is already up to date with origin/main.",
      tone: "info",
      offerPull: false,
    });
    expect(pullToast({ kind: "UpToDate" }, null).message).toBe("Already up to date.");
  });
});
