import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RepoError, type ConflictSegment, type RepoClient } from "../ipc/RepoClient";
import { ConflictResolutionPane } from "./ConflictResolutionPane";
import { fakeRepoClient } from "../test/fakeRepoClient";

const TEST_REPO_PATH = "/repo";

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(overrides, "empty");
}

// Segment content carries embedded trailing newlines, mirroring what
// `parse_conflict_markers` (crates/git-core/src/merge.rs) actually produces via
// `split_inclusive('\n')` — each piece keeps its own line terminator.
const segments: ConflictSegment[] = [
  { kind: "Clean", content: "line one\n" },
  { kind: "Conflict", ours: "main two\n", theirs: "feature two\n" },
  { kind: "Clean", content: "line three\n" },
];

describe("ConflictResolutionPane", () => {
  it("renders clean segments as text and conflict segments with both sides", async () => {
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={vi.fn()} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText(/line one/));
    expect(screen.getByText(/main two/)).toBeInTheDocument();
    expect(screen.getByText(/feature two/)).toBeInTheDocument();
    expect(screen.getByText(/line three/)).toBeInTheDocument();
  });

  // #654: nothing is chosen for the user; one click on Save used to take ours everywhere.
  it("starts every conflict unresolved and keeps Save disabled until each one is chosen", async () => {
    const onResolve = vi.fn();
    const twoConflicts: ConflictSegment[] = [
      { kind: "Conflict", ours: "a ours\n", theirs: "a theirs\n" },
      { kind: "Clean", content: "middle\n" },
      { kind: "Conflict", ours: "b ours\n", theirs: "b theirs\n" },
    ];
    const client = fakeClient({ getConflictHunks: async () => twoConflicts });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });
    const save = screen.getByRole("button", { name: "Save resolution" });
    expect(screen.getByRole("status")).toHaveTextContent("0 of 2 resolved");
    expect(save).toBeDisabled();
    expect(screen.getByText("Choose a side for every conflict to save.")).toBeInTheDocument();
    for (const name of ["Accept ours", "Accept theirs", "Accept both"]) {
      expect(within(first).getByRole("button", { name })).toHaveAttribute("aria-pressed", "false");
    }
    expect(within(first).getByLabelText("Result")).toHaveTextContent("Not chosen yet");
    fireEvent.click(save);
    expect(onResolve).not.toHaveBeenCalled();

    fireEvent.click(within(first).getByRole("button", { name: "Accept theirs" }));
    expect(within(first).getByRole("button", { name: "Accept theirs" })).toHaveAttribute("aria-pressed", "true");
    expect(within(first).getByRole("button", { name: "Accept ours" })).toHaveAttribute("aria-pressed", "false");
    expect(within(first).getByLabelText("Result")).toHaveTextContent("a theirs");
    expect(screen.getByRole("status")).toHaveTextContent("1 of 2 resolved");
    expect(save).toBeDisabled();

    fireEvent.click(within(second).getByRole("button", { name: "Accept both" }));
    expect(within(second).getByLabelText("Result")).toHaveTextContent("b ours b theirs");
    expect(screen.getByRole("status")).toHaveTextContent("2 of 2 resolved");
    expect(save).toBeEnabled();
    fireEvent.click(save);
    expect(onResolve).toHaveBeenCalledWith("shared.txt", "a theirs\nmiddle\nb ours\nb theirs\n");
  });

  it("shows an empty choice as an empty result, not as unresolved", async () => {
    const client = fakeClient({
      getConflictHunks: async () => [{ kind: "Conflict", ours: "", theirs: "kept\n" }],
    });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={vi.fn()} onResolveAddDelete={vi.fn()} />);

    fireEvent.click(await screen.findByRole("button", { name: "Accept ours" }));
    expect(screen.getByLabelText("Result")).toHaveTextContent("Empty: the block's lines are removed");
    expect(screen.getByRole("button", { name: "Save resolution" })).toBeEnabled();
  });

  it("names the branches on each side of a merge conflict", async () => {
    const client = fakeClient({
      getConflictHunks: async () => segments,
      getHeadInfo: async () => ({ ...(await fakeClient().getHeadInfo(TEST_REPO_PATH)), operation: "Merge" }),
      listBranches: async () => [{ name: "main", isCurrent: true }],
      getMergeMessage: async () => "Merge branch 'feature'\n",
    });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={vi.fn()} onResolveAddDelete={vi.fn()} />);

    const block = await screen.findByRole("region", { name: "Conflict 1 of 1" });
    expect(await within(block).findByText("Ours (main)")).toBeInTheDocument();
    expect(within(block).getByText("Theirs (feature)")).toBeInTheDocument();
  });

  it("labels the sides plainly when the branch names can't be read", async () => {
    const client = fakeRepoClient({ getConflictHunks: async () => segments }, "throw");

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={vi.fn()} onResolveAddDelete={vi.fn()} />);

    const block = await screen.findByRole("region", { name: "Conflict 1 of 1" });
    expect(within(block).getByText("Ours")).toBeInTheDocument();
    expect(within(block).getByText("Theirs")).toBeInTheDocument();
  });

  it("moves between conflicts with Next and Previous conflict", async () => {
    const threeConflicts: ConflictSegment[] = [
      { kind: "Conflict", ours: "1o\n", theirs: "1t\n" },
      { kind: "Clean", content: "x\n" },
      { kind: "Conflict", ours: "2o\n", theirs: "2t\n" },
      { kind: "Conflict", ours: "3o\n", theirs: "3t\n" },
    ];
    const client = fakeClient({ getConflictHunks: async () => threeConflicts });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={vi.fn()} onResolveAddDelete={vi.fn()} />);

    await screen.findByRole("region", { name: "Conflict 1 of 3" });
    const next = screen.getByRole("button", { name: "Next conflict" });
    const previous = screen.getByRole("button", { name: "Previous conflict" });
    fireEvent.click(next);
    expect(screen.getByRole("region", { name: "Conflict 1 of 3" })).toHaveFocus();
    fireEvent.click(next);
    expect(screen.getByRole("region", { name: "Conflict 2 of 3" })).toHaveFocus();
    fireEvent.click(next);
    expect(screen.getByRole("region", { name: "Conflict 3 of 3" })).toHaveFocus();
    expect(next).toBeDisabled();
    fireEvent.click(previous);
    expect(screen.getByRole("region", { name: "Conflict 2 of 3" })).toHaveFocus();
  });

  it("Accept theirs changes that conflict's contribution to the saved text", async () => {
    const onResolve = vi.fn();
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Accept theirs"));
    fireEvent.click(screen.getByText("Accept theirs"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nfeature two\nline three\n");
  });

  it("Accept both concatenates ours then theirs", async () => {
    const onResolve = vi.fn();
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Accept both"));
    fireEvent.click(screen.getByText("Accept both"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith(
      "shared.txt",
      "line one\nmain two\nfeature two\nline three\n",
    );
  });

  it("Save resolution is disabled until the conflict hunks finish loading", async () => {
    const onResolve = vi.fn();
    let resolveHunks: (segments: ConflictSegment[]) => void = () => {};
    const client = fakeClient({
      getConflictHunks: () => new Promise((resolve) => (resolveHunks = resolve)),
    });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    const saveButton = screen.getByText("Save resolution").closest("button");
    expect(saveButton).toBeDisabled();
    fireEvent.click(screen.getByText("Save resolution"));
    expect(onResolve).not.toHaveBeenCalled();

    resolveHunks(segments);
    fireEvent.click(await screen.findByRole("button", { name: "Accept ours" }));
    await waitFor(() => expect(saveButton).not.toBeDisabled());
  });

  it("Accept both does not insert a spurious blank line when one side of a conflict is empty", async () => {
    const onResolve = vi.fn();
    const oneEmptySide: ConflictSegment[] = [
      { kind: "Clean", content: "line one\n" },
      { kind: "Conflict", ours: "line two\n", theirs: "" },
      { kind: "Clean", content: "line three\n" },
    ];
    const client = fakeClient({ getConflictHunks: async () => oneEmptySide });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Accept both"));
    fireEvent.click(screen.getByText("Accept both"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nline two\nline three\n");
  });

  it("Accept ours does not insert a spurious blank line when theirs is empty", async () => {
    const onResolve = vi.fn();
    const theirsEmpty: ConflictSegment[] = [
      { kind: "Clean", content: "line one\n" },
      { kind: "Conflict", ours: "line two\n", theirs: "" },
      { kind: "Clean", content: "line three\n" },
    ];
    const client = fakeClient({ getConflictHunks: async () => theirsEmpty });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Accept ours"));
    fireEvent.click(screen.getByText("Accept ours"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nline two\nline three\n");
  });

  it("Accept theirs does not insert a spurious blank line when ours is empty", async () => {
    const onResolve = vi.fn();
    const oursEmpty: ConflictSegment[] = [
      { kind: "Clean", content: "line one\n" },
      { kind: "Conflict", ours: "", theirs: "line two\n" },
      { kind: "Clean", content: "line three\n" },
    ];
    const client = fakeClient({ getConflictHunks: async () => oursEmpty });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Accept theirs"));
    fireEvent.click(screen.getByText("Accept theirs"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nline two\nline three\n");
  });

  it("shows a keep-ours/keep-theirs/delete fallback for an add/delete conflict", async () => {
    const client = fakeClient({
      getConflictHunks: async () => {
        throw new RepoError("AddDeleteConflict", "'binary.dat' is an add/delete conflict, not a text conflict");
      },
    });

    render(
      <ConflictResolutionPane
        repoPath={TEST_REPO_PATH}
        client={client}
        path="binary.dat"
        onResolve={vi.fn()}
        onResolveAddDelete={vi.fn()}
      />,
    );

    await waitFor(() => screen.getByText("Keep our version"));
    expect(screen.getByText("Keep their version")).toBeInTheDocument();
    expect(screen.getByText("Delete file")).toBeInTheDocument();
  });

  // #496: the kind decides, not the text; the old substring check matched this one.
  it("shows any other failure as an error, even one that mentions add/delete conflicts", async () => {
    const client = fakeClient({
      getConflictHunks: async () => {
        throw new RepoError("Other", "'binary.dat' is a text conflict, not an add/delete conflict");
      },
    });

    render(
      <ConflictResolutionPane
        repoPath={TEST_REPO_PATH}
        client={client}
        path="binary.dat"
        onResolve={vi.fn()}
        onResolveAddDelete={vi.fn()}
      />,
    );

    expect(await screen.findByText("'binary.dat' is a text conflict, not an add/delete conflict")).toBeInTheDocument();
    expect(screen.queryByText("Keep our version")).not.toBeInTheDocument();
  });

  it("clicking Keep their version calls onResolveAddDelete with Theirs", async () => {
    const onResolveAddDelete = vi.fn();
    const client = fakeClient({
      getConflictHunks: async () => {
        throw new RepoError("AddDeleteConflict", "'binary.dat' is an add/delete conflict, not a text conflict");
      },
    });

    render(
      <ConflictResolutionPane
        repoPath={TEST_REPO_PATH}
        client={client}
        path="binary.dat"
        onResolve={vi.fn()}
        onResolveAddDelete={onResolveAddDelete}
      />,
    );

    await waitFor(() => screen.getByText("Keep their version"));
    fireEvent.click(screen.getByText("Keep their version"));

    expect(onResolveAddDelete).toHaveBeenCalledWith("binary.dat", "Theirs");
  });

  it("still shows a plain error message for an unrelated failure", async () => {
    const client = fakeClient({
      getConflictHunks: async () => {
        throw new Error("network error");
      },
    });

    render(
      <ConflictResolutionPane
        repoPath={TEST_REPO_PATH}
        client={client}
        path="shared.txt"
        onResolve={vi.fn()}
        onResolveAddDelete={vi.fn()}
      />,
    );

    await waitFor(() => screen.getByRole("alert"));
    expect(screen.getByRole("alert")).toHaveTextContent("network error");
    expect(screen.queryByText("Keep our version")).not.toBeInTheDocument();
  });
});

describe("ConflictResolutionPane whole-file header", () => {
  function renderPane(getConflictHunks: RepoClient["getConflictHunks"]) {
    const onResolveWholeFile = vi.fn();
    const onMarkResolved = vi.fn();
    render(
      <ConflictResolutionPane
        repoPath={TEST_REPO_PATH}
        client={fakeClient({ getConflictHunks })}
        path="shared.txt"
        onResolve={vi.fn()}
        onResolveAddDelete={vi.fn()}
        onResolveWholeFile={onResolveWholeFile}
        onMarkResolved={onMarkResolved}
      />,
    );
    return { onResolveWholeFile, onMarkResolved };
  }

  it("offers whole-file ours/theirs and Mark resolved for a text conflict", async () => {
    const { onResolveWholeFile, onMarkResolved } = renderPane(async () => segments);
    const toolbar = await screen.findByRole("group", { name: "Whole file" });
    const ours = within(toolbar).getByRole("button", { name: "Use ours (whole file)" });
    await waitFor(() => expect(ours).toBeEnabled());
    expect(ours).toHaveAttribute("title", "Replace the file with the current branch's version (HEAD).");
    expect(within(toolbar).getByRole("button", { name: "Use theirs (whole file)" })).toHaveAttribute(
      "title",
      "Replace the file with the incoming version.",
    );
    fireEvent.click(ours);
    fireEvent.click(within(toolbar).getByRole("button", { name: "Use theirs (whole file)" }));
    expect(onResolveWholeFile.mock.calls).toEqual([
      ["shared.txt", "Ours"],
      ["shared.txt", "Theirs"],
    ]);
    fireEvent.click(within(toolbar).getByRole("button", { name: "Mark resolved" }));
    expect(onMarkResolved).toHaveBeenCalledTimes(1);
  });

  it("disables the header until the conflict has loaded", () => {
    renderPane(() => new Promise(() => {}));
    expect(screen.getByRole("button", { name: "Use ours (whole file)" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Mark resolved" })).toBeDisabled();
  });

  it("keeps the header for add/delete conflicts next to their own buttons", async () => {
    renderPane(() => Promise.reject(new RepoError("AddDeleteConflict", "'x' is an add/delete conflict, not a text conflict")));
    expect(await screen.findByRole("button", { name: "Keep our version" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mark resolved" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Use theirs (whole file)" })).toBeEnabled();
  });

  it("keeps the header when the hunks fail for another reason (binary conflict)", async () => {
    const { onResolveWholeFile } = renderPane(() => Promise.reject("stream did not contain valid UTF-8"));
    expect(await screen.findByRole("alert")).toHaveTextContent("stream did not contain valid UTF-8");
    fireEvent.click(screen.getByRole("button", { name: "Use theirs (whole file)" }));
    expect(onResolveWholeFile).toHaveBeenCalledWith("shared.txt", "Theirs");
  });
});
