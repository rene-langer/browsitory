import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CommitChangeRequest } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import { commitChangeMessage, commitsPatchName, patchFileName, usePatchActions } from "./usePatchActions";
import { fakeRepoClient } from "../test/fakeRepoClient";

const CONTENT = { base64: "aGk=" };

function setup(overrides: Partial<Record<string, unknown>> = {}, ok = true) {
  const client = {
    applyCommitChange: vi.fn(async () => {}),
    getPatch: vi.fn(async () => CONTENT),
    savePatchFile: vi.fn(async () => "/home/me/x.patch"),
    openPatchFile: vi.fn(async () => CONTENT),
    applyPatch: vi.fn(async () => {}),
    ...overrides,
  };
  let state = { patchOutcome: null, error: null } as unknown as AppState;
  const setState = vi.fn((update: (prev: AppState) => AppState) => {
    state = update(state);
  });
  const runMutationWithOutcome = vi.fn(async (mutate: () => Promise<void>) => {
    if (!ok) return false;
    await mutate();
    return true;
  });
  const { result } = renderHook(() =>
    usePatchActions(fakeRepoClient(client), "/repo", runMutationWithOutcome as never, setState),
  );
  return { client, actions: result.current, state: () => state, runMutationWithOutcome };
}

const change: CommitChangeRequest = {
  commitId: "1234567890abcdef",
  path: "a.txt",
  oldPath: null,
  hunk: { oldStart: 3, newStart: 3, contextLines: 3, algorithm: "Patience" },
  reverse: true,
  stage: false,
};

describe("usePatchActions", () => {
  it("reverts a commit's hunk through the mutation runner and names it in the toast", async () => {
    const { client, actions, state } = setup();
    await actions.applyCommitChange(change);
    expect(client.applyCommitChange).toHaveBeenCalledWith("/repo", change);
    expect(state().patchOutcome).toEqual({ message: "Reverted hunk in a.txt from 1234567 in the working tree", undoable: true });
  });

  it("sets no toast when the change doesn't apply (the runner shows the error)", async () => {
    const { actions, state } = setup({}, false);
    await actions.applyCommitChange(change);
    expect(state().patchOutcome).toBeNull();
  });

  it("saves the patch text where the user picks", async () => {
    const { client, actions, state, runMutationWithOutcome } = setup();
    await actions.savePatch({ kind: "Staged" }, "staged.patch");
    expect(client.getPatch).toHaveBeenCalledWith("/repo", { kind: "Staged" });
    expect(client.savePatchFile).toHaveBeenCalledWith("staged.patch", CONTENT);
    expect(state().patchOutcome).toEqual({ message: "Saved patch to /home/me/x.patch", undoable: false });
    // Reading needs no refresh.
    expect(runMutationWithOutcome).not.toHaveBeenCalled();
  });

  it("shows a failed save in the error banner and says nothing when the dialog is cancelled", async () => {
    const failing = setup({ getPatch: vi.fn(async () => Promise.reject(new Error("There are no staged changes."))) });
    await failing.actions.savePatch({ kind: "Staged" }, "staged.patch");
    expect(failing.state().error).toBe("There are no staged changes.");

    const cancelled = setup({ savePatchFile: vi.fn(async () => null) });
    await cancelled.actions.savePatch({ kind: "Staged" }, "staged.patch");
    expect(cancelled.state().patchOutcome).toBeNull();
  });

  it("applies a picked patch file, and nothing when the dialog is cancelled", async () => {
    const { client, actions, state } = setup();
    await actions.applyPatchFile();
    expect(client.applyPatch).toHaveBeenCalledWith("/repo", CONTENT);
    expect(state().patchOutcome).toEqual({ message: "Applied the patch to the working tree", undoable: true });

    const cancelled = setup({ openPatchFile: vi.fn(async () => null) });
    await cancelled.actions.applyPatchFile();
    expect(cancelled.client.applyPatch).not.toHaveBeenCalled();
  });
});

describe("patch names and messages", () => {
  it("names a file like git format-patch", () => {
    expect(patchFileName("Fix the parser: handle CRLF!")).toBe("0001-fix-the-parser-handle-crlf.patch");
    expect(patchFileName("???")).toBe("0001-patch.patch");
  });

  it("names one commit by its summary and several by their count", () => {
    const commits = [{ id: "c1", summary: "Add tests" }] as never;
    expect(commitsPatchName(["c1"], commits)).toBe("0001-add-tests.patch");
    expect(commitsPatchName(["c1", "c2", "c3"], commits)).toBe("3-commits.patch");
  });

  it("says what landed where", () => {
    expect(commitChangeMessage({ ...change, hunk: null, reverse: false, stage: true })).toBe(
      "Applied a.txt from 1234567 (staged)",
    );
  });
});
