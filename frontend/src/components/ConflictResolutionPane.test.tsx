import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RepoError, type ConflictSegment, type RepoClient } from "../ipc/RepoClient";
import { ConflictResolutionPane } from "./ConflictResolutionPane";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { DEFAULT_DIFF_SETTINGS } from "../ipc/generated/wire-constants";
import { DiffSettingsContext } from "../state/useDiffSettings";
import { MergeEditorContext, useMergeEditorRegistry, type MergeEditorRegistry } from "../state/useCurrentMergeEditor";

const TEST_REPO_PATH = "/repo";

// These tests describe the block view (#720). The three-pane view is the default since #875;
// the block view stays behind the layout switch, remembered per viewer in localStorage.
beforeEach(() => {
  localStorage.setItem("mergeEditor.view", "blocks");
});

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(overrides, "empty");
}

// Segment content carries embedded trailing newlines, mirroring what
// `parse_conflict_markers` (crates/git-core/src/merge.rs) actually produces via
// `split_inclusive('\n')` — each piece keeps its own line terminator.
const segments: ConflictSegment[] = [
  { kind: "Clean", content: "line one\n" },
  { kind: "Conflict", ours: "main two\n", base: "line two\n", theirs: "feature two\n" },
  { kind: "Clean", content: "line three\n" },
];

describe("ConflictResolutionPane", () => {
  it("renders clean segments as text and conflict segments with both sides", async () => {
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={vi.fn()} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText(/line one/));
    // A side's line is cut where its changed words are marked (#879): match each side as a whole.
    const sides = [...document.querySelectorAll("pre")].map((part) => part.textContent);
    expect(sides).toContainEqual(expect.stringMatching(/main two/));
    expect(sides).toContainEqual(expect.stringMatching(/feature two/));
    expect(screen.getByText(/line three/)).toBeInTheDocument();
  });

  // #654: nothing is chosen for the user; one click on Save used to take ours everywhere.
  it("starts every conflict unresolved and keeps Save disabled until each one is chosen", async () => {
    const onResolve = vi.fn();
    const twoConflicts: ConflictSegment[] = [
      { kind: "Conflict", ours: "a ours\n", base: "", theirs: "a theirs\n" },
      { kind: "Clean", content: "middle\n" },
      { kind: "Conflict", ours: "b ours\n", base: "", theirs: "b theirs\n" },
    ];
    const client = fakeClient({ getConflictHunks: async () => twoConflicts });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });
    const save = screen.getByRole("button", { name: "Save resolution" });
    expect(screen.getByRole("status")).toHaveTextContent("0 of 2 resolved");
    expect(save).toBeDisabled();
    expect(screen.getByText("Choose or type a result for every conflict to save.")).toBeInTheDocument();
    for (const name of ["Accept ours", "Accept theirs", "Ours then theirs", "Theirs then ours", "Accept base"]) {
      expect(within(first).getByRole("button", { name })).toHaveAttribute("aria-pressed", "false");
    }
    expect(within(first).getByLabelText("Result")).toHaveValue("");
    expect(within(first).getByText("Not chosen")).toBeInTheDocument();
    fireEvent.click(save);
    expect(onResolve).not.toHaveBeenCalled();

    fireEvent.click(within(first).getByRole("button", { name: "Accept theirs" }));
    expect(within(first).getByRole("button", { name: "Accept theirs" })).toHaveAttribute("aria-pressed", "true");
    expect(within(first).getByRole("button", { name: "Accept ours" })).toHaveAttribute("aria-pressed", "false");
    expect(within(first).getByLabelText("Result")).toHaveValue("a theirs\n");
    expect(screen.getByRole("status")).toHaveTextContent("1 of 2 resolved");
    expect(save).toBeDisabled();

    fireEvent.click(within(second).getByRole("button", { name: "Ours then theirs" }));
    expect(within(second).getByLabelText("Result")).toHaveValue("b ours\nb theirs\n");
    expect(screen.getByRole("status")).toHaveTextContent("2 of 2 resolved");
    expect(save).toBeEnabled();
    fireEvent.click(save);
    expect(onResolve).toHaveBeenCalledWith("shared.txt", "a theirs\nmiddle\nb ours\nb theirs\n");
  });

  it("shows an empty choice as an empty result, not as unresolved", async () => {
    const client = fakeClient({
      getConflictHunks: async () => [{ kind: "Conflict", ours: "", base: "", theirs: "kept\n" }],
    });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={vi.fn()} onResolveAddDelete={vi.fn()} />);

    fireEvent.click(await screen.findByRole("button", { name: "Accept ours" }));
    expect(screen.getByLabelText("Result")).toHaveValue("");
    expect(screen.getByText("Empty: the block's lines are removed.")).toBeInTheDocument();
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
      { kind: "Conflict", ours: "1o\n", base: "", theirs: "1t\n" },
      { kind: "Clean", content: "x\n" },
      { kind: "Conflict", ours: "2o\n", base: "", theirs: "2t\n" },
      { kind: "Conflict", ours: "3o\n", base: "", theirs: "3t\n" },
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

  it("Ours then theirs concatenates ours then theirs", async () => {
    const onResolve = vi.fn();
    const client = fakeClient({ getConflictHunks: async () => segments });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Ours then theirs"));
    fireEvent.click(screen.getByText("Ours then theirs"));
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

  it("Ours then theirs does not insert a spurious blank line when one side of a conflict is empty", async () => {
    const onResolve = vi.fn();
    const oneEmptySide: ConflictSegment[] = [
      { kind: "Clean", content: "line one\n" },
      { kind: "Conflict", ours: "line two\n", base: "", theirs: "" },
      { kind: "Clean", content: "line three\n" },
    ];
    const client = fakeClient({ getConflictHunks: async () => oneEmptySide });

    render(<ConflictResolutionPane repoPath={TEST_REPO_PATH} client={client} path="shared.txt" onResolve={onResolve} onResolveAddDelete={vi.fn()} />);

    await waitFor(() => screen.getByText("Ours then theirs"));
    fireEvent.click(screen.getByText("Ours then theirs"));
    fireEvent.click(screen.getByText("Save resolution"));

    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nline two\nline three\n");
  });

  it("Accept ours does not insert a spurious blank line when theirs is empty", async () => {
    const onResolve = vi.fn();
    const theirsEmpty: ConflictSegment[] = [
      { kind: "Clean", content: "line one\n" },
      { kind: "Conflict", ours: "line two\n", base: "", theirs: "" },
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
      { kind: "Conflict", ours: "", base: "", theirs: "line two\n" },
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

describe("ConflictResolutionPane merge editor (#720)", () => {
  function renderPane(conflict: ConflictSegment[]) {
    const onResolve = vi.fn();
    render(
      <ConflictResolutionPane
        repoPath={TEST_REPO_PATH}
        client={fakeClient({ getConflictHunks: async () => conflict })}
        path="shared.txt"
        onResolve={onResolve}
        onResolveAddDelete={vi.fn()}
      />,
    );
    return { onResolve };
  }

  it("shows each block's base behind Show base, and can take it", async () => {
    const { onResolve } = renderPane(segments);
    const block = await screen.findByRole("region", { name: "Conflict 1 of 1" });
    expect(within(block).queryByText("Base")).not.toBeInTheDocument();

    const toggle = screen.getByRole("button", { name: "Show base" });
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(within(block).getByText("Base")).toBeInTheDocument();
    expect(within(block).getByText("line two")).toBeInTheDocument();

    fireEvent.click(within(block).getByRole("button", { name: "Accept base" }));
    expect(within(block).getByLabelText("Result")).toHaveValue("line two\n");
    fireEvent.click(screen.getByRole("button", { name: "Save resolution" }));
    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nline two\nline three\n");
  });

  it("says when a block has no base, as in an add/add conflict", async () => {
    renderPane([{ kind: "Conflict", ours: "o\n", base: "", theirs: "t\n" }]);
    fireEvent.click(await screen.findByRole("button", { name: "Show base" }));
    expect(screen.getByText("Empty: no common lines")).toBeInTheDocument();
  });

  it("Theirs then ours puts their lines first", async () => {
    const { onResolve } = renderPane(segments);
    fireEvent.click(await screen.findByRole("button", { name: "Theirs then ours" }));
    expect(screen.getByText("Theirs then ours", { selector: "span" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Save resolution" }));
    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\nfeature two\nmain two\nline three\n");
  });

  it("typing in a block's result resolves it as edited, and Reset clears it", async () => {
    const { onResolve } = renderPane(segments);
    const block = await screen.findByRole("region", { name: "Conflict 1 of 1" });
    const reset = within(block).getByRole("button", { name: "Reset" });
    expect(reset).toBeDisabled();

    fireEvent.click(within(block).getByRole("button", { name: "Accept ours" }));
    fireEvent.change(within(block).getByLabelText("Result"), { target: { value: "main two, fixed\n" } });
    expect(within(block).getByText("Edited")).toBeInTheDocument();
    expect(within(block).getByRole("button", { name: "Accept ours" })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByRole("status")).toHaveTextContent("1 of 1 resolved");

    fireEvent.click(reset);
    expect(within(block).getByLabelText("Result")).toHaveValue("");
    expect(screen.getByRole("status")).toHaveTextContent("0 of 1 resolved");
    expect(screen.getByRole("button", { name: "Save resolution" })).toBeDisabled();

    fireEvent.change(within(block).getByLabelText("Result"), { target: { value: "typed\n" } });
    fireEvent.click(screen.getByRole("button", { name: "Save resolution" }));
    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one\ntyped\nline three\n");
  });

  it("collapses a long unchanged run between conflicts and shows it on request", async () => {
    const lines = Array.from({ length: 12 }, (_, i) => `same ${i + 1}\n`).join("");
    renderPane([
      { kind: "Conflict", ours: "a\n", base: "", theirs: "b\n" },
      { kind: "Clean", content: lines },
      { kind: "Conflict", ours: "c\n", base: "", theirs: "d\n" },
    ]);
    const expand = await screen.findByRole("button", { name: "Show 6 unchanged lines" });
    expect(screen.getByText(/same 3/)).toBeInTheDocument();
    expect(screen.queryByText(/same 6/)).not.toBeInTheDocument();
    expect(screen.getByText(/same 10/)).toBeInTheDocument();

    fireEvent.click(expand);
    expect(screen.getByText(/same 6/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /unchanged lines/ })).not.toBeInTheDocument();
  });

  it("keeps only the lines next to the conflict of a long run at the file's start", async () => {
    const lines = Array.from({ length: 10 }, (_, i) => `head ${i + 1}\n`).join("");
    renderPane([
      { kind: "Clean", content: lines },
      { kind: "Conflict", ours: "a\n", base: "", theirs: "b\n" },
    ]);
    expect(await screen.findByRole("button", { name: "Show 7 unchanged lines" })).toBeInTheDocument();
    expect(screen.queryByText(/head 1\n/)).not.toBeInTheDocument();
    expect(screen.getByText(/head 8/)).toBeInTheDocument();
  });

  it("edits the whole result, with markers around unchosen blocks, and saves it once they are gone", async () => {
    const { onResolve } = renderPane(segments);
    fireEvent.click(await screen.findByRole("button", { name: "Edit whole result" }));

    const whole = screen.getByLabelText("Whole result");
    expect(whole).toHaveValue(
      "line one\n<<<<<<< ours\nmain two\n||||||| base\nline two\n=======\nfeature two\n>>>>>>> theirs\nline three\n",
    );
    const save = screen.getByRole("button", { name: "Save resolution" });
    expect(save).toBeDisabled();
    expect(screen.getByText(/Remove the conflict markers/)).toBeInTheDocument();

    fireEvent.change(whole, { target: { value: "line one!\nboth two\nline three\n" } });
    expect(save).toBeEnabled();
    fireEvent.click(save);
    expect(onResolve).toHaveBeenCalledWith("shared.txt", "line one!\nboth two\nline three\n");
  });

  it("starts the whole result from the blocks' choices", async () => {
    renderPane(segments);
    fireEvent.click(await screen.findByRole("button", { name: "Accept theirs" }));
    fireEvent.click(screen.getByRole("button", { name: "Edit whole result" }));
    expect(screen.getByLabelText("Whole result")).toHaveValue("line one\nfeature two\nline three\n");
    expect(screen.getByRole("button", { name: "Save resolution" })).toBeEnabled();
  });

  it("goes back to the blocks at once when the whole result is unchanged", async () => {
    renderPane(segments);
    fireEvent.click(await screen.findByRole("button", { name: "Edit whole result" }));
    fireEvent.click(screen.getByRole("button", { name: "Back to blocks" }));
    expect(screen.getByRole("region", { name: "Conflict 1 of 1" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("asks before dropping edits to the whole result on the way back", async () => {
    renderPane(segments);
    fireEvent.click(await screen.findByRole("button", { name: "Accept ours" }));
    fireEvent.click(screen.getByRole("button", { name: "Edit whole result" }));
    fireEvent.change(screen.getByLabelText("Whole result"), { target: { value: "changed\n" } });

    fireEvent.click(screen.getByRole("button", { name: "Back to blocks" }));
    const dialog = screen.getByRole("dialog", { name: "Drop the edits" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Keep editing" }));
    expect(screen.getByLabelText("Whole result")).toHaveValue("changed\n");

    fireEvent.click(screen.getByRole("button", { name: "Back to blocks" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Drop the edits" })).getByRole("button", { name: "Drop edits" }));
    const block = screen.getByRole("region", { name: "Conflict 1 of 1" });
    // The blocks keep their choices; only the whole-result edits are dropped.
    expect(within(block).getByRole("button", { name: "Accept ours" })).toHaveAttribute("aria-pressed", "true");
  });
});

// #763: the read-only parts are highlighted like the diff viewer; the results stay plain fields.
describe("ConflictResolutionPane syntax highlighting", () => {
  const tsSegments: ConflictSegment[] = [
    { kind: "Clean", content: "const one = 1;\n" },
    { kind: "Conflict", ours: "const ours = 2;\n", base: "const base = 2;\n", theirs: "const theirs = 2;\n" },
    { kind: "Clean", content: "const three = 3;\n" },
  ];
  const highlighted = (element: Element) => element.querySelector('[style*="--shiki-token"]') !== null;
  const withSettings = (syntaxHighlight: boolean, children: ReactNode) => (
    <DiffSettingsContext.Provider
      value={{
        settings: { ...DEFAULT_DIFF_SETTINGS, syntaxHighlight },
        update: () => {},
        error: null,
        dismissError: () => {},
      }}
    >
      {children}
    </DiffSettingsContext.Provider>
  );
  const pane = (path: string) => (
    <ConflictResolutionPane
      repoPath={TEST_REPO_PATH}
      client={fakeClient({ getConflictHunks: async () => tsSegments })}
      path={path}
      onResolve={vi.fn()}
      onResolveAddDelete={vi.fn()}
    />
  );

  it("highlights ours, theirs, base and the unchanged lines, keeping their text", async () => {
    const { container } = render(withSettings(true, pane("src/a.ts")));
    const block = await screen.findByRole("region", { name: "Conflict 1 of 1" });
    fireEvent.click(screen.getByRole("button", { name: "Show base" }));
    await waitFor(() => expect(highlighted(block)).toBe(true), { timeout: 15000 });
    const parts = [...container.querySelectorAll("pre")];
    expect(parts.map((part) => part.textContent)).toEqual([
      "const one = 1;\n",
      "const base = 2;\n",
      "const ours = 2;\n",
      "const theirs = 2;\n",
      "const three = 3;\n",
    ]);
    for (const part of parts) expect(highlighted(part)).toBe(true);
    // The editable result is a plain text field.
    fireEvent.click(within(block).getByRole("button", { name: "Accept ours" }));
    expect(within(block).getByLabelText("Result")).toHaveValue("const ours = 2;\n");
  }, 20000);

  it("leaves everything plain when syntax highlighting is off", async () => {
    const { container } = render(withSettings(false, pane("src/a.ts")));
    await screen.findByRole("region", { name: "Conflict 1 of 1" });
    // Give a highlighter that would run a chance to finish.
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(highlighted(container)).toBe(false);
    // Its changed words are marked (#879), which cuts the line: match the side as a whole.
    expect([...container.querySelectorAll("pre")].map((part) => part.textContent)).toContain("const ours = 2;\n");
  });

  it("leaves a file of an unknown language plain", async () => {
    const { container } = render(withSettings(true, pane("notes.unknownext")));
    await screen.findByRole("region", { name: "Conflict 1 of 1" });
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(highlighted(container)).toBe(false);
  });
});

// #764: keys for a block's choices and for moving between conflicts.
describe("ConflictResolutionPane keyboard shortcuts", () => {
  const twoConflicts: ConflictSegment[] = [
    { kind: "Conflict", ours: "a ours\n", base: "a base\n", theirs: "a theirs\n" },
    { kind: "Clean", content: "middle\n" },
    { kind: "Conflict", ours: "b ours\n", base: "b base\n", theirs: "b theirs\n" },
  ];
  const renderPane = (children?: (pane: ReactNode) => ReactNode) => {
    const pane = (
      <ConflictResolutionPane
        repoPath={TEST_REPO_PATH}
        client={fakeClient({ getConflictHunks: async () => twoConflicts })}
        path="shared.txt"
        onResolve={vi.fn()}
        onResolveAddDelete={vi.fn()}
      />
    );
    render(<>{children === undefined ? pane : children(pane)}</>);
  };

  it("takes a block's choices with 1 … 5 while the block has the focus", async () => {
    renderPane();
    const block = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const keys: [string, string, string][] = [
      ["1", "Accept ours", "a ours\n"],
      ["2", "Accept theirs", "a theirs\n"],
      ["3", "Ours then theirs", "a ours\na theirs\n"],
      ["4", "Theirs then ours", "a theirs\na ours\n"],
      ["5", "Accept base", "a base\n"],
    ];
    for (const [key, name, result] of keys) {
      const button = within(block).getByRole("button", { name });
      expect(button).toHaveAttribute("aria-keyshortcuts", key);
      button.focus();
      fireEvent.keyDown(button, { key, code: `Digit${key}` });
      expect(button).toHaveAttribute("aria-pressed", "true");
      expect(within(block).getByLabelText("Result")).toHaveValue(result);
    }
    // The other block keeps its own choice.
    expect(within(screen.getByRole("region", { name: "Conflict 2 of 2" })).getByLabelText("Result")).toHaveValue("");
  });

  it("leaves digits typed in the result field alone, and Esc goes back to the block", async () => {
    renderPane();
    const block = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const result = within(block).getByLabelText("Result");
    result.focus();
    fireEvent.keyDown(result, { key: "2", code: "Digit2" });
    expect(within(block).getByRole("button", { name: "Accept theirs" })).toHaveAttribute("aria-pressed", "false");
    fireEvent.keyDown(result, { key: "Escape" });
    expect(block).toHaveFocus();
    fireEvent.keyDown(block, { key: "2", code: "Digit2" });
    expect(within(block).getByRole("button", { name: "Accept theirs" })).toHaveAttribute("aria-pressed", "true");
  });

  it("ignores the digits with a modifier", async () => {
    renderPane();
    const block = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    block.focus();
    fireEvent.keyDown(block, { key: "1", code: "Digit1", ctrlKey: true });
    fireEvent.keyDown(block, { key: "1", code: "Digit1", altKey: true });
    expect(within(block).getByRole("button", { name: "Accept ours" })).toHaveAttribute("aria-pressed", "false");
  });

  it("moves between conflicts with F7 and Shift+F7, also from a result field", async () => {
    renderPane();
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });
    expect(screen.getByRole("button", { name: "Next conflict" })).toHaveAttribute("aria-keyshortcuts", "F7");
    expect(screen.getByRole("button", { name: "Previous conflict" })).toHaveAttribute("aria-keyshortcuts", "Shift+F7");
    const toolbarButton = screen.getByRole("button", { name: "Show base" });
    toolbarButton.focus();
    fireEvent.keyDown(toolbarButton, { key: "F7" });
    expect(first).toHaveFocus();
    fireEvent.keyDown(first, { key: "F7" });
    expect(second).toHaveFocus();
    // At the last conflict F7 stays put.
    fireEvent.keyDown(second, { key: "F7" });
    expect(second).toHaveFocus();
    const result = within(second).getByLabelText("Result");
    result.focus();
    fireEvent.keyDown(result, { key: "F7", shiftKey: true });
    expect(first).toHaveFocus();
  });

  it("registers with the tab's merge editor registry for the palette", async () => {
    let registry: MergeEditorRegistry | null = null;
    function Harness({ children }: { children: ReactNode }) {
      const value = useMergeEditorRegistry();
      registry = value;
      return <MergeEditorContext.Provider value={value}>{children}</MergeEditorContext.Provider>;
    }
    renderPane((pane) => <Harness>{pane}</Harness>);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    await waitFor(() => expect(registry?.current).toMatchObject({ path: "shared.txt", canGoPrevious: false, canGoNext: true }));

    // No conflict is current yet: a choice acts on the first one and focuses it.
    act(() => registry!.current!.choose("theirs"));
    expect(within(first).getByRole("button", { name: "Accept theirs" })).toHaveAttribute("aria-pressed", "true");
    expect(first).toHaveFocus();
    act(() => registry!.current!.next());
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });
    expect(second).toHaveFocus();
    await waitFor(() => expect(registry?.current).toMatchObject({ canGoPrevious: true, canGoNext: false }));
    act(() => registry!.current!.choose("base"));
    expect(within(second).getByLabelText("Result")).toHaveValue("b base\n");
    act(() => registry!.current!.previous());
    expect(first).toHaveFocus();
  });
});
