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
    listRemotes: async () => unused(),
    getCurrentUpstream: async () => null,
    addRemote: async () => unused(),
    renameRemote: async () => unused(),
    updateRemoteUrls: async () => unused(),
    removeRemote: async () => unused(),
    setCurrentUpstream: async () => unused(),
    clearCurrentUpstream: async () => unused(),
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

const threeCommits: RebasePlanCommit[] = [
  { id: "aaa", shortId: "aaa1111", summary: "add a", authorName: "Rene", timestamp: 1 },
  { id: "bbb", shortId: "bbb2222", summary: "add b", authorName: "Rene", timestamp: 2 },
  { id: "ccc", shortId: "ccc3333", summary: "add c", authorName: "Rene", timestamp: 3 },
];

const fourCommits: RebasePlanCommit[] = [
  { id: "aaa", shortId: "aaa1111", summary: "add a", authorName: "Rene", timestamp: 1 },
  { id: "bbb", shortId: "bbb2222", summary: "add b", authorName: "Rene", timestamp: 2 },
  { id: "ccc", shortId: "ccc3333", summary: "add c", authorName: "Rene", timestamp: 3 },
  { id: "ddd", shortId: "ddd4444", summary: "add d", authorName: "Rene", timestamp: 4 },
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

  it("a Drop between the leader and a Squash row doesn't steal the combined-message field", async () => {
    // Regression test: `recomputeGroupLeaders` used to treat the row immediately before a
    // Squash/Fixup row as the group leader without checking whether that row was itself a
    // Drop. `git-core::rebase`'s `land_current_step` always walks past Drop entries to find
    // the real leader (the nearest Pick/Reword/Edit), so a UI mismatch here meant the combined
    // message a user typed landed on a row the backend never reads — see
    // e2e/specs/rebase.spec.ts for the black-box repro this was caught by.
    const onStartRebase = vi.fn();
    const client = fakeClient({ commitsSince: async () => threeCommits });

    render(
      <RebasePlanner
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
      />,
    );
    await screen.findByText(/add a/);

    // Row 0 = "add a" (Pick, the real leader), row 1 = "add b" (Drop), row 2 = "add c" (Squash).
    fireEvent.change(screen.getAllByLabelText("Action")[1], { target: { value: "Drop" } });
    fireEvent.change(screen.getAllByLabelText("Action")[2], { target: { value: "Squash" } });

    // Exactly one combined-message field should exist, pre-filled from the leader ("add a")
    // and the surviving squash member ("add c") — not from the dropped row.
    const combinedFields = await screen.findAllByLabelText("Combined message");
    expect(combinedFields).toHaveLength(1);
    expect(combinedFields[0]).toHaveValue("add a\n\nadd c");

    fireEvent.change(combinedFields[0], { target: { value: "e2e: combined rebase commit" } });
    fireEvent.click(screen.getByText("Start Rebase"));

    const [, plan] = onStartRebase.mock.calls[0];
    expect(plan[0].commitId).toBe("aaa");
    expect(plan[0].combinedMessage).toBe("e2e: combined rebase commit");
    expect(plan[1].commitId).toBe("bbb");
    expect(plan[1].action).toEqual({ kind: "Drop" });
    expect(plan[2].commitId).toBe("ccc");
    expect(plan[2].combinedMessage).toBeNull();
  });

  it("keeps a hand-edited combined message when an unrelated group's action changes", async () => {
    // Regression test: `recomputeGroupLeaders` used to blank *every* leader's combinedMessage and
    // recompute defaults on any row change, so typing a custom message into one group and then
    // touching a different group silently threw the typed message away.
    const onStartRebase = vi.fn();
    const client = fakeClient({ commitsSince: async () => fourCommits });

    render(
      <RebasePlanner
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
      />,
    );
    await screen.findByText(/add a/);

    // Group A: leader "add a" + squashed "add b". Group B: leader "add c" + squashed "add d".
    fireEvent.change(screen.getAllByLabelText("Action")[1], { target: { value: "Squash" } });
    fireEvent.change(screen.getAllByLabelText("Action")[3], { target: { value: "Squash" } });
    let combinedFields = await screen.findAllByLabelText("Combined message");
    expect(combinedFields).toHaveLength(2);

    fireEvent.change(combinedFields[0], { target: { value: "hand-written message for A" } });

    // Change group B only: its member switches Squash → Fixup, which legitimately changes B's
    // default message but says nothing about A.
    fireEvent.change(screen.getAllByLabelText("Action")[3], { target: { value: "Fixup" } });

    combinedFields = screen.getAllByLabelText("Combined message");
    expect(combinedFields[0]).toHaveValue("hand-written message for A");
    // Group B genuinely changed, so it does get a fresh default (a Fixup member contributes no
    // message text).
    expect(combinedFields[1]).toHaveValue("add c");

    fireEvent.click(screen.getByText("Start Rebase"));
    const [, plan] = onStartRebase.mock.calls[0];
    expect(plan[0].combinedMessage).toBe("hand-written message for A");
  });

  it("keeps a hand-edited combined message when unrelated rows are reordered", async () => {
    const client = fakeClient({ commitsSince: async () => fourCommits });

    render(
      <RebasePlanner client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );
    await screen.findByText(/add a/);

    // One group: leader "add a" + squashed "add b". Rows "add c"/"add d" are plain Picks.
    fireEvent.change(screen.getAllByLabelText("Action")[1], { target: { value: "Squash" } });
    const combined = await screen.findByLabelText("Combined message");
    fireEvent.change(combined, { target: { value: "hand-written message" } });

    // Reorder two rows outside the group — the group's membership is untouched.
    fireEvent.click(screen.getAllByText("Move down")[2]);

    expect(screen.getByLabelText("Combined message")).toHaveValue("hand-written message");
  });

  it("recomputes the default combined message when the group's own membership changes", async () => {
    const client = fakeClient({ commitsSince: async () => threeCommits });

    render(
      <RebasePlanner client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );
    await screen.findByText(/add a/);

    fireEvent.change(screen.getAllByLabelText("Action")[1], { target: { value: "Squash" } });
    fireEvent.change(await screen.findByLabelText("Combined message"), {
      target: { value: "stale hand-written message" },
    });

    // A new member joins this very group: its default is genuinely out of date now, so it's
    // recomputed rather than preserved.
    fireEvent.change(screen.getAllByLabelText("Action")[2], { target: { value: "Squash" } });

    expect(screen.getByLabelText("Combined message")).toHaveValue("add a\n\nadd b\n\nadd c");
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
