import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RebasePlanCommit, RepoClient } from "../ipc/RepoClient";
import { RebasePlanner } from "./RebasePlanner";
import { fakeRepoClient } from "../test/fakeRepoClient";

const TEST_REPO_PATH = "/repo";

// A commit's full message is its summary unless a test says otherwise (#721).
function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(
    {
      getCommitMessage: async (_repo: string, id: string) =>
        `${fourCommits.find((commit) => commit.id === id)?.summary ?? id}\n`,
      ...overrides,
    },
    "empty",
  );
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
  it("disables Start rebase while another repository operation is active", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={vi.fn()}
        onCancel={vi.fn()}
        blockedReason="A merge is in progress."
      />,
    );
    await screen.findByText(/add a/);

    expect(screen.getByText("Start rebase")).toBeDisabled();
    // #521: the reason shows, as a title and as text.
    expect(screen.getByText("Start rebase")).toHaveAttribute("title", "A merge is in progress.");
    expect(screen.getByText("A merge is in progress.")).toBeInTheDocument();
  });

  it("lists commits oldest-first with a default Pick action each", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );

    expect(await screen.findByText(/add a/)).toBeInTheDocument();
    expect(screen.getByText(/add b/)).toBeInTheDocument();
  });

  it("moving a row down reorders the plan sent to onStartRebase", async () => {
    const onStartRebase = vi.fn();
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
      />,
    );
    await screen.findByText(/add a/);

    fireEvent.click(screen.getAllByText("Move down")[0]);
    fireEvent.click(screen.getByText("Start rebase"));

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
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
      />,
    );
    await screen.findByText(/add a/);

    fireEvent.change(screen.getAllByLabelText(/^Action for /)[0], { target: { value: "Reword" } });
    fireEvent.change(await screen.findByPlaceholderText("New commit message"), {
      target: { value: "reworded" },
    });
    fireEvent.click(screen.getByText("Start rebase"));

    const [, plan] = onStartRebase.mock.calls[0];
    expect(plan[0].action).toEqual({ kind: "Reword", message: "reworded" });
  });

  it("attaching a Squash row reveals the leader's combined-message field, pre-filled", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );
    await screen.findByText(/add a/);

    fireEvent.change(screen.getAllByLabelText(/^Action for /)[1], { target: { value: "Squash" } });

    const combined = await screen.findByLabelText("Combined message");
    expect(combined).toHaveValue("add a\n\nadd b");
  });

  // #721: the default used to hold only the summaries, so the bodies were lost.
  it("pre-fills the combined message with the group's full messages, bodies included", async () => {
    const onStartRebase = vi.fn();
    const bodies: Record<string, string> = {
      aaa: "add a\n\nWhy a.\n",
      bbb: "add b\n\nWhy b.\nSigned-off-by: Rene\n",
      ccc: "add c\n\nWhy c.\n",
    };
    const getCommitMessage = vi.fn(async (_repo: string, id: string) => bodies[id]);
    const client = fakeClient({ commitsSince: async () => threeCommits, getCommitMessage });

    render(<RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={onStartRebase} onCancel={vi.fn()} />);
    await screen.findByText(/add a/);
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[1], { target: { value: "Squash" } });
    // A Fixup's message is left out, as `git rebase -i` does.
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[2], { target: { value: "Fixup" } });

    const combined = await screen.findByLabelText("Combined message");
    await waitFor(() =>
      expect(combined).toHaveValue("add a\n\nWhy a.\n\nadd b\n\nWhy b.\nSigned-off-by: Rene"),
    );
    expect(getCommitMessage).not.toHaveBeenCalledWith(TEST_REPO_PATH, "ccc");

    fireEvent.click(screen.getByRole("button", { name: "Start rebase" }));
    const [, plan] = onStartRebase.mock.calls[0];
    expect(plan[0].combinedMessage).toBe("add a\n\nWhy a.\n\nadd b\n\nWhy b.\nSigned-off-by: Rene");
    expect(plan[1].combinedMessage).toBeNull();
    expect(plan[2].combinedMessage).toBeNull();
  });

  it("keeps Start disabled while a group's full messages load", async () => {
    const resolvers: Array<() => void> = [];
    const client = fakeClient({
      commitsSince: async () => commits,
      getCommitMessage: (_repo, id) =>
        new Promise((resolve) => resolvers.push(() => resolve(`${id} summary\n\n${id} body\n`))),
    });

    render(<RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByText(/add a/);
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[1], { target: { value: "Squash" } });

    expect(screen.getByText("Loading the full messages…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeDisabled();
    resolvers.forEach((resolve) => resolve());
    await waitFor(() =>
      expect(screen.getByLabelText("Combined message")).toHaveValue(
        "aaa summary\n\naaa body\n\nbbb summary\n\nbbb body",
      ),
    );
    expect(screen.queryByText("Loading the full messages…")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeEnabled();
  });

  it("uses the summary, and says so, when a squashed commit's full message can't be read", async () => {
    const client = fakeClient({
      commitsSince: async () => commits,
      getCommitMessage: async (_repo, id) => {
        if (id === "bbb") throw new Error("object not found");
        return "add a\n\nWhy a.\n";
      },
    });

    render(<RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByText(/add a/);
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[1], { target: { value: "Squash" } });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Couldn't read the full message of bbb2222 (object not found). Only its summary is in the combined message",
    );
    expect(screen.getByLabelText("Combined message")).toHaveValue("add a\n\nWhy a.\n\nadd b");
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeEnabled();
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
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
      />,
    );
    await screen.findByText(/add a/);

    // Row 0 = "add a" (Pick, the real leader), row 1 = "add b" (Drop), row 2 = "add c" (Squash).
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[1], { target: { value: "Drop" } });
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[2], { target: { value: "Squash" } });

    // Exactly one combined-message field should exist, pre-filled from the leader ("add a")
    // and the surviving squash member ("add c") — not from the dropped row.
    const combinedFields = await screen.findAllByLabelText("Combined message");
    expect(combinedFields).toHaveLength(1);
    expect(combinedFields[0]).toHaveValue("add a\n\nadd c");

    fireEvent.change(combinedFields[0], { target: { value: "e2e: combined rebase commit" } });
    fireEvent.click(screen.getByText("Start rebase"));

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
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
      />,
    );
    await screen.findByText(/add a/);

    // Group A: leader "add a" + squashed "add b". Group B: leader "add c" + squashed "add d".
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[1], { target: { value: "Squash" } });
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[3], { target: { value: "Squash" } });
    let combinedFields = await screen.findAllByLabelText("Combined message");
    expect(combinedFields).toHaveLength(2);

    fireEvent.change(combinedFields[0], { target: { value: "hand-written message for A" } });

    // Change group B only: its member switches Squash → Fixup, which legitimately changes B's
    // default message but says nothing about A.
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[3], { target: { value: "Fixup" } });

    combinedFields = screen.getAllByLabelText("Combined message");
    expect(combinedFields[0]).toHaveValue("hand-written message for A");
    // Group B genuinely changed, so it does get a fresh default (a Fixup member contributes no
    // message text).
    expect(combinedFields[1]).toHaveValue("add c");

    fireEvent.click(screen.getByText("Start rebase"));
    const [, plan] = onStartRebase.mock.calls[0];
    expect(plan[0].combinedMessage).toBe("hand-written message for A");
  });

  it("keeps a hand-edited combined message when unrelated rows are reordered", async () => {
    const client = fakeClient({ commitsSince: async () => fourCommits });

    render(
      <RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );
    await screen.findByText(/add a/);

    // One group: leader "add a" + squashed "add b". Rows "add c"/"add d" are plain Picks.
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[1], { target: { value: "Squash" } });
    const combined = await screen.findByLabelText("Combined message");
    fireEvent.change(combined, { target: { value: "hand-written message" } });

    // Reorder two rows outside the group — the group's membership is untouched.
    fireEvent.click(screen.getAllByText("Move down")[2]);

    expect(screen.getByLabelText("Combined message")).toHaveValue("hand-written message");
  });

  it("recomputes the default combined message when the group's own membership changes", async () => {
    const client = fakeClient({ commitsSince: async () => threeCommits });

    render(
      <RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );
    await screen.findByText(/add a/);

    fireEvent.change(screen.getAllByLabelText(/^Action for /)[1], { target: { value: "Squash" } });
    fireEvent.change(await screen.findByLabelText("Combined message"), {
      target: { value: "stale hand-written message" },
    });

    // A new member joins this very group: its default is genuinely out of date now, so it's
    // recomputed rather than preserved.
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[2], { target: { value: "Squash" } });

    expect(screen.getByLabelText("Combined message")).toHaveValue("add a\n\nadd b\n\nadd c");
  });

  it("disables Squash and Fixup on the first row", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />,
    );
    await screen.findByText(/add a/);

    const firstRowActionSelect = screen.getAllByLabelText(/^Action for /)[0] as HTMLSelectElement;
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
      <RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={onCancel} />,
    );
    await screen.findByText(/add a/);

    fireEvent.click(screen.getByText("Cancel"));

    expect(onCancel).toHaveBeenCalled();
  });

  it("pre-marks rows named in presetActions, with the leader's combined message filled in", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={vi.fn()}
        onCancel={vi.fn()}
        presetActions={new Map([["bbb", "Squash"]])}
      />,
    );
    await screen.findAllByLabelText("Action");

    // Row 0 = "add a" (Pick, the leader), row 1 = "add b" (preset Squash).
    expect(screen.getAllByLabelText(/^Action for /)[0]).toHaveValue("Pick");
    expect(screen.getAllByLabelText(/^Action for /)[1]).toHaveValue("Squash");

    const combinedFields = await screen.findAllByLabelText("Combined message");
    expect(combinedFields).toHaveLength(1);
    expect(combinedFields[0]).toHaveValue("add a\n\nadd b");
  });

  it("pre-marks a preset Drop on a non-adjacent row and leaves the others as Pick", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={vi.fn()}
        onCancel={vi.fn()}
        presetActions={new Map([["aaa", "Drop"]])}
      />,
    );
    await screen.findAllByLabelText("Action");

    expect(screen.getAllByLabelText(/^Action for /)[0]).toHaveValue("Drop");
    expect(screen.getAllByLabelText(/^Action for /)[1]).toHaveValue("Pick");
  });

  it("blocks Start and explains when a preset commit is not in the planned history", async () => {
    const client = fakeClient({ commitsSince: async () => commits });
    const onStartRebase = vi.fn();

    render(
      <RebasePlanner
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
        presetActions={new Map([["not-on-this-branch", "Drop"]])}
      />,
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Some selected commits are not on the current branch",
    );
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeDisabled();
  });

  it("also blocks Start when an all-Pick preset names a commit outside the planned history", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={vi.fn()}
        onCancel={vi.fn()}
        presetActions={new Map([["aaa", "Pick"], ["not-on-this-branch", "Pick"]])}
      />,
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Some selected commits are not on the current branch",
    );
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeDisabled();
  });

  // #655: the message field used to hold only the summary, and the backend replaces the whole
  // message, so rewording silently dropped the body.
  it("loads the full message, body included, into the reword field", async () => {
    const onStartRebase = vi.fn();
    const getCommitMessage = vi.fn(async (_repo: string, id: string) =>
      id === "aaa" ? "add a\n\nWhy a matters.\nSigned-off-by: Rene\n" : "add b\n",
    );
    const client = fakeClient({ commitsSince: async () => commits, getCommitMessage });

    render(<RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={onStartRebase} onCancel={vi.fn()} />);
    await screen.findByText(/add a/);

    fireEvent.change(screen.getByRole("combobox", { name: "Action for aaa1111 add a" }), { target: { value: "Reword" } });
    const field = await screen.findByRole("textbox", { name: "New message for aaa1111" });
    await waitFor(() => expect(field).toHaveValue("add a\n\nWhy a matters.\nSigned-off-by: Rene\n"));
    expect(field.tagName).toBe("TEXTAREA");
    expect(getCommitMessage).toHaveBeenCalledWith(TEST_REPO_PATH, "aaa");

    fireEvent.click(screen.getByRole("button", { name: "Start rebase" }));
    const [, plan] = onStartRebase.mock.calls[0];
    expect(plan[0].action).toEqual({ kind: "Reword", message: "add a\n\nWhy a matters.\nSigned-off-by: Rene\n" });
  });

  it("keeps Start disabled while a reworded commit's message loads", async () => {
    let resolveMessage: (message: string) => void = () => {};
    const client = fakeClient({
      commitsSince: async () => commits,
      getCommitMessage: () => new Promise((resolve) => (resolveMessage = resolve)),
    });

    render(<RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByText(/add a/);
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[0], { target: { value: "Reword" } });

    expect(screen.getByText("Loading the message…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeDisabled();
    resolveMessage("add a\n\nbody\n");
    expect(await screen.findByRole("textbox", { name: "New message for aaa1111" })).toHaveValue("add a\n\nbody\n");
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeEnabled();
  });

  it("falls back to the summary, and says so, when the full message can't be read", async () => {
    const client = fakeClient({
      commitsSince: async () => commits,
      getCommitMessage: async () => {
        throw new Error("object not found");
      },
    });

    render(<RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByText(/add a/);
    fireEvent.change(screen.getAllByLabelText(/^Action for /)[0], { target: { value: "Reword" } });

    expect(await screen.findByRole("textbox", { name: "New message for aaa1111" })).toHaveValue("add a");
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Couldn't read the full message of aaa1111 (object not found). Only the summary is filled in; add the body back if it had one.",
    );
  });

  it("shows a loading state, with Start disabled, until the commits arrive", async () => {
    let resolveCommits: (value: RebasePlanCommit[]) => void = () => {};
    const client = fakeClient({ commitsSince: () => new Promise((resolve) => (resolveCommits = resolve)) });

    render(<RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />);

    expect(screen.getByText("Loading commits…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeDisabled();
    resolveCommits(commits);
    expect(await screen.findByText(/add a/)).toBeInTheDocument();
    expect(screen.queryByText("Loading commits…")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeEnabled();
  });

  it("shows a failure to list the commits, with Retry, and keeps Start disabled", async () => {
    const commitsSince = vi
      .fn<RepoClient["commitsSince"]>()
      .mockRejectedValueOnce(new Error("revspec 'base' not found"))
      .mockResolvedValueOnce(commits);
    const client = fakeClient({ commitsSince });

    render(<RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("revspec 'base' not found");
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText(/add a/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeEnabled();
  });

  it("says when there is nothing to rebase, and keeps Start disabled", async () => {
    const client = fakeClient({ commitsSince: async () => [] });

    render(<RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />);

    expect(await screen.findByText("No commits to rebase: the current branch has nothing above this base.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeDisabled();
  });

  it("says the rows are oldest first and names each row's controls after its commit", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(<RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={vi.fn()} onCancel={vi.fn()} />);
    await screen.findByText(/add a/);

    expect(screen.getByText(/Oldest commit first/)).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Action for bbb2222 add b" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Move up: aaa1111 add a" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Move up: bbb2222 add b" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Move down: aaa1111 add a" })).toBeEnabled();
  });

  it("moves a row with Alt+Up and Alt+Down, keeping focus on it", async () => {
    const onStartRebase = vi.fn();
    const client = fakeClient({ commitsSince: async () => threeCommits });

    render(<RebasePlanner repoPath={TEST_REPO_PATH} client={client} onto="base" onStartRebase={onStartRebase} onCancel={vi.fn()} />);
    await screen.findByText(/add a/);

    const select = screen.getByRole("combobox", { name: "Action for aaa1111 add a" });
    select.focus();
    fireEvent.keyDown(select, { key: "ArrowDown", altKey: true });
    fireEvent.keyDown(select, { key: "ArrowDown", altKey: true });
    expect(select).toHaveFocus();
    fireEvent.keyDown(select, { key: "ArrowUp", altKey: true });
    // A plain arrow key belongs to the control (the select changes its value), not to the row.
    fireEvent.keyDown(select, { key: "ArrowDown" });
    const rows = within(screen.getByRole("list")).getAllByRole("listitem");
    expect(rows.map((row) => row.textContent?.slice(0, 7))).toEqual(["bbb2222", "aaa1111", "ccc3333"]);
    expect(select).toHaveFocus();
  });
});
