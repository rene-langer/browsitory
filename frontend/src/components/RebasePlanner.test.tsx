import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RebasePlanCommit, RepoClient } from "../ipc/RepoClient";
import { RebasePlanner } from "./RebasePlanner";

function unused(): never {
  throw new Error("not used in this test");
}

function fakeClient(overrides: Partial<RepoClient>): RepoClient {
  return {
    pickRepoFolder: unused,
    listRecentRepos: unused,
    openRepo: unused,
    getStatus: unused,
    getCommitGraph: unused,
    listBranches: unused,
    createBranch: unused,
    switchBranch: unused,
    deleteBranch: unused,
    renameBranch: unused,
    listStashes: unused,
    saveStash: unused,
    applyStash: unused,
    dropStash: unused,
    getBlame: unused,
    getWorkingDiff: unused,
    getCommitDiff: unused,
    getCommitFiles: unused,
    stageFile: unused,
    unstageFile: unused,
    commit: unused,
    mergeBranch: unused,
    getConflictHunks: unused,
    resolveConflict: unused,
    abortMerge: unused,
    getMergeMessage: unused,
    resolveAddDeleteConflict: unused,
    commitsSince: unused,
    startRebase: unused,
    rebaseContinue: unused,
    abortRebase: unused,
    getRebaseProgress: unused,
    ...overrides,
  };
}

const commits: RebasePlanCommit[] = [
  { id: "aaa", shortId: "aaa1111", summary: "add a", authorName: "Rene", timestamp: 1 },
  { id: "bbb", shortId: "bbb2222", summary: "add b", authorName: "Rene", timestamp: 2 },
];

describe("RebasePlanner", () => {
  it("lists commits oldest-first with a default Pick action each", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );

    expect(await screen.findByText(/add a/)).toBeInTheDocument();
    expect(screen.getByText(/add b/)).toBeInTheDocument();
  });

  it("moving a row down reorders the plan sent to onStartRebase", async () => {
    const onStartRebase = vi.fn();
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
      />,
    );
    await screen.findByText(/add a/);

    fireEvent.click(screen.getAllByText("Move down")[0]);
    fireEvent.click(screen.getByText("Start Rebase"));

    expect(onStartRebase).toHaveBeenCalledWith(
      "base",
      expect.arrayContaining([
        expect.objectContaining({ commitId: "bbb" }),
        expect.objectContaining({ commitId: "aaa" }),
      ]),
    );
    const [, plan] = onStartRebase.mock.calls[0];
    expect(plan[0].commitId).toBe("bbb");
    expect(plan[1].commitId).toBe("aaa");
  });

  it("selecting Reword reveals a message field and includes it in the plan", async () => {
    const onStartRebase = vi.fn();
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
      />,
    );
    await screen.findByText(/add a/);

    fireEvent.change(screen.getAllByLabelText("Action")[0], { target: { value: "Reword" } });
    fireEvent.change(screen.getByPlaceholderText("New commit message"), {
      target: { value: "reworded" },
    });
    fireEvent.click(screen.getByText("Start Rebase"));

    const [, plan] = onStartRebase.mock.calls[0];
    expect(plan[0].action).toEqual({ kind: "Reword", message: "reworded" });
  });

  it("attaching a Squash row reveals the leader's combined-message field, pre-filled", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );
    await screen.findByText(/add a/);

    fireEvent.change(screen.getAllByLabelText("Action")[1], { target: { value: "Squash" } });

    const combined = await screen.findByLabelText("Combined message");
    expect(combined).toHaveValue("add a\n\nadd b");
  });

  it("disables Squash and Fixup on the first row", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );
    await screen.findByText(/add a/);

    const firstRowActionSelect = screen.getAllByLabelText("Action")[0] as HTMLSelectElement;
    const squashOption = Array.from(firstRowActionSelect.options).find(
      (o) => o.value === "Squash",
    );
    const fixupOption = Array.from(firstRowActionSelect.options).find(
      (o) => o.value === "Fixup",
    );
    expect(squashOption?.disabled).toBe(true);
    expect(fixupOption?.disabled).toBe(true);
  });

  it("Cancel calls onCancel without starting a rebase", async () => {
    const onCancel = vi.fn();
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner client={client} onto="base" onStartRebase={vi.fn()} onCancel={onCancel} />,
    );
    await screen.findByText(/add a/);

    fireEvent.click(screen.getByText("Cancel"));

    expect(onCancel).toHaveBeenCalled();
  });
});
