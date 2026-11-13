import { describe, expect, it, vi } from "vitest";
import type { Rerun } from "../ipc/RepoClient";
import { rerunBlockedReason, runRerun, type RerunActions } from "./rerun";

const fetch: Rerun = { kind: "Fetch", remoteName: "origin", prune: true };
const fetchAll: Rerun = { kind: "FetchAll", prune: null };
const pull: Rerun = { kind: "Pull" };
const push: Rerun = { kind: "PushCurrentBranch", remoteName: "origin", noVerify: true };
const pushBranch: Rerun = {
  kind: "PushBranch",
  remoteName: "origin",
  localBranch: "topic",
  remoteBranch: "feature/topic",
  setUpstream: true,
  noVerify: false,
};
const pushTags: Rerun = { kind: "PushTags", remoteName: "backup", names: ["v1", "v2"], noVerify: false };

describe("runRerun (#890)", () => {
  function actions(): RerunActions {
    return {
      fetchRemote: vi.fn(),
      fetchAllRemotes: vi.fn(),
      pullCurrentUpstream: vi.fn(),
      pushCurrentBranch: vi.fn(),
      pushBranch: vi.fn(),
      pushTags: vi.fn(),
    };
  }

  it("runs each operation again with the parameters it was given", () => {
    const run = actions();
    runRerun(fetch, run);
    expect(run.fetchRemote).toHaveBeenCalledWith("origin");
    runRerun(fetchAll, run);
    expect(run.fetchAllRemotes).toHaveBeenCalledOnce();
    runRerun(pull, run);
    expect(run.pullCurrentUpstream).toHaveBeenCalledOnce();
    runRerun(push, run);
    expect(run.pushCurrentBranch).toHaveBeenCalledWith("origin", true);
    runRerun(pushBranch, run);
    expect(run.pushBranch).toHaveBeenCalledWith("origin", "topic", "feature/topic", true, false);
    runRerun(pushTags, run);
    expect(run.pushTags).toHaveBeenCalledWith("backup", ["v1", "v2"], false);
  });
});

describe("rerunBlockedReason (#890)", () => {
  const state = { remotes: ["origin", "backup"], branches: ["main", "topic"] };

  it("allows an operation whose remote and branch are still there", () => {
    for (const rerun of [fetch, fetchAll, pull, push, pushBranch, pushTags]) {
      expect(rerunBlockedReason(rerun, state)).toBeNull();
    }
  });

  it("refuses when the state it ran in changed: its remote or branch is gone", () => {
    expect(rerunBlockedReason(fetch, { ...state, remotes: ["backup"] })).toBe("The remote origin no longer exists.");
    expect(rerunBlockedReason(push, { ...state, remotes: [] })).toBe("The remote origin no longer exists.");
    expect(rerunBlockedReason(pushTags, { ...state, remotes: ["origin"] })).toBe("The remote backup no longer exists.");
    expect(rerunBlockedReason(pushBranch, { ...state, branches: ["main"] })).toBe("The branch topic no longer exists.");
    expect(rerunBlockedReason(fetchAll, { ...state, remotes: [] })).toBe("The repository has no remotes any more.");
  });
});
