import { useState } from "react";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import type { CommitDropRun } from "../lib/actions/commitActions";
import type { SelectedRow } from "../state/useAppState";
import { CommitDragContext, useCommitDragSession } from "../state/useCommitDrag";
import { treeProps } from "../test/branchTreeProps";
import { BranchTreeWithDialogs } from "../test/BranchTreeWithDialogs";
import { CommitGraph } from "./CommitGraph";

// #744: commits dragged from the graph onto a branch in the sidebar cherry-pick them onto the
// current branch. The graph and the tree share the drag through `CommitDragContext`, as in
// `RepoWorkspace`. jsdom has no layout, so each drag says which element is under the pointer.

const commit = (id: string, summary: string, parentIds: string[]): GraphCommit => ({
  id,
  shortId: id.slice(0, 7),
  summary,
  authorName: "A",
  authorEmail: "a@x",
  timestamp: 0,
  parentIds,
  branchRefs: [],
  remoteBranchRefs: [],
});

// Newest first, as the graph lists them.
const commits = [
  commit("mmmmmmm4", "merge commit", ["ccccccc3", "zzzzzzz9"]),
  commit("ccccccc3", "third commit", ["bbbbbbb2"]),
  commit("bbbbbbb2", "second commit", ["aaaaaaa1"]),
  commit("aaaaaaa1", "first commit", []),
];

const branches = [
  { name: "main", isCurrent: true },
  { name: "feat/foo", isCurrent: false },
  { name: "other", isCurrent: false },
];

const head = (headId: string | null) => ({
  headId,
  detachedAt: null,
  headPushed: false,
  operation: null,
  operationLeftover: false,
  squashMessage: null,
  commitHooks: [] as string[],
  sequence: null,
  rebase: null,
  bisect: null,
  terminalSequence: null,
  terminalRebase: null,
});

function Host({
  onRun,
  onSelectRow,
  busy,
  headId,
  treeBranches,
}: {
  onRun: (run: CommitDropRun) => void;
  onSelectRow: (row: SelectedRow) => void;
  busy: string | null;
  headId: string | null;
  treeBranches: typeof branches;
}) {
  const session = useCommitDragSession(onRun);
  const [selectedRow, setSelectedRow] = useState<SelectedRow>("uncommitted");
  const currentBranch = treeBranches.find((branch) => branch.isCurrent)?.name ?? null;
  return (
    <CommitDragContext.Provider value={session}>
      <CommitGraph
        status={[]}
        commits={commits}
        selectedRow={selectedRow}
        operationsBlocked={busy !== null}
        operationsBlockedReason={busy}
        onSelectRow={(row) => {
          setSelectedRow(row);
          onSelectRow(row);
        }}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        headInfo={head(headId)}
        currentBranch={currentBranch}
        onCherryPick={vi.fn()}
        onRevert={vi.fn()}
        onCherryPickMany={vi.fn()}
        onRevertMany={vi.fn()}
        onCheckoutCommit={vi.fn()}
        onChooseParent={vi.fn()}
        onRequestReset={vi.fn()}
        onEditHeadMessage={vi.fn()}
      />
      <BranchTreeWithDialogs
        {...treeProps({ branches: treeBranches, operationDisabled: busy !== null, operationDisabledReason: busy })}
      />
    </CommitDragContext.Provider>
  );
}

function renderBoth({
  busy = null,
  headId = "elsewhere",
  treeBranches = branches,
}: { busy?: string | null; headId?: string | null; treeBranches?: typeof branches } = {}) {
  const onRun = vi.fn();
  const onSelectRow = vi.fn();
  render(<Host onRun={onRun} onSelectRow={onSelectRow} busy={busy} headId={headId} treeBranches={treeBranches} />);
  return { onRun, onSelectRow };
}

const graphRow = (summary: string) => screen.getByText(new RegExp(`${summary}$`)).closest("li") as HTMLElement;
const mainRow = () => screen.getByText(/main.*\(current\)/).closest("li") as HTMLElement;
const otherRow = () => screen.getByRole("button", { name: "other" });
const status = () => within(screen.getByRole("region", { name: "Branches" })).getByRole("status");
const menuNames = () => screen.getAllByRole("menuitem").map((item) => item.textContent);

describe("dragging commits onto a branch (#744)", () => {
  const originalElementFromPoint = document.elementFromPoint;
  beforeEach(() => {
    localStorage.clear();
  });
  afterEach(() => {
    document.elementFromPoint = originalElementFromPoint;
    vi.useRealTimers();
  });

  function pressAndMove(source: HTMLElement, over: Element) {
    document.elementFromPoint = vi.fn(() => over);
    fireEvent.pointerDown(source, { clientX: 5, clientY: 5, button: 0 });
    fireEvent.pointerMove(window, { clientX: 5, clientY: 40 });
  }

  function drag(source: HTMLElement, target: HTMLElement) {
    pressAndMove(source, target);
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });
  }

  it("a commit dropped on the current branch offers Cherry-pick and Cancel, and runs the pick", () => {
    const { onRun } = renderBoth();
    pressAndMove(graphRow("third commit"), mainRow());
    expect(mainRow()).toHaveAttribute("data-drop-position", "on");
    expect(graphRow("third commit")).toHaveAttribute("data-dragging", "true");
    expect(status()).toHaveTextContent("Dragging commit ccccccc. Drop it on main to cherry-pick; Escape cancels.");
    // The label beside the pointer says what the drop does.
    expect(screen.getByText("Cherry-pick ccccccc onto main")).toBeInTheDocument();
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });

    expect(menuNames()).toEqual(["Cherry-pick ccccccc onto main", "Cancel"]);
    expect(status()).toHaveTextContent("Dropped commit ccccccc on main: Cherry-pick ccccccc onto main");
    fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick ccccccc onto main" }));
    expect(onRun).toHaveBeenCalledWith({ kind: "pick", commitId: "ccccccc3" });
    expect(mainRow()).not.toHaveAttribute("data-drop-position");
  });

  it("Cancel in the drop menu runs nothing", () => {
    const { onRun } = renderBoth();
    drag(graphRow("third commit"), mainRow());
    fireEvent.click(screen.getByRole("menuitem", { name: "Cancel" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(onRun).not.toHaveBeenCalled();
  });

  it("a merge commit's drop opens the parent picker", () => {
    const { onRun } = renderBoth();
    drag(graphRow("merge commit"), mainRow());
    fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick mmmmmmm onto main…" }));
    expect(onRun).toHaveBeenCalledWith({ kind: "choose-parent", commitId: "mmmmmmm4" });
  });

  it("dragging a row of a multi-selection drags every selected commit, picked oldest first", () => {
    const { onRun } = renderBoth();
    fireEvent.click(graphRow("third commit"));
    fireEvent.click(graphRow("first commit"), { ctrlKey: true });
    pressAndMove(graphRow("third commit"), mainRow());
    expect(graphRow("third commit")).toHaveAttribute("data-dragging", "true");
    expect(graphRow("first commit")).toHaveAttribute("data-dragging", "true");
    expect(graphRow("second commit")).not.toHaveAttribute("data-dragging");
    expect(status()).toHaveTextContent("Dragging 2 commits. Drop them on main to cherry-pick; Escape cancels.");
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });
    fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick 2 commits onto main" }));
    expect(onRun).toHaveBeenCalledWith({ kind: "pick-many", commitIds: ["aaaaaaa1", "ccccccc3"] });
  });

  it("dragging a row outside the multi-selection drags that commit only", () => {
    const { onRun } = renderBoth();
    fireEvent.click(graphRow("third commit"));
    fireEvent.click(graphRow("first commit"), { ctrlKey: true });
    drag(graphRow("second commit"), mainRow());
    fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick bbbbbbb onto main" }));
    expect(onRun).toHaveBeenCalledWith({ kind: "pick", commitId: "bbbbbbb2" });
  });

  it("shows why a branch that isn't current refuses the drop, and opens no menu", () => {
    const { onRun } = renderBoth();
    pressAndMove(graphRow("third commit"), otherRow());
    expect(otherRow()).toHaveAttribute("data-drop-position", "refused");
    const reason = "Only the current branch, main, can take a cherry-pick: drop on main.";
    expect(screen.getByText(reason)).toBeInTheDocument();
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(status()).toHaveTextContent(`Can't drop here. ${reason}`);
    expect(onRun).not.toHaveBeenCalled();
  });

  it("refuses the drop with the busy reason while another operation runs", () => {
    renderBoth({ busy: "A rebase is in progress." });
    pressAndMove(graphRow("third commit"), mainRow());
    expect(mainRow()).toHaveAttribute("data-drop-position", "refused");
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(status()).toHaveTextContent("Can't drop here. A rebase is in progress.");
  });

  it("refuses HEAD itself, as the commit menu's Cherry-pick does", () => {
    renderBoth({ headId: "ccccccc3" });
    drag(graphRow("third commit"), mainRow());
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(status()).toHaveTextContent("Can't drop here. HEAD already contains this commit.");
  });

  it("on a detached HEAD says to check out a branch first", () => {
    renderBoth({ treeBranches: branches.map((branch) => ({ ...branch, isCurrent: false })) });
    pressAndMove(graphRow("third commit"), otherRow());
    expect(status()).toHaveTextContent(
      "Dragging commit ccccccc. Check out a branch first: a cherry-pick applies to the current branch.",
    );
    expect(otherRow()).toHaveAttribute("data-drop-position", "refused");
    fireEvent.keyDown(window, { key: "Escape" });
  });

  it("Escape cancels the drag, and the dropped row isn't also clicked", () => {
    const { onRun, onSelectRow } = renderBoth();
    pressAndMove(graphRow("third commit"), mainRow());
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(status()).toHaveTextContent("Drag cancelled.");
    expect(mainRow()).not.toHaveAttribute("data-drop-position");

    drag(graphRow("third commit"), mainRow());
    fireEvent.click(graphRow("third commit"));
    expect(onSelectRow).not.toHaveBeenCalled();
    expect(onRun).not.toHaveBeenCalled();
  });

  it("opens a collapsed folder hovered for a moment during a commit drag", () => {
    renderBoth();
    const folder = screen.getByRole("button", { name: "feat" });
    fireEvent.click(folder);
    expect(screen.queryByRole("button", { name: "foo" })).not.toBeInTheDocument();

    vi.useFakeTimers();
    pressAndMove(graphRow("third commit"), folder);
    act(() => vi.advanceTimersByTime(700));
    expect(screen.getByRole("button", { name: "foo" })).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
  });
});
