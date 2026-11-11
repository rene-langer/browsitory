import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
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

// #729: a flat | tree layout for the file list (remembered per machine) and a path filter. While
// the filter hides files, the bulk actions act on the shown files only and say so.
describe("file tree and path filter (#729)", () => {
  beforeEach(() => localStorage.clear());

  const nested = [
    entry("src/lib/a.ts", false),
    entry("src/lib/b.ts", false),
    entry("README.md", false),
    entry("src/lib/c.ts", true),
    entry("docs/guide.md", true),
  ];
  const unstagedList = () => screen.getByRole("listbox", { name: "Unstaged changes" });
  const stagedList = () => screen.getByRole("listbox", { name: "Staged changes" });
  const optionNames = (list: HTMLElement) =>
    within(list)
      .queryAllByRole("option")
      .map((option) => option.querySelector("[title]")?.getAttribute("title"));
  const filter = () => screen.getByRole("searchbox", { name: "Filter files" });

  it("is flat by default, with full paths", () => {
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={nested} />);
    expect(screen.getByRole("button", { name: "Flat" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Tree" })).toHaveAttribute("aria-pressed", "false");
    expect(within(unstagedList()).queryByRole("group", { name: /src\/lib/ })).toBeNull();
    expect(within(unstagedList()).getByRole("option", { name: /src\/lib\/a\.ts/ })).toHaveTextContent("src/lib/a.ts");
  });

  it("groups files by folder in the tree layout, folders first, and remembers the choice", () => {
    const client = fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty");
    const { unmount } = render(<Pane client={client} status={nested} />);
    fireEvent.click(screen.getByRole("button", { name: "Tree" }));

    expect(screen.getByRole("button", { name: "Tree" })).toHaveAttribute("aria-pressed", "true");
    // A chain of single-child folders shows on one row.
    const folder = within(unstagedList()).getByRole("group", { name: "src/lib" });
    expect(optionNames(folder)).toEqual([
      "src/lib/a.ts (Modified)",
      "src/lib/b.ts (Modified)",
    ]);
    // The row shows the file's name; its accessible name keeps the whole path.
    const row = within(folder).getByRole("option", { name: /src\/lib\/a\.ts/ });
    expect(within(row).getByText("a.ts")).toBeInTheDocument();
    expect(optionNames(unstagedList())).toEqual([
      "src/lib/a.ts (Modified)",
      "src/lib/b.ts (Modified)",
      "README.md (Modified)",
    ]);
    expect(optionNames(stagedList())).toEqual(["docs/guide.md (Modified)", "src/lib/c.ts (Modified)"]);
    expect(localStorage.getItem("working-tree-file-layout")).toBe("tree");

    unmount();
    render(<Pane client={client} status={nested} />);
    expect(within(unstagedList()).getByRole("group", { name: "src/lib" })).toBeInTheDocument();
  });

  it("collapses a folder, and the keys skip its files", async () => {
    localStorage.setItem("working-tree-file-layout", "tree");
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={nested} />);
    await screen.findByText("unstaged src/lib/a.ts");

    const toggle = within(unstagedList()).getByRole("button", { name: "src/lib" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(optionNames(unstagedList())).toEqual(["README.md (Modified)"]);
    // The current file was hidden: the shown one takes over.
    expect(await within(viewer()).findByText("unstaged README.md")).toBeInTheDocument();
    fireEvent.keyDown(unstagedList(), { key: "ArrowDown" });
    expect(await within(viewer()).findByText("staged docs/guide.md")).toBeInTheDocument();

    // Left and Right on a folder collapse and expand it.
    fireEvent.keyDown(toggle, { key: "ArrowRight" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    fireEvent.keyDown(toggle, { key: "ArrowLeft" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
  });

  it("stages and unstages a whole folder, leaving conflicts out", () => {
    localStorage.setItem("working-tree-file-layout", "tree");
    const onStageAllFiles = vi.fn();
    const onUnstageAllFiles = vi.fn();
    render(
      <Pane
        client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")}
        status={[...nested, entry("src/lib/conflict.ts", false, "Conflicted")]}
        actions={{ onStageAllFiles, onUnstageAllFiles }}
      />,
    );
    fireEvent.click(within(unstagedList()).getByRole("button", { name: "Stage folder src/lib" }));
    expect(onStageAllFiles).toHaveBeenCalledWith(["src/lib/a.ts", "src/lib/b.ts"]);
    fireEvent.click(within(stagedList()).getByRole("button", { name: "Unstage folder src/lib" }));
    expect(onUnstageAllFiles).toHaveBeenCalledWith(["src/lib/c.ts"]);
  });

  it("keeps conflicts in a flat group above the tree", () => {
    localStorage.setItem("working-tree-file-layout", "tree");
    render(
      <Pane
        client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")}
        status={[...nested, entry("src/x/conflict.ts", false, "Conflicted")]}
      />,
    );
    const conflicts = within(unstagedList()).getByRole("group", { name: /^Conflicts \(1\)/ });
    expect(within(conflicts).getByRole("option", { name: /src\/x\/conflict\.ts/ })).toHaveTextContent("src/x/conflict.ts");
    expect(within(conflicts).queryByRole("group")).toBeNull();
  });

  it("narrows both groups to the paths that match the filter", async () => {
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={nested} />);
    fireEvent.change(filter(), { target: { value: "LIB" } });

    expect(optionNames(unstagedList())).toEqual(["src/lib/a.ts (Modified)", "src/lib/b.ts (Modified)"]);
    expect(optionNames(stagedList())).toEqual(["src/lib/c.ts (Modified)"]);
    expect(screen.getByText("Changes (2 of 3)")).toBeInTheDocument();
    expect(screen.getByText("Staged (1 of 2)")).toBeInTheDocument();

    fireEvent.change(filter(), { target: { value: "nothing-like-this" } });
    expect(screen.getAllByText("No file matches the filter.")).toHaveLength(2);

    // Escape clears it.
    fireEvent.keyDown(filter(), { key: "Escape" });
    expect(filter()).toHaveValue("");
    expect(optionNames(unstagedList())).toHaveLength(3);
  });

  it("stages and unstages only the shown files while filtering, and says so", () => {
    const onStageAllFiles = vi.fn();
    const onUnstageAllFiles = vi.fn();
    render(
      <Pane
        client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")}
        status={[...nested, entry("src/lib/new.ts", false, "New")]}
        actions={{ onStageAllFiles, onUnstageAllFiles }}
      />,
    );
    fireEvent.change(filter(), { target: { value: "lib" } });

    expect(screen.queryByRole("button", { name: "Stage all" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Stage 3 shown" }));
    expect(onStageAllFiles).toHaveBeenCalledWith(["src/lib/a.ts", "src/lib/b.ts", "src/lib/new.ts"]);
    fireEvent.click(screen.getByRole("button", { name: "Stage 2 modified shown" }));
    expect(onStageAllFiles).toHaveBeenLastCalledWith(["src/lib/a.ts", "src/lib/b.ts"]);
    fireEvent.click(screen.getByRole("button", { name: "Unstage 1 shown" }));
    expect(onUnstageAllFiles).toHaveBeenCalledWith(["src/lib/c.ts"]);
  });

  it("discards only the shown files while filtering, listing them in the confirmation", () => {
    const onDiscardAllUnstaged = vi.fn();
    render(
      <Pane
        client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")}
        status={nested}
        actions={{ onDiscardAllUnstaged }}
      />,
    );
    fireEvent.change(filter(), { target: { value: "lib" } });
    fireEvent.click(screen.getByRole("button", { name: "Discard 2 shown…" }));

    const dialog = screen.getByRole("dialog", { name: "Discard shown changes" });
    const listed = within(within(dialog).getByRole("list", { name: "Files to discard" })).getAllByRole("listitem");
    expect(listed.map((item) => item.textContent)).toEqual(["src/lib/a.ts", "src/lib/b.ts"]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Discard 2 files" }));
    expect(onDiscardAllUnstaged).toHaveBeenCalledWith(["src/lib/a.ts", "src/lib/b.ts"]);
  });

  it("deletes only the shown untracked files while filtering", () => {
    const onDeleteUntracked = vi.fn();
    render(
      <Pane
        client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")}
        status={[...nested, entry("src/lib/new.ts", false, "New"), entry("notes.txt", false, "New")]}
        actions={{ onDeleteUntracked }}
      />,
    );
    fireEvent.change(filter(), { target: { value: "lib" } });
    fireEvent.click(screen.getByRole("button", { name: "Delete 1 untracked shown…" }));

    const dialog = screen.getByRole("dialog", { name: "Delete untracked files" });
    const listed = within(within(dialog).getByRole("list", { name: "Untracked files to delete" })).getAllByRole("listitem");
    expect(listed.map((item) => item.textContent)).toEqual(["src/lib/new.ts"]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete 1 file" }));
    expect(onDeleteUntracked).toHaveBeenCalledWith(["src/lib/new.ts"]);
  });

  it("Ctrl+A selects the shown files only, and a filter change drops hidden files from the selection", async () => {
    render(<Pane client={fakeRepoClient({ getWorkingDiff: diffNamingFile }, "empty")} status={nested} />);
    await screen.findByText("unstaged src/lib/a.ts");

    fireEvent.keyDown(unstagedList(), { key: "a", ctrlKey: true });
    const bar = () => screen.getByRole("group", { name: "Selected unstaged files" });
    expect(within(bar()).getByText("3 selected")).toBeInTheDocument();

    fireEvent.change(filter(), { target: { value: "lib" } });
    expect(within(bar()).getByText("2 selected")).toBeInTheDocument();
    fireEvent.change(filter(), { target: { value: "" } });
    // README.md left the selection when the filter hid it.
    expect(within(bar()).getByText("2 selected")).toBeInTheDocument();

    fireEvent.change(filter(), { target: { value: "a.ts" } });
    expect(screen.queryByRole("group", { name: "Selected unstaged files" })).toBeNull();
    fireEvent.change(filter(), { target: { value: "" } });
    fireEvent.keyDown(unstagedList(), { key: "a", ctrlKey: true });
    fireEvent.change(filter(), { target: { value: ".ts" } });
    fireEvent.keyDown(unstagedList(), { key: "a", ctrlKey: true });
    expect(within(bar()).getByText("2 selected")).toBeInTheDocument();
  });
});
