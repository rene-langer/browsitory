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

  it("Save resolution defaults to ours for every conflict and calls onResolve with the joined text", async () => {
    const onResolve = vi.fn();
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText(/line one/));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nmain two\nline three\n");
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
