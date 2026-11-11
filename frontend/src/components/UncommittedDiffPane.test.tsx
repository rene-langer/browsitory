import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { DEFAULT_DIFF_SETTINGS, type DiffHunk, type HeadInfo, type RepoClient, type StatusEntry } from "../ipc/RepoClient";
import { DiffSettingsContext } from "../state/useDiffSettings";
import { OpenPreferencesContext } from "../state/openPreferences";
import { WorkspaceActionsContext, type WorkspaceActions } from "../state/useWorkspaceActions";
import { testWorkspaceActions } from "../test/workspaceActions";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { DiffPane, type DiscardRequest } from "./DiffPane";
import { NO_HEAD_INFO } from "./diffPaneShared";

const REPO = "/repo";

const hunk = (content: string): DiffHunk => ({
  oldStart: 1,
  oldLines: 1,
  newStart: 1,
  newLines: 1,
  lines: [{ origin: "Add", content }],
});

// Each file's diff says whose it is, so a test can tell which file the viewer shows.
const diffNamingFile = vi.fn(async (_repo: string, path: string, staged: boolean) => [
  hunk(`${staged ? "staged" : "unstaged"} ${path}`),
]);

function Pane({
  client,
  status,
  actions = {},
  expandUntracked = true,
  conflictRequest = false,
  onConflictRequestHandled,
  headInfo,
}: {
  client: RepoClient;
  status: StatusEntry[];
  actions?: Partial<WorkspaceActions>;
  expandUntracked?: boolean;
  conflictRequest?: boolean;
  onConflictRequestHandled?: () => void;
  headInfo?: HeadInfo;
}) {
  const [discardRequest, setDiscardRequest] = useState<DiscardRequest | null>(null);
  return (
    <DiffSettingsContext.Provider
      value={{ settings: { ...DEFAULT_DIFF_SETTINGS, expandUntracked }, update: () => {}, error: null, dismissError: () => {} }}
    >
      <WorkspaceActionsContext.Provider value={testWorkspaceActions(actions)}>
        <DiffPane
          repoPath={REPO}
          client={client}
          selectedRow="uncommitted"
          status={status}
          mergeMessage={null}
          rebaseProgress={null}
          discardRequest={discardRequest}
          onDiscardRequestChange={setDiscardRequest}
          conflictRequest={conflictRequest}
          onConflictRequestHandled={onConflictRequestHandled}
          headInfo={headInfo}
        />
      </WorkspaceActionsContext.Provider>
    </DiffSettingsContext.Provider>
  );
}

const entry = (path: string, staged: boolean, kind: StatusEntry["kind"] = "Modified"): StatusEntry => ({
  path,
  oldPath: null,
  staged,
  kind,
});

const status = [entry("a.txt", false), entry("b.txt", false), entry("c.txt", true, "New")];

const viewer = () => screen.getByRole("region", { name: /^Changes in / });

describe("working-tree view (#639)", () => {
  it("lists both groups compactly and shows only the current file's diff", async () => {
    const getWorkingDiff = vi.fn(diffNamingFile);
    render(<Pane client={fakeRepoClient({ getWorkingDiff }, "empty")} status={status} />);

    const unstaged = screen.getByRole("listbox", { name: "Unstaged changes" });
    const staged = screen.getByRole("listbox", { name: "Staged changes" });
    expect(within(unstaged).getAllByRole("option")).toHaveLength(2);
    expect(within(staged).getAllByRole("option")).toHaveLength(1);
    // A status letter, with the kind in the row's accessible name.
    expect(within(unstaged).getByRole("option", { name: /^a\.txt ?\(Modified\)/ })).toHaveTextContent("M");
    expect(within(staged).getByRole("option", { name: /^c\.txt ?\(New\)/ })).toHaveTextContent("A");

    // The first file is current; its diff is the only one fetched and shown.
    expect(await within(viewer()).findByText("unstaged a.txt")).toBeInTheDocument();
    expect(getWorkingDiff).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("unstaged b.txt")).not.toBeInTheDocument();
    expect(screen.getAllByRole("group", { name: "Diff view options" })).toHaveLength(1);
  });

  it("shows a clicked file's diff in the one viewer", async () => {
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} />);
    await screen.findByText("unstaged a.txt");

    fireEvent.click(screen.getByRole("option", { name: /c\.txt/ }));

    expect(await within(viewer()).findByText("staged c.txt")).toBeInTheDocument();
    expect(screen.queryByText("unstaged a.txt")).not.toBeInTheDocument();
    expect(viewer()).toHaveAccessibleName("Changes in c.txt (staged)");
  });

  it("keeps the Staged group on screen when it is empty", () => {
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={[entry("a.txt", false)]} />);
    expect(screen.getByText("Staged (0)")).toBeInTheDocument();
    expect(screen.getByText("Nothing staged yet.")).toBeInTheDocument();
  });

  it("moves to the file now at the same place when the current one is staged", async () => {
    const client = fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty");
    const { rerender } = render(<Pane client={client} status={status} />);
    await screen.findByText("unstaged a.txt");

    rerender(<Pane client={client} status={[entry("b.txt", false), entry("a.txt", true), entry("c.txt", true, "New")]} />);

    expect(await within(viewer()).findByText("unstaged b.txt")).toBeInTheDocument();
    // The diff's lines are options too (#668): look in the file list.
    const unstaged = screen.getByRole("listbox", { name: "Unstaged changes" });
    expect(within(unstaged).getByRole("option", { name: /b\.txt/ })).toHaveAttribute("aria-selected", "true");
  });

  it("moves to the other group when the current file's group empties", async () => {
    const client = fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty");
    const { rerender } = render(<Pane client={client} status={[entry("a.txt", false), entry("c.txt", true)]} />);
    await screen.findByText("unstaged a.txt");

    rerender(<Pane client={client} status={[entry("c.txt", true), entry("a.txt", true)]} />);

    expect(await within(viewer()).findByText("staged c.txt")).toBeInTheDocument();
  });

  it("Down on a group's last file moves to the other group's first, Up comes back", async () => {
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} />);
    await screen.findByText("unstaged a.txt");
    const unstaged = screen.getByRole("listbox", { name: "Unstaged changes" });

    fireEvent.keyDown(unstaged, { key: "ArrowDown" });
    expect(await within(viewer()).findByText("unstaged b.txt")).toBeInTheDocument();
    fireEvent.keyDown(unstaged, { key: "ArrowDown" });
    expect(await within(viewer()).findByText("staged c.txt")).toBeInTheDocument();
    const staged = screen.getByRole("listbox", { name: "Staged changes" });
    expect(staged).toHaveFocus();

    fireEvent.keyDown(staged, { key: "k" });
    expect(await within(viewer()).findByText("unstaged b.txt")).toBeInTheDocument();
    expect(unstaged).toHaveFocus();
  });

  it("Enter in the list moves focus into the diff; j and k there go to the next and previous file", async () => {
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} />);
    await screen.findByText("unstaged a.txt");

    fireEvent.keyDown(screen.getByRole("listbox", { name: "Unstaged changes" }), { key: "Enter" });
    const diff = () => within(viewer()).getByRole("group", { name: "Diff hunks" });
    await waitFor(() => expect(diff()).toHaveFocus());

    fireEvent.keyDown(diff(), { key: "j" });
    expect(await within(viewer()).findByText("unstaged b.txt")).toBeInTheDocument();
    await waitFor(() => expect(diff()).toHaveFocus());

    fireEvent.keyDown(diff(), { key: "j" });
    expect(await within(viewer()).findByText("staged c.txt")).toBeInTheDocument();
    fireEvent.keyDown(diff(), { key: "k" });
    expect(await within(viewer()).findByText("unstaged b.txt")).toBeInTheDocument();
  });

  it("stages a hunk of the shown file", async () => {
    const onStageHunk = vi.fn();
    render(
      <Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={status} actions={{ onStageHunk }} />,
    );
    fireEvent.click(screen.getByRole("option", { name: /b\.txt/ }));
    await within(viewer()).findByText("unstaged b.txt");

    fireEvent.click(within(viewer()).getByText("Stage hunk"));

    expect(onStageHunk).toHaveBeenCalledWith("b.txt", 1, 1, { contextLines: 3, algorithm: "Default" });
  });

  it("with Expand untracked files off (#623), loads an untracked file only on Show contents", async () => {
    const getWorkingDiff = vi.fn(diffNamingFile);
    render(
      <Pane
        client={fakeRepoClient({ getWorkingDiff }, "empty")}
        status={[entry("new.txt", false, "New"), entry("a.txt", false)]}
        expandUntracked={false}
      />,
    );

    fireEvent.click(await within(viewer()).findByRole("button", { name: "Show contents" }));

    expect(await within(viewer()).findByText("unstaged new.txt")).toBeInTheDocument();
    expect(getWorkingDiff).toHaveBeenCalledWith(REPO, "new.txt", false, expect.anything());
  });

  it("with Expand untracked files off, links to the Diff preferences (#784)", async () => {
    const openPreferences = vi.fn();
    render(
      <OpenPreferencesContext.Provider value={openPreferences}>
        <Pane client={fakeRepoClient({}, "empty")} status={[entry("new.txt", false, "New")]} expandUntracked={false} />
      </OpenPreferencesContext.Provider>,
    );
    fireEvent.click(await within(viewer()).findByRole("button", { name: "Diff preferences" }));
    expect(openPreferences).toHaveBeenCalledWith("diff");
  });

  it("offers no Diff preferences link where nothing opens Preferences", async () => {
    render(<Pane client={fakeRepoClient({}, "empty")} status={[entry("new.txt", false, "New")]} expandUntracked={false} />);
    await within(viewer()).findByRole("button", { name: "Show contents" });
    expect(within(viewer()).queryByRole("button", { name: "Diff preferences" })).toBeNull();
  });

  it("with Expand untracked files off, a tracked file still loads at once", async () => {
    const getWorkingDiff = vi.fn(diffNamingFile);
    render(
      <Pane
        client={fakeRepoClient({ getWorkingDiff }, "empty")}
        status={[entry("new.txt", false, "New"), entry("a.txt", false)]}
        expandUntracked={false}
      />,
    );
    await within(viewer()).findByRole("button", { name: "Show contents" });
    expect(getWorkingDiff).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("option", { name: /a\.txt/ }));
    expect(await within(viewer()).findByText("unstaged a.txt")).toBeInTheDocument();
  });
});

describe("empty states and conflicts (#665)", () => {
  it("says the working tree is clean when nothing changed", () => {
    render(<Pane client={fakeRepoClient({}, "empty")} status={[]} headInfo={{ ...NO_HEAD_INFO, headId: "abc" }} />);
    expect(screen.getByRole("status")).toHaveTextContent("Working tree clean");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("explains the first commit in a repository without commits", () => {
    render(<Pane client={fakeRepoClient({}, "empty")} status={[]} headInfo={NO_HEAD_INFO} />);
    expect(screen.getByRole("status")).toHaveTextContent("No commits yet");
    expect(screen.getByRole("status")).toHaveTextContent("commit to make the first commit");
  });

  it("lists conflicts first in Changes, in a group of their own", () => {
    const withConflicts = [entry("a.txt", false), entry("z.txt", false, "Conflicted"), entry("c.txt", true, "New")];
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={withConflicts} />);
    const unstaged = screen.getByRole("listbox", { name: "Unstaged changes" });
    const conflicts = within(unstaged).getByRole("group", { name: /^Conflicts \(1\)/ });
    expect(within(conflicts).getAllByRole("option")).toHaveLength(1);
    expect(within(conflicts).getByRole("option", { name: /^z\.txt/ })).toBeInTheDocument();
    const others = within(unstaged).getByRole("group", { name: "Other changes" });
    expect(within(others).getByRole("option", { name: /^a\.txt/ })).toBeInTheDocument();
    // Keyboard order follows the display: the conflict first.
    expect(within(unstaged).getAllByRole("option")[0]).toHaveAccessibleName(/^z\.txt/);
  });

  it("Go to conflict makes the first conflict the current file, then hands the request back", async () => {
    const onConflictRequestHandled = vi.fn();
    const withConflicts = [entry("a.txt", false), entry("z.txt", false, "Conflicted")];
    const client = fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty");
    const { rerender } = render(<Pane client={client} status={withConflicts} />);
    rerender(<Pane client={client} status={withConflicts} conflictRequest onConflictRequestHandled={onConflictRequestHandled} />);
    await waitFor(() => expect(onConflictRequestHandled).toHaveBeenCalledOnce());
    expect(screen.getByRole("option", { name: /^z\.txt/ })).toHaveAttribute("aria-selected", "true");
  });
});
