import { useState, type ComponentProps } from "react";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { GraphCommit, StatusEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { assignLanes } from "../lib/commitGraphLayout";
import { type MergeExpansion, withExpandedMerges } from "../lib/mergeExpansion";
import type { VisitKind } from "../lib/navigationHistory";
import { CommitGraph } from "./CommitGraph";
import styles from "./CommitGraph.module.css";
import { describedByText } from "../test/describedBy";

const status: StatusEntry[] = [
  { path: "src/main.rs", oldPath: null, staged: false, kind: "Modified" },
  { path: "README.md", oldPath: null, staged: true, kind: "New" },
];

const commits: GraphCommit[] = [
  {
    id: "aaa111...",
    shortId: "aaa1111",
    summary: "second commit",
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 2,
    parentIds: [],
    branchRefs: [],
    remoteBranchRefs: [],
  },
  {
    id: "bbb222...",
    shortId: "bbb2222",
    summary: "first commit",
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 1,
    parentIds: [],
    branchRefs: [],
    remoteBranchRefs: [],
  },
];

const NO_HEAD = {
  headId: null,
  detachedAt: null,
  headPushed: false,
  operation: null,
  operationLeftover: false,
  squashMessage: null,
  commitHooks: [] as string[], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null,
} as const;
const historyProps = {
  headInfo: NO_HEAD,
  currentBranch: "main",
  onCherryPick: vi.fn(),
  onRevert: vi.fn(),
  onCheckoutCommit: vi.fn(),
  onChooseParent: vi.fn(),
  onRequestReset: vi.fn(),
  onEditHeadMessage: vi.fn(),
};

describe("CommitGraph", () => {
  it("scrolls the selected row into view again when a Go to asks for it (#441)", () => {
    const scrollIntoView = vi.fn();
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = scrollIntoView;
    try {
      const props = {
        ...historyProps,
        status,
        commits,
        selectedRow: { commitId: "bbb222..." } as SelectedRow,
        operationsBlocked: false,
        onSelectRow: vi.fn(),
        onBranchFromCommit: vi.fn(),
        onRebaseFromCommit: vi.fn(),
      };
      const { rerender } = render(<CommitGraph {...props} revealRequest={1} />);
      const before = scrollIntoView.mock.calls.length;
      rerender(<CommitGraph {...props} revealRequest={1} />);
      expect(scrollIntoView.mock.calls.length).toBe(before);

      rerender(<CommitGraph {...props} revealRequest={2} />);
      expect(scrollIntoView.mock.calls.length).toBe(before + 1);
      expect(scrollIntoView.mock.contexts.at(-1)).toHaveTextContent("first commit");
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });

  it("exposes the row list as a labeled listbox, matching ListRow's option rows", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByRole("listbox", { name: "Commit history" })).toBeInTheDocument();
  });

  it("renders the Uncommitted changes row with a change-count badge", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByText("Uncommitted changes (2)")).toBeInTheDocument();
  });

  it("renders each commit's short id and summary", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByText(/aaa1111/)).toBeInTheDocument();
    expect(screen.getByText(/second commit/)).toBeInTheDocument();
    expect(screen.getByText(/bbb2222/)).toBeInTheDocument();
    expect(screen.getByText(/first commit/)).toBeInTheDocument();
  });

  it("clicking a commit row calls onSelectRow with that commit's id", () => {
    const onSelectRow = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText(/second commit/).closest("li")!);

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "aaa111..." });
  });

  it("ArrowDown moves from Uncommitted changes to the first commit", () => {
    const onSelectRow = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "ArrowDown" });

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "aaa111..." }, "step");
  });

  it("ArrowUp from the first row does nothing (clamped, not wrapped)", () => {
    const onSelectRow = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "ArrowUp" });

    expect(onSelectRow).toHaveBeenCalledWith("uncommitted", "step");
  });

  it("ArrowDown from the last commit does nothing (clamped)", () => {
    const onSelectRow = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow={{ commitId: "bbb222..." }}
        operationsBlocked={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "ArrowDown" });

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "bbb222..." }, "step");
  });

  it("right-clicking a commit row shows a 'Branch from here' menu entry", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);

    expect(screen.getByText("Branch from here")).toBeInTheDocument();
  });

  it("renders the menu next to the commit list, not inside its listbox (#669)", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);

    const list = screen.getByRole("listbox", { name: "Commit history" });
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(within(list).queryByRole("menu")).toBeNull();
    expect(list.querySelector('[role="menu"]')).toBeNull();
  });

  it("clicking 'Branch from here' calls onBranchFromCommit with that commit's id and closes the menu", () => {
    const onBranchFromCommit = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={onBranchFromCommit}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);
    fireEvent.click(screen.getByText("Branch from here"));

    expect(onBranchFromCommit).toHaveBeenCalledWith("aaa111...");
    expect(screen.queryByText("Branch from here")).not.toBeInTheDocument();
  });

  it("right-clicking the Uncommitted changes row does not show the menu", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/Uncommitted changes/).closest("li")!);

    expect(screen.queryByText("Branch from here")).not.toBeInTheDocument();
  });

  it("renders a branch badge for a commit that is a branch tip", () => {
    const commitsWithBranch: GraphCommit[] = [
      { ...commits[0], branchRefs: ["main"], remoteBranchRefs: [] },
      commits[1],
    ];
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commitsWithBranch}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByText("main")).toBeInTheDocument();
  });

  it("marks the commit of a detached HEAD with a HEAD badge (#467)", () => {
    render(
      <CommitGraph
        {...historyProps}
        headInfo={{ ...NO_HEAD, headId: commits[0].id, detachedAt: commits[0].shortId }}
        currentBranch={null}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const rows = document.querySelectorAll("li.commit-row");
    const badge = within(rows[0] as HTMLElement).getByText("HEAD");
    expect(badge).toHaveClass(styles.headBadge);
    expect(badge).toHaveAttribute("title", "Detached HEAD");
    expect(within(rows[1] as HTMLElement).queryByText("HEAD")).not.toBeInTheDocument();
  });

  it("shows no HEAD badge while HEAD is on a branch (the branch badge marks it)", () => {
    render(
      <CommitGraph
        {...historyProps}
        headInfo={{ ...NO_HEAD, headId: commits[0].id }}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.queryByText("HEAD")).not.toBeInTheDocument();
  });

  it("renders a remote branch badge for a commit that is a remote-tracking tip", () => {
    const commitsWithRemoteBranch: GraphCommit[] = [
      { ...commits[0], remoteBranchRefs: ["origin/main"] },
      commits[1],
    ];
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commitsWithRemoteBranch}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByText("origin/main")).toBeInTheDocument();
  });

  it("hides a remote branch badge not in graphRemoteBranchSelection", () => {
    const commitsWithRemoteBranch: GraphCommit[] = [
      { ...commits[0], remoteBranchRefs: ["origin/main"] },
      commits[1],
    ];
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commitsWithRemoteBranch}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        graphRemoteBranchSelection={["origin/other"]}
      />,
    );

    expect(screen.queryByText("origin/main")).not.toBeInTheDocument();
  });

  it("hides every remote branch badge when graphRemoteBranchSelection is an explicit empty array", () => {
    const commitsWithRemoteBranch: GraphCommit[] = [
      { ...commits[0], remoteBranchRefs: ["origin/main"] },
      commits[1],
    ];
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commitsWithRemoteBranch}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        graphRemoteBranchSelection={[]}
      />,
    );

    expect(screen.queryByText("origin/main")).not.toBeInTheDocument();
  });

  it("still shows a remote branch badge whose ref is included in graphRemoteBranchSelection", () => {
    const commitsWithRemoteBranch: GraphCommit[] = [
      { ...commits[0], remoteBranchRefs: ["origin/main"] },
      commits[1],
    ];
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commitsWithRemoteBranch}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        graphRemoteBranchSelection={["origin/main"]}
      />,
    );

    expect(screen.getByText("origin/main")).toBeInTheDocument();
  });

  it("renders both a local and a remote badge for a commit that is both tips, each with its own CSS class", () => {
    const commitsWithBothBadges: GraphCommit[] = [
      { ...commits[0], branchRefs: ["main"], remoteBranchRefs: ["origin/main"] },
      commits[1],
    ];
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commitsWithBothBadges}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const localBadge = screen.getByText("main");
    const remoteBadge = screen.getByText("origin/main");
    expect(localBadge).toHaveClass(styles.branchBadge);
    expect(remoteBadge).toHaveClass(styles.remoteBranchBadge);
  });

  it("renders a lane graphic for every commit row", () => {
    const { container } = render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(container.querySelectorAll("li.commit-row svg").length).toBe(commits.length);
  });

  it("still renders each commit's short id and summary as plain text in its own li (hard E2E compatibility constraint)", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const row = screen.getByText(/second commit/).closest("li");
    expect(row).not.toBeNull();
    expect(row?.tagName).toBe("LI");
    expect(row?.textContent).toContain("aaa1111 second commit");
  });

  it("still sets aria-selected on the selected commit's li (hard E2E compatibility constraint)", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow={{ commitId: "aaa111..." }}
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const row = screen.getByText(/second commit/).closest("li");
    expect(row).not.toBeNull();
    expect(row?.getAttribute("aria-selected")).toBe("true");
  });

  it("right-clicking a commit and choosing Rebase onto here calls onRebaseFromCommit", () => {
    const onRebaseFromCommit = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={onRebaseFromCommit}
      />,
    );

    const row = screen.getByText(/second commit/).closest("li");
    fireEvent.contextMenu(row!);
    fireEvent.click(screen.getByText("Rebase onto here"));

    expect(onRebaseFromCommit).toHaveBeenCalledWith("aaa111...");
  });

  it("dims other lanes' lines while hovering a row, and undims on mouse leave", () => {
    // A fork: F1 and M2 are both children of M1, so F1 sits on lane 0 and M2 opens lane 1.
    const forkCommits: GraphCommit[] = [
      {
        id: "F1",
        shortId: "F1",
        summary: "F1",
        authorName: "Rene",
        authorEmail: "rene@example.com",
        timestamp: 3,
        parentIds: ["M1"],
        branchRefs: [],
        remoteBranchRefs: [],
      },
      {
        id: "M2",
        shortId: "M2",
        summary: "M2",
        authorName: "Rene",
        authorEmail: "rene@example.com",
        timestamp: 2,
        parentIds: ["M1"],
        branchRefs: [],
        remoteBranchRefs: [],
      },
      {
        id: "M1",
        shortId: "M1",
        summary: "M1",
        authorName: "Rene",
        authorEmail: "rene@example.com",
        timestamp: 1,
        parentIds: [],
        branchRefs: [],
        remoteBranchRefs: [],
      },
    ];

    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={forkCommits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const m2Row = screen.getByText(/M2/).closest("li")!;
    fireEvent.mouseEnter(m2Row);

    const f1Row = screen.getByText(/F1/).closest("li")!;
    const f1Circle = f1Row.querySelector("circle")!;
    const m2Circle = m2Row.querySelector("circle")!;
    expect(f1Circle.getAttribute("opacity")).toBe("0.25");
    expect(m2Circle.getAttribute("opacity")).toBe("1");

    fireEvent.mouseLeave(m2Row);
    expect(f1Circle.getAttribute("opacity")).toBe("1");
  });

  const chainCommits: GraphCommit[] = [
    { ...commits[0], id: "D", shortId: "D", summary: "D", parentIds: ["C"] },
    { ...commits[0], id: "C", shortId: "C", summary: "C", parentIds: ["B"] },
    { ...commits[0], id: "B", shortId: "B", summary: "B", parentIds: ["A"] },
    { ...commits[0], id: "A", shortId: "A", summary: "A", parentIds: ["R"] },
  ];

  // `selectedRow` is controlled by the app, and the graph only honors a multi-selection while the
  // focused commit belongs to it, so these tests need a stateful host rather than a static prop.
  function Host(props: ComponentProps<typeof CommitGraph>) {
    const [selectedRow, setSelectedRow] = useState<SelectedRow>(props.selectedRow);
    return (
      <CommitGraph
        {...historyProps}
        {...props}
        selectedRow={selectedRow}
        onSelectRow={(next) => {
          setSelectedRow(next);
          props.onSelectRow(next);
        }}
      />
    );
  }

  function renderChain(overrides: Partial<ComponentProps<typeof CommitGraph>> = {}) {
    const onRebaseSelection = vi.fn();
    render(
      <Host
        {...historyProps}
        status={status}
        commits={chainCommits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        onRebaseSelection={onRebaseSelection}
        {...overrides}
      />,
    );
    return { onRebaseSelection };
  }

  const row = (name: string) => screen.getByText(new RegExp(`^${name} `)).closest("li")!;

  it("offers Cherry-pick and Revert N commits for a multi-selection, oldest first", () => {
    const onCherryPickMany = vi.fn();
    const onRevertMany = vi.fn();
    renderChain({ onCherryPickMany, onRevertMany });

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });
    fireEvent.click(row("A"), { ctrlKey: true });
    const toolbar = screen.getByRole("toolbar", { name: "Selected commits" });
    fireEvent.click(within(toolbar).getByRole("button", { name: "Cherry-pick 3 commits" }));
    expect(onCherryPickMany).toHaveBeenCalledWith(["A", "B", "D"]);
    fireEvent.click(within(toolbar).getByRole("button", { name: "Revert 3 commits" }));
    expect(onRevertMany).toHaveBeenCalledWith(["A", "B", "D"]);

    fireEvent.contextMenu(row("B"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick 3 commits" }));
    expect(onCherryPickMany).toHaveBeenCalledTimes(2);
    // After the rebase items, which keep their order; the destructive Drop goes last (#645).
    const labels = within(toolbar)
      .getAllByRole("button")
      .map((button) => button.textContent);
    expect(labels.slice(-3)).toEqual(["Cherry-pick 3 commits", "Revert 3 commits", "Drop 3 commits"]);
  });

  it("keeps multi-commit pick and revert next to Compare these commits", () => {
    const onCherryPickMany = vi.fn();
    renderChain({ onCherryPickMany, onRevertMany: vi.fn(), onCompareCommits: vi.fn() });
    fireEvent.click(row("C"));
    fireEvent.click(row("A"), { ctrlKey: true });
    const toolbar = screen.getByRole("toolbar", { name: "Selected commits" });
    expect(within(toolbar).getByRole("button", { name: "Compare these commits" })).toBeEnabled();
    fireEvent.click(within(toolbar).getByRole("button", { name: "Cherry-pick 2 commits" }));
    expect(onCherryPickMany).toHaveBeenCalledWith(["A", "C"]);
  });

  it("disables multi-commit pick and revert while pending or with a merge commit selected", () => {
    const withMerge: GraphCommit[] = [
      { ...commits[0], id: "M", shortId: "M", summary: "M", parentIds: ["B", "X"] },
      ...chainCommits.slice(2),
    ];
    renderChain({ commits: withMerge, onCherryPickMany: vi.fn(), onRevertMany: vi.fn() });
    fireEvent.click(row("M"));
    fireEvent.click(row("B"), { ctrlKey: true });
    const toolbar = screen.getByRole("toolbar", { name: "Selected commits" });
    const pick = within(toolbar).getByRole("button", { name: "Cherry-pick 2 commits" });
    expect(pick).toBeDisabled();
    expect(pick).toHaveAttribute("title", "Cherry-pick merge commits one at a time.");
    expect(within(toolbar).getByRole("button", { name: "Revert 2 commits" })).toHaveAttribute(
      "title",
      "Revert merge commits one at a time.",
    );
  });

  it("disables multi-commit pick and revert while an operation is pending", () => {
    renderChain({ operationsBlocked: true, onCherryPickMany: vi.fn(), onRevertMany: vi.fn() });
    fireEvent.click(row("D"));
    fireEvent.click(row("C"), { ctrlKey: true });
    expect(screen.getByRole("button", { name: "Cherry-pick 2 commits" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Revert 2 commits" })).toBeDisabled();
  });

  it("offers Compare these commits only with exactly two selected", () => {
    const onCompareCommits = vi.fn();
    renderChain({ onCompareCommits });

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });
    expect(screen.getByRole("button", { name: "Compare these commits" })).toBeEnabled();

    fireEvent.click(row("C"), { ctrlKey: true });
    expect(screen.queryByRole("button", { name: "Compare these commits" })).toBeNull();
  });

  it("orders the two commits older..newer whatever the click order", () => {
    const onCompareCommits = vi.fn();
    renderChain({ onCompareCommits });

    fireEvent.click(row("B"));
    fireEvent.click(row("D"), { ctrlKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Compare these commits" }));
    expect(onCompareCommits).toHaveBeenLastCalledWith("B", "D");

    fireEvent.contextMenu(row("D"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Compare these commits" }));
    expect(onCompareCommits).toHaveBeenLastCalledWith("B", "D");
  });

  it("keeps Compare these commits enabled while an operation is pending", () => {
    renderChain({ operationsBlocked: true, onCompareCommits: vi.fn() });
    fireEvent.click(row("C"));
    fireEvent.click(row("B"), { shiftKey: true });
    expect(screen.getByRole("button", { name: "Compare these commits" })).toBeEnabled();
  });

  it("saves one commit, or the selected ones oldest first, as a patch even while busy (#448)", () => {
    const onSavePatch = vi.fn();
    renderChain({ operationsBlocked: true, onSavePatch });
    fireEvent.contextMenu(row("C"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Save as patch…" }));
    expect(onSavePatch).toHaveBeenLastCalledWith(["C"]);

    fireEvent.click(row("B"));
    fireEvent.click(row("D"), { ctrlKey: true });
    const save = screen.getByRole("button", { name: "Save 2 commits as patch…" });
    expect(save).toBeEnabled();
    fireEvent.click(save);
    expect(onSavePatch).toHaveBeenLastCalledWith(["B", "D"]);
  });

  it("offers Diff against working tree on a single commit", () => {
    const onCompareWithWorkingTree = vi.fn();
    renderChain({ operationsBlocked: true, onCompareWithWorkingTree });
    fireEvent.contextMenu(row("C"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Diff against working tree" }));
    expect(onCompareWithWorkingTree).toHaveBeenCalledWith("C");
  });

  it("shift-clicking a second commit and right-clicking within the range shows the multi-select menu", () => {
    renderChain();

    fireEvent.click(row("C"));
    fireEvent.click(row("B"), { shiftKey: true });
    fireEvent.contextMenu(row("B"));

    expect(screen.getByRole("menuitem", { name: "Squash 2 commits" })).toBeEnabled();
    expect(screen.getByRole("menuitem", { name: "Interactive rebase…" })).toBeEnabled();
    expect(screen.queryByText("Branch from here")).not.toBeInTheDocument();
  });

  it("Squash N commits opens the planner on the oldest selected commit's parent with the newer ones preset", () => {
    const { onRebaseSelection } = renderChain();

    fireEvent.click(row("C"));
    fireEvent.click(row("B"), { shiftKey: true });
    fireEvent.contextMenu(row("B"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Squash 2 commits" }));

    // C and B fold together onto B's parent A; B (oldest) stays the surviving Pick.
    expect(onRebaseSelection).toHaveBeenCalledWith("A", new Map([["C", "Squash"]]));
  });

  it("Ctrl+click toggles a non-adjacent commit into the selection", () => {
    renderChain();

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });

    expect(row("D")).toHaveAttribute("aria-selected", "true");
    expect(row("C")).toHaveAttribute("aria-selected", "false");
    expect(row("B")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("toolbar", { name: "Selected commits" })).toHaveTextContent(
      "2 commits selected",
    );
  });

  it("Cmd+click toggles too, and toggling a selected commit off removes it", () => {
    renderChain();

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { metaKey: true });
    fireEvent.click(row("B"), { metaKey: true });

    expect(row("B")).toHaveAttribute("aria-selected", "false");
    expect(screen.queryByRole("toolbar", { name: "Selected commits" })).not.toBeInTheDocument();
  });

  it("a non-adjacent selection offers Drop and Interactive rebase but disables Squash and Fixup", () => {
    const { onRebaseSelection } = renderChain();

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });

    expect(screen.getByRole("button", { name: "Squash 2 commits" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Fixup 2 commits" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Drop 2 commits" }));
    expect(onRebaseSelection).toHaveBeenLastCalledWith(
      "A",
      new Map([
        ["B", "Drop"],
        ["D", "Drop"],
      ]),
    );

    fireEvent.click(screen.getByRole("button", { name: "Interactive rebase…" }));
    expect(onRebaseSelection).toHaveBeenLastCalledWith(
      "A",
      new Map([
        ["B", "Pick"],
        ["D", "Pick"],
      ]),
    );
  });

  it("disables every multi-select action while a repository operation is pending", () => {
    renderChain({ operationsBlocked: true });

    fireEvent.click(row("C"));
    fireEvent.click(row("B"), { shiftKey: true });

    for (const name of [
      "Interactive rebase…",
      "Squash 2 commits",
      "Fixup 2 commits",
      "Drop 2 commits",
    ]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
  });

  it("explains why actions are disabled when the selection holds a root commit", () => {
    const rootChain: GraphCommit[] = [
      { ...commits[0], id: "B", shortId: "B", summary: "B", parentIds: ["A"] },
      { ...commits[0], id: "A", shortId: "A", summary: "A", parentIds: [] },
    ];
    renderChain({ commits: rootChain });

    fireEvent.click(row("B"));
    fireEvent.click(row("A"), { shiftKey: true });

    const drop = screen.getByRole("button", { name: "Drop 2 commits" });
    expect(drop).toBeDisabled();
    expect(drop).toHaveAttribute("title", "Merge and root commits can't be rebased.");
  });

  it("Escape collapses a multi-selection back to the focused commit", () => {
    renderChain();

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "Escape" });

    expect(screen.queryByRole("toolbar", { name: "Selected commits" })).not.toBeInTheDocument();
  });

  it("Escape keeps the focused commit selected", () => {
    renderChain();

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "Escape" });

    expect(row("B")).toHaveAttribute("aria-selected", "true");
    expect(row("D")).toHaveAttribute("aria-selected", "false");
  });

  it("Ctrl-toggling off the focused commit moves focus to a remaining selected commit", () => {
    const onSelectRow = vi.fn();
    renderChain({ onSelectRow });

    fireEvent.click(row("D"));
    fireEvent.click(row("C"), { ctrlKey: true });
    fireEvent.click(row("B"), { ctrlKey: true });
    fireEvent.click(row("B"), { ctrlKey: true });

    expect(row("B")).toHaveAttribute("aria-selected", "false");
    expect(row("D")).toHaveAttribute("aria-selected", "true");
    expect(row("C")).toHaveAttribute("aria-selected", "true");
    const focused = onSelectRow.mock.lastCall![0] as { commitId: string };
    expect(["D", "C"]).toContain(focused.commitId);
    expect(screen.getByRole("toolbar", { name: "Selected commits" })).toHaveTextContent(
      "2 commits selected",
    );
  });

  it("Ctrl-toggling off a non-focused commit keeps focus where it is", () => {
    const onSelectRow = vi.fn();
    renderChain({ onSelectRow });

    fireEvent.click(row("D"));
    fireEvent.click(row("C"), { ctrlKey: true });
    fireEvent.click(row("B"), { ctrlKey: true });
    // Focus is on B; toggle off C.
    fireEvent.click(row("C"), { ctrlKey: true });

    expect(onSelectRow).toHaveBeenLastCalledWith({ commitId: "B" });
    expect(row("C")).toHaveAttribute("aria-selected", "false");
  });

  it("clicking the Uncommitted row collapses a multi-selection", () => {
    renderChain();

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });
    fireEvent.click(screen.getByText(/Uncommitted changes/).closest("li")!);

    expect(screen.queryByRole("toolbar", { name: "Selected commits" })).not.toBeInTheDocument();
    expect(row("D")).toHaveAttribute("aria-selected", "false");
    expect(row("B")).toHaveAttribute("aria-selected", "false");
  });

  it("an external focus change collapses the selection, and it does not resurface later", () => {
    const props = {
      ...historyProps,
      status,
      commits: chainCommits,
      operationsBlocked: false,
      onSelectRow: vi.fn(),
      onBranchFromCommit: vi.fn(),
      onRebaseFromCommit: vi.fn(),
    };
    const view = render(<CommitGraph {...props} selectedRow={{ commitId: "D" }} />);
    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });
    // The graph reported B as focus; the static prop is the host's reaction.
    view.rerender(<CommitGraph {...props} selectedRow={{ commitId: "B" }} />);
    expect(screen.getByRole("toolbar", { name: "Selected commits" })).toBeInTheDocument();

    // Search/sidebar jumps focus to C, then back to former member D.
    view.rerender(<CommitGraph {...props} selectedRow={{ commitId: "C" }} />);
    expect(screen.queryByRole("toolbar", { name: "Selected commits" })).not.toBeInTheDocument();
    view.rerender(<CommitGraph {...props} selectedRow={{ commitId: "D" }} />);
    expect(screen.queryByRole("toolbar", { name: "Selected commits" })).not.toBeInTheDocument();
    expect(row("B")).toHaveAttribute("aria-selected", "false");
  });

  it("prunes selected ids that are no longer in the loaded commits", () => {
    const props = {
      ...historyProps,
      status,
      operationsBlocked: false,
      onSelectRow: vi.fn(),
      onBranchFromCommit: vi.fn(),
      onRebaseFromCommit: vi.fn(),
    };
    const view = render(
      <CommitGraph {...props} commits={chainCommits} selectedRow={{ commitId: "D" }} />,
    );
    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });
    view.rerender(<CommitGraph {...props} commits={chainCommits} selectedRow={{ commitId: "B" }} />);
    expect(screen.getByRole("toolbar", { name: "Selected commits" })).toBeInTheDocument();

    // D disappears (e.g. after a rebase/refresh): only B remains, so no multi-select toolbar.
    view.rerender(
      <CommitGraph {...props} commits={chainCommits.filter((c) => c.id !== "D")} selectedRow={{ commitId: "B" }} />,
    );
    expect(screen.queryByRole("toolbar", { name: "Selected commits" })).not.toBeInTheDocument();
    expect(row("B")).toHaveAttribute("aria-selected", "true");
  });

  it("right-clicking a commit outside the selection opens the normal single-commit menu", () => {
    renderChain();

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });
    fireEvent.contextMenu(row("C"));

    expect(screen.getByRole("menuitem", { name: "Branch from here" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Drop 2 commits" })).not.toBeInTheDocument();
  });

  it("disables Squash across a fork point but still offers Interactive rebase", () => {
    const forkCommits: GraphCommit[] = [
      { ...commits[0], id: "F1", shortId: "F1", summary: "F1", parentIds: ["M1"] },
      { ...commits[0], id: "M2", shortId: "M2", summary: "M2", parentIds: ["M1"] },
      { ...commits[0], id: "M1", shortId: "M1", summary: "M1", parentIds: ["R"] },
    ];
    renderChain({ commits: forkCommits });

    fireEvent.click(row("F1"));
    fireEvent.click(row("M2"), { shiftKey: true });
    fireEvent.contextMenu(row("M2"));

    expect(screen.getByRole("menuitem", { name: "Squash 2 commits" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Interactive rebase…" })).toBeEnabled();
  });

  it("disables Rebase onto here while a repository operation is pending", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={true}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);

    expect(screen.getByText("Rebase onto here")).toBeDisabled();
  });
});

describe("CommitGraph — keyboard access", () => {
  const chain: GraphCommit[] = [
    { ...commits[0], id: "C", shortId: "C", summary: "C", parentIds: ["B"] },
    { ...commits[0], id: "B", shortId: "B", summary: "B", parentIds: ["A"] },
    { ...commits[0], id: "A", shortId: "A", summary: "A", parentIds: [] },
  ];

  function setup(selectedRow: "uncommitted" | { commitId: string }) {
    const handlers = {
      onSelectRow: vi.fn(),
      onBranchFromCommit: vi.fn(),
      onRebaseFromCommit: vi.fn(),
      onRebaseSelection: vi.fn(),
    };
    const utils = render(
      <CommitGraph {...historyProps} status={status} commits={chain} selectedRow={selectedRow} operationsBlocked={false} {...handlers} />,
    );
    return { ...utils, ...handlers, list: screen.getByRole("listbox") };
  }

  it("asks for the search field on Ctrl/Cmd+F while the list has focus", () => {
    const onRequestSearch = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={chain}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        onRequestSearch={onRequestSearch}
      />,
    );
    const list = screen.getByRole("listbox", { name: "Commit history" });
    fireEvent.keyDown(list, { key: "f", ctrlKey: true });
    fireEvent.keyDown(list, { key: "F", metaKey: true });
    expect(onRequestSearch).toHaveBeenCalledTimes(2);
    expect(list.getAttribute("aria-keyshortcuts")).toContain("Control+F Meta+F");
  });

  it("advertises its shortcuts", () => {
    const { list } = setup("uncommitted");
    expect(list.getAttribute("aria-keyshortcuts")).toContain("Enter");
  });

  it("Enter opens the context menu for the selected commit and Branch from here uses its id", () => {
    const { list, onBranchFromCommit } = setup({ commitId: "B" });
    fireEvent.keyDown(list, { key: "Enter" });
    fireEvent.click(screen.getByRole("menuitem", { name: "Branch from here" }));
    expect(onBranchFromCommit).toHaveBeenCalledWith("B");
  });

  it("the Menu key and Shift+F10 open the same menu", () => {
    const { list, unmount } = setup({ commitId: "B" });
    fireEvent.keyDown(list, { key: "ContextMenu" });
    expect(screen.getByRole("menuitem", { name: "Rebase onto here" })).toBeInTheDocument();
    unmount();
    const second = setup({ commitId: "B" });
    fireEvent.keyDown(second.list, { key: "F10", shiftKey: true });
    expect(screen.getByRole("menuitem", { name: "Rebase onto here" })).toBeInTheDocument();
  });

  it("Enter on the Uncommitted changes row opens no menu", () => {
    const { list } = setup("uncommitted");
    fireEvent.keyDown(list, { key: "Enter" });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("Home and End jump to the first and last row", () => {
    const { list, onSelectRow } = setup({ commitId: "B" });
    fireEvent.keyDown(list, { key: "Home" });
    expect(onSelectRow).toHaveBeenLastCalledWith("uncommitted", "step");
    fireEvent.keyDown(list, { key: "End" });
    expect(onSelectRow).toHaveBeenLastCalledWith({ commitId: "A" }, "step");
  });

  it("PageDown and PageUp move by a page, clamped to the ends", () => {
    const { list, onSelectRow } = setup({ commitId: "B" });
    fireEvent.keyDown(list, { key: "PageDown" });
    expect(onSelectRow).toHaveBeenLastCalledWith({ commitId: "A" }, "step");
    fireEvent.keyDown(list, { key: "PageUp" });
    expect(onSelectRow).toHaveBeenLastCalledWith("uncommitted", "step");
  });

  it("Shift+ArrowDown extends the selection and the menu then offers Squash", () => {
    const { rerender, list, onSelectRow, onRebaseSelection, ...rest } = setup({ commitId: "C" });
    fireEvent.click(screen.getByText(/^C /).closest("li")!);
    fireEvent.keyDown(list, { key: "ArrowDown", shiftKey: true });
    expect(onSelectRow).toHaveBeenLastCalledWith({ commitId: "B" }, "step");
    rerender(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={chain}
        selectedRow={{ commitId: "B" }}
        operationsBlocked={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={rest.onBranchFromCommit}
        onRebaseFromCommit={rest.onRebaseFromCommit}
        onRebaseSelection={onRebaseSelection}
      />,
    );
    fireEvent.keyDown(list, { key: "Enter" });
    fireEvent.click(screen.getByRole("menuitem", { name: "Squash 2 commits" }));
    expect(onRebaseSelection).toHaveBeenCalledWith("A", new Map([["C", "Squash"]]));
  });

  it("keys pressed inside the open menu do not re-trigger list navigation", () => {
    const { list, onSelectRow } = setup({ commitId: "B" });
    fireEvent.keyDown(list, { key: "Enter" });
    onSelectRow.mockClear();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Branch from here" }), { key: "ArrowDown" });
    expect(onSelectRow).not.toHaveBeenCalled();
  });
});

describe("CommitGraph history metadata and pagination", () => {
  const baseProps = {
    ...historyProps,
    status,
    commits,
    selectedRow: "uncommitted" as const,
    operationsBlocked: false,
    onSelectRow: vi.fn(),
    onBranchFromCommit: vi.fn(),
    onRebaseFromCommit: vi.fn(),
  };

  it("shows each commit's author and date in the row", () => {
    render(<CommitGraph {...baseProps} />);

    expect(screen.getAllByText("Rene")).toHaveLength(2);
    expect(screen.getAllByTestId("commit-date")).toHaveLength(2);
  });

  it("says older history exists, and that it is loading (#685)", () => {
    const { rerender } = render(<CommitGraph {...baseProps} hasMore onLoadMore={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("Showing the latest 2 commits; scroll for older ones.");
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();

    rerender(<CommitGraph {...baseProps} hasMore loadingMore onLoadMore={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("Loading older commits…");
  });

  it("says nothing about older history when the whole history is loaded", () => {
    render(<CommitGraph {...baseProps} hasMore={false} onLoadMore={vi.fn()} />);

    expect(screen.queryByRole("status")).toBeNull();
  });

  it("loads more when keyboard navigation reaches the last row", () => {
    const onLoadMore = vi.fn();
    render(
      <CommitGraph {...baseProps} selectedRow={{ commitId: "bbb222..." }} hasMore onLoadMore={onLoadMore} />,
    );

    fireEvent.keyDown(screen.getByRole("listbox", { name: "Commit history" }), { key: "ArrowDown" });

    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });

  it("draws a hollow working-tree node in the lane column of the Uncommitted changes row", () => {
    render(<CommitGraph {...baseProps} />);

    expect(screen.getByTestId("working-tree-node")).toBeInTheDocument();
  });
});

describe("CommitGraph — history actions", () => {
  const chain: GraphCommit[] = [
    { ...commits[0], id: "C", shortId: "C", summary: "C", parentIds: ["B"] },
    { ...commits[0], id: "M", shortId: "M", summary: "M", parentIds: ["B", "A"] },
    { ...commits[0], id: "B", shortId: "B", summary: "B", parentIds: ["A"] },
    { ...commits[0], id: "A", shortId: "A", summary: "A", parentIds: [] },
  ];

  function setup(overrides: Partial<ComponentProps<typeof CommitGraph>> = {}) {
    const handlers = {
      onSelectRow: vi.fn(),
      onBranchFromCommit: vi.fn(),
      onRebaseFromCommit: vi.fn(),
      onRebaseSelection: vi.fn(),
      onCherryPick: vi.fn(),
      onRevert: vi.fn(),
      onCheckoutCommit: vi.fn(),
      onChooseParent: vi.fn(),
      onRequestReset: vi.fn(),
      onEditHeadMessage: vi.fn(),
    };
    render(
      <CommitGraph
        status={status}
        commits={chain}
        selectedRow="uncommitted"
        operationsBlocked={false}
        headInfo={{ ...NO_HEAD, headId: "C" }}
        currentBranch="main"
        {...handlers}
        {...overrides}
      />,
    );
    const openMenu = (summary: string) =>
      fireEvent.contextMenu(screen.getByText(`${summary} ${summary}`).closest("li")!);
    return { ...handlers, openMenu };
  }

  it("offers cherry-pick, revert, checkout and reset on a single commit", () => {
    const { openMenu, onCherryPick, onRevert, onCheckoutCommit, onRequestReset } = setup();
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick" }));
    expect(onCherryPick).toHaveBeenCalledWith("B", null);
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Revert" }));
    expect(onRevert).toHaveBeenCalledWith("B", null);
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Switch to (detached)" }));
    expect(onCheckoutCommit).toHaveBeenCalledWith("B");
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Reset main to here…" }));
    // The reset dialog is RepoWorkspace's (#514); focus returns to this list (#146).
    expect(onRequestReset).toHaveBeenCalledWith("B", screen.getByRole("listbox", { name: "Commit history" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("disables cherry-pick on HEAD and rewords HEAD in the amend box", () => {
    const { openMenu, onEditHeadMessage } = setup();
    openMenu("C");
    expect(screen.getByRole("menuitem", { name: "Cherry-pick" })).toBeDisabled();
    fireEvent.click(screen.getByRole("menuitem", { name: "Reword…" }));
    expect(onEditHeadMessage).toHaveBeenCalled();
    openMenu("M");
    expect(screen.queryByRole("menuitem", { name: "Reword…" })).not.toBeInTheDocument();
  });

  it("asks RepoWorkspace for the parent picker of a merge commit (#514)", () => {
    const { openMenu, onCherryPick, onRevert, onChooseParent } = setup();
    const list = screen.getByRole("listbox", { name: "Commit history" });
    openMenu("M");
    expect(screen.queryByRole("menuitem", { name: "Cherry-pick" })).toBeNull();
    fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick…" }));
    expect(onChooseParent).toHaveBeenCalledWith("Cherry-pick", "M", list);
    openMenu("M");
    fireEvent.click(screen.getByRole("menuitem", { name: "Revert…" }));
    expect(onChooseParent).toHaveBeenCalledWith("Revert", "M", list);
    expect(onCherryPick).not.toHaveBeenCalled();
    expect(onRevert).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  // The dialogs themselves are RepoWorkspace's (#514, see App.test); the menu hands focus
  // straight to the list (#146).
  it("moves focus to the commit list when a dialog entry is chosen, wherever focus was before", () => {
    const { openMenu } = setup();
    const outside = document.createElement("button");
    outside.textContent = "outside";
    document.body.appendChild(outside);
    try {
      const list = screen.getByRole("listbox", { name: "Commit history" });
      for (const [summary, item] of [
        ["B", "Reset main to here…"],
        ["M", "Cherry-pick…"],
      ] as const) {
        outside.focus();
        const outsideFocus = vi.spyOn(outside, "focus");
        openMenu(summary);
        fireEvent.click(screen.getByRole("menuitem", { name: item }));
        // Focus goes straight to the list, never back through the element focused before.
        expect(outsideFocus).not.toHaveBeenCalled();
        outsideFocus.mockRestore();
        expect(list).toHaveFocus();
      }
    } finally {
      outside.remove();
    }
  });

  it("disables Cherry-pick… on a merge commit at HEAD", () => {
    const { openMenu } = setup({ headInfo: { ...NO_HEAD, headId: "M" } });
    openMenu("M");
    expect(screen.getByRole("menuitem", { name: "Cherry-pick…" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Revert…" })).toBeEnabled();
  });

  it("disables the new actions while pending", () => {
    const { openMenu } = setup({ operationsBlocked: true });
    openMenu("B");
    for (const name of ["Cherry-pick", "Revert", "Switch to (detached)", "Reset main to here…"]) {
      expect(screen.getByRole("menuitem", { name })).toBeDisabled();
    }
  });

  it("disables reset on a detached HEAD", () => {
    const { openMenu } = setup({ headInfo: { ...NO_HEAD, headId: "C", detachedAt: "C" }, currentBranch: null });
    openMenu("B");
    expect(screen.getByRole("menuitem", { name: "Reset HEAD to here…" })).toBeDisabled();
  });
});

describe("CommitGraph — one-click history edits", () => {
  // Newest first: E -> D -> C -> B -> A (root); X is on another branch, off C.
  const line = (id: string, parentIds: string[]): GraphCommit => ({
    ...commits[0],
    id,
    shortId: id,
    summary: id,
    parentIds,
  });
  const chain: GraphCommit[] = [line("E", ["D"]), line("X", ["C"]), line("D", ["C"]), line("C", ["B"]), line("B", ["A"]), line("A", [])];
  const EDIT_ITEMS = ["Reword…", "Squash with parent", "Fixup into parent", "Move up", "Move down", "Drop"];

  function setup(overrides: Partial<ComponentProps<typeof CommitGraph>> = {}) {
    const handlers = {
      onSelectRow: vi.fn(),
      onBranchFromCommit: vi.fn(),
      onRebaseFromCommit: vi.fn(),
      onCherryPick: vi.fn(),
      onRevert: vi.fn(),
      onCheckoutCommit: vi.fn(),
      onChooseParent: vi.fn(),
      onRequestReset: vi.fn(),
      onEditHeadMessage: vi.fn(),
      onEditHistory: vi.fn(),
      onRewordCommit: vi.fn(),
    };
    render(
      <CommitGraph
        status={[]}
        commits={chain}
        selectedRow="uncommitted"
        operationsBlocked={false}
        headInfo={{ ...NO_HEAD, headId: "E" }}
        currentBranch="main"
        {...handlers}
        {...overrides}
      />,
    );
    const openMenu = (summary: string) => fireEvent.contextMenu(screen.getByText(`${summary} ${summary}`).closest("li")!);
    return { ...handlers, openMenu };
  }

  it("runs each edit of an older commit from the right base", () => {
    const { openMenu, onEditHistory, onRewordCommit } = setup();
    const cases: [string, unknown, string][] = [
      ["Squash with parent", { kind: "Squash" }, "A"],
      ["Fixup into parent", { kind: "Fixup" }, "A"],
      ["Move up", { kind: "MoveUp" }, "B"],
      ["Move down", { kind: "MoveDown" }, "A"],
      ["Drop", { kind: "Drop" }, "B"],
    ];
    for (const [name, request, onto] of cases) {
      openMenu("C");
      fireEvent.click(screen.getByRole("menuitem", { name }));
      expect(onEditHistory).toHaveBeenLastCalledWith("C", onto, request);
    }
    openMenu("C");
    fireEvent.click(screen.getByRole("menuitem", { name: "Reword…" }));
    expect(onRewordCommit).toHaveBeenCalledWith("C", "B");
  });

  it("rewords HEAD in the amend box and can't move it up", () => {
    const { openMenu, onEditHeadMessage, onRewordCommit } = setup();
    openMenu("E");
    expect(screen.getByRole("menuitem", { name: "Move up" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Move up" })).toHaveAttribute("title", "This is already the newest commit.");
    fireEvent.click(screen.getByRole("menuitem", { name: "Reword…" }));
    expect(onEditHeadMessage).toHaveBeenCalled();
    expect(onRewordCommit).not.toHaveBeenCalled();
  });

  it("disables the edits with a reason off the current branch, on a dirty tree and while busy", () => {
    const { openMenu } = setup();
    openMenu("X");
    for (const name of EDIT_ITEMS) {
      const item = screen.getByRole("menuitem", { name });
      expect(item).toBeDisabled();
      expect(item).toHaveAttribute("title", "Only commits of the current branch can be edited.");
    }
  });

  it("needs a clean tracked tree", () => {
    const { openMenu } = setup({ status });
    openMenu("C");
    for (const name of EDIT_ITEMS) {
      expect(screen.getByRole("menuitem", { name })).toHaveAttribute("title", "Commit or stash your changes first.");
    }
  });

  it("disables every edit while an operation is pending", () => {
    const { openMenu } = setup({ operationsBlocked: true });
    openMenu("C");
    for (const name of EDIT_ITEMS) expect(screen.getByRole("menuitem", { name })).toBeDisabled();
  });

  it("is reachable from the keyboard through the row's menu", () => {
    const { onEditHistory } = setup({ selectedRow: { commitId: "D" } });
    fireEvent.keyDown(screen.getByRole("listbox", { name: "Commit history" }), { key: "Enter" });
    fireEvent.click(screen.getByRole("menuitem", { name: "Drop" }));
    expect(onEditHistory).toHaveBeenCalledWith("D", "C", { kind: "Drop" });
  });
});

describe("CommitGraph — tag badges", () => {
  const remotes = [
    { name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null },
    { name: "backup", fetchUrl: "y", pushUrl: null, authMode: null, authUsername: null },
  ];
  const tagged: GraphCommit[] = [{ ...commits[0], tags: ["v2.0"] }, ...commits.slice(1)];

  function renderGraph(overrides: Partial<ComponentProps<typeof CommitGraph>> = {}) {
    const handlers = {
      onSelectRow: vi.fn(),
      onBranchFromCommit: vi.fn(),
      onRebaseFromCommit: vi.fn(),
      onCherryPick: vi.fn(),
      onRevert: vi.fn(),
      onCheckoutCommit: vi.fn(),
      onChooseParent: vi.fn(),
      onRequestReset: vi.fn(),
      onEditHeadMessage: vi.fn(),
    };
    const graph = (more: Partial<ComponentProps<typeof CommitGraph>>) => (
      <CommitGraph
        status={[]}
        commits={tagged}
        selectedRow="uncommitted"
        operationsBlocked={false}
        headInfo={{ headId: null, detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }}
        currentBranch="main"
        {...handlers}
        {...overrides}
        {...more}
      />
    );
    const { rerender } = render(graph({}));
    return { ...handlers, rerenderWith: (more: Partial<ComponentProps<typeof CommitGraph>>) => rerender(graph(more)) };
  }

  it("blocks an open tag delete confirmation, saying why, once the repository is busy (#521)", () => {
    const onDeleteTag = vi.fn(async () => {});
    const { rerenderWith } = renderGraph({ remotes, onPushTag: vi.fn(), onDeleteTag });
    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete tag…" }));

    rerenderWith({ operationsBlocked: true, operationsBlockedReason: "A transfer is in progress." });
    const dialog = screen.getByRole("dialog", { name: "Delete local tag v2.0" });
    expect(dialog).toHaveTextContent("A transfer is in progress.");
    const confirm = within(dialog).getByRole("button", { name: "Delete tag" });
    expect(confirm).toBeDisabled();
    expect(confirm).toHaveAttribute("title", "A transfer is in progress.");
    expect(describedByText(confirm)).toBe("A transfer is in progress.");
    fireEvent.click(confirm);
    expect(onDeleteTag).not.toHaveBeenCalled();
  });

  it("opens a tag menu with push per remote, checkout, and a confirmed delete", async () => {
    const onPushTag = vi.fn();
    const onDeleteTag = vi.fn(async () => {});
    const { onCheckoutCommit } = renderGraph({ remotes, onPushTag, onDeleteTag });

    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Push v2.0 to backup" }));
    expect(onPushTag).toHaveBeenCalledWith("backup", "v2.0");

    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    expect(screen.getByRole("menuitem", { name: "Push v2.0 to origin" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Cherry-pick" })).toBeNull();
    fireEvent.click(screen.getByRole("menuitem", { name: "Switch to (detached)" }));
    expect(onCheckoutCommit).toHaveBeenCalledWith(tagged[0].id);

    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete tag…" }));
    const dialog = screen.getByRole("dialog", { name: "Delete local tag v2.0" });
    expect(dialog).toHaveTextContent("Delete local tag v2.0?");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete tag" }));
    await waitFor(() => expect(onDeleteTag).toHaveBeenCalledWith("v2.0"));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Delete local tag v2.0" })).toBeNull());
  });

  it("disables tag push without remotes", () => {
    renderGraph({ remotes: [], onPushTag: vi.fn(), onDeleteTag: vi.fn(async () => {}) });
    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    const push = screen.getByRole("menuitem", { name: "Push tag" });
    expect(push).toBeDisabled();
    expect(push).toHaveAttribute("title", "No remote configured.");
  });

  it("falls back to the commit menu when no tag actions are wired", () => {
    renderGraph();
    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    expect(screen.getByRole("menuitem", { name: "Cherry-pick" })).toBeInTheDocument();
  });

  it("offers Branch from tag and Delete from remote… on a tag badge", () => {
    const onDeleteTagFromRemote = vi.fn();
    const { onBranchFromCommit } = renderGraph({
      remotes,
      onPushTag: vi.fn(),
      onDeleteTag: vi.fn(async () => {}),
      onDeleteTagFromRemote,
    });

    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Branch from tag" }));
    expect(onBranchFromCommit).toHaveBeenCalledWith(tagged[0].id);

    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete from remote…" }));
    expect(onDeleteTagFromRemote).toHaveBeenCalledWith("v2.0");
  });

  it("disables Delete from remote… without remotes", () => {
    renderGraph({ remotes: [], onPushTag: vi.fn(), onDeleteTag: vi.fn(async () => {}), onDeleteTagFromRemote: vi.fn() });
    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    const item = screen.getByRole("menuitem", { name: "Delete from remote…" });
    expect(item).toBeDisabled();
    expect(item).toHaveAttribute("title", "No remote configured.");
  });
});

describe("CommitGraph — Create tag…", () => {
  function renderGraph(overrides: Partial<ComponentProps<typeof CommitGraph>> = {}) {
    render(
      <CommitGraph
        status={[]}
        commits={commits}
        selectedRow={{ commitId: commits[1].id }}
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        onCherryPick={vi.fn()}
        onRevert={vi.fn()}
        onCheckoutCommit={vi.fn()}
        onChooseParent={vi.fn()}
        onRequestReset={vi.fn()}
        onEditHeadMessage={vi.fn()}
        headInfo={{ headId: null, detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }}
        currentBranch="main"
        {...overrides}
      />,
    );
  }

  it("is in the commit menu and opens with the keyboard for the selected commit", () => {
    const onCreateTag = vi.fn();
    renderGraph({ onCreateTag });

    fireEvent.keyDown(screen.getByRole("listbox", { name: "Commit history" }), { key: "Enter" });
    fireEvent.click(screen.getByRole("menuitem", { name: "Create tag…" }));

    expect(onCreateTag).toHaveBeenCalledWith(commits[1].id);
  });

  it("is absent when no handler is wired", () => {
    renderGraph();
    fireEvent.keyDown(screen.getByRole("listbox", { name: "Commit history" }), { key: "Enter" });
    expect(screen.queryByRole("menuitem", { name: "Create tag…" })).toBeNull();
  });
});

describe("CommitGraph — parent and child keys (#618)", () => {
  const node = (id: string, parentIds: string[]): GraphCommit => ({
    ...commits[0],
    id,
    shortId: id,
    summary: `${id} summary`,
    parentIds,
  });
  // T on M; M merges S into B; B and S both start from A, whose parent R is not loaded.
  const graph = [node("T", ["M"]), node("M", ["B", "S"]), node("S", ["A"]), node("B", ["A"]), node("A", ["R"])];

  function setup(selected: string, graphCommits: GraphCommit[] = graph) {
    const onSelectRow = vi.fn();
    const onGoToCommit = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={graphCommits}
        selectedRow={{ commitId: selected }}
        operationsBlocked={false}
        onSelectRow={onSelectRow}
        onGoToCommit={onGoToCommit}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );
    return { onSelectRow, onGoToCommit, list: screen.getByRole("listbox", { name: "Commit history" }) };
  }

  it("[ selects the first parent", () => {
    const { list, onSelectRow } = setup("M");
    fireEvent.keyDown(list, { key: "[" });
    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "B" });
    expect(list.getAttribute("aria-keyshortcuts")).toContain("[ ] Shift+[");
  });

  it("[ finds a parent outside the loaded history as Go to does", () => {
    const { list, onSelectRow, onGoToCommit } = setup("A");
    fireEvent.keyDown(list, { key: "[" });
    expect(onGoToCommit).toHaveBeenCalledWith("R");
    expect(onSelectRow).not.toHaveBeenCalled();
  });

  it("Shift+[ on a merge commit picks among its parents", () => {
    const { list, onSelectRow } = setup("M");
    fireEvent.keyDown(list, { key: "{", shiftKey: true });
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Parent 1: B B summary",
      "Parent 2: S S summary",
    ]);
    fireEvent.click(screen.getByRole("menuitem", { name: "Parent 2: S S summary" }));
    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "S" });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("Shift+[ on a commit with one parent goes straight to it", () => {
    const { list, onSelectRow } = setup("B");
    fireEvent.keyDown(list, { key: "{", shiftKey: true });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "A" });
  });

  it("] selects the only child, and of several the one on the same lane", () => {
    const first = setup("M");
    fireEvent.keyDown(first.list, { key: "]" });
    expect(first.onSelectRow).toHaveBeenCalledWith({ commitId: "T" });
    cleanup();
    // S, the row above B, reserves A's lane first, so A is drawn on S's lane, not B's.
    const lanes = assignLanes(graph);
    expect(lanes[4].lane).toBe(lanes[2].lane);
    expect(lanes[4].lane).not.toBe(lanes[3].lane);
    const second = setup("A");
    fireEvent.keyDown(second.list, { key: "]" });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(second.onSelectRow).toHaveBeenCalledWith({ commitId: "S" });
  });

  it("] offers a picker when no child is on the commit's lane", () => {
    // Q is the second parent of both merges, so it has a lane of its own.
    const merges = [node("M1", ["X1", "Q"]), node("M2", ["X2", "Q"]), node("X1", []), node("X2", []), node("Q", [])];
    const { list, onSelectRow } = setup("Q", merges);
    fireEvent.keyDown(list, { key: "]" });
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["M1 M1 summary", "M2 M2 summary"]);
    fireEvent.click(screen.getByRole("menuitem", { name: "M2 M2 summary" }));
    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "M2" });
  });

  it("] without a loaded child and Cmd+[ (Back on macOS) do nothing here", () => {
    const { list, onSelectRow, onGoToCommit } = setup("T");
    fireEvent.keyDown(list, { key: "]" });
    fireEvent.keyDown(list, { key: "[", metaKey: true });
    expect(onSelectRow).not.toHaveBeenCalled();
    expect(onGoToCommit).not.toHaveBeenCalled();
  });
});

describe("CommitGraph — copy items (#614)", () => {
  function renderBusyGraph() {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(
      <CommitGraph
        {...historyProps}
        status={[]}
        commits={[{ ...commits[0], timestamp: 1_700_000_000, tags: ["v2.0"] }, ...commits.slice(1)]}
        selectedRow="uncommitted"
        operationsBlocked
        operationsBlockedReason="A transfer is in progress."
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        onPushTag={vi.fn()}
        onDeleteTag={vi.fn(async () => {})}
      />,
    );
    return writeText;
  }

  it("ends the commit menu with enabled copy items that write the expected text", () => {
    const writeText = renderBusyGraph();
    const expected: [string, string][] = [
      ["Copy SHA", "aaa111..."],
      ["Copy short SHA", "aaa1111"],
      ["Copy subject", "second commit"],
      ["Copy author", "Rene <rene@example.com>"],
      ["Copy date", "2023-11-14T22:13:20.000Z"],
    ];
    for (const [label, text] of expected) {
      fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);
      const names = screen.getAllByRole("menuitem").map((item) => item.textContent);
      expect(names.slice(-expected.length)).toEqual(expected.map(([name]) => name));
      const item = screen.getByRole("menuitem", { name: label });
      expect(item).toBeEnabled();
      fireEvent.click(item);
      expect(writeText).toHaveBeenLastCalledWith(text);
    }
  });

  it("groups the commit menu with separators: refs, apply, HEAD, edits, Drop, exports, copying (#645)", () => {
    renderBusyGraph();
    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);
    const groups: string[][] = [[]];
    for (const child of screen.getByRole("menu").children) {
      if (child.getAttribute("role") === "separator") groups.push([]);
      // The item's label, not a disabled item's reason under it (#664).
      else groups[groups.length - 1].push(child.querySelector('[role^="menuitem"]')?.textContent ?? "");
    }
    expect(groups[0][0]).toBe("Branch from here");
    expect(groups.find((group) => group.includes("Rebase onto here"))).not.toContain("Branch from here");
    expect(groups[groups.length - 1]).toEqual([
      "Copy SHA",
      "Copy short SHA",
      "Copy subject",
      "Copy author",
      "Copy date",
    ]);
    expect(groups.every((group) => group.length > 0)).toBe(true);
  });

  it("offers Copy tag name in the tag badge menu, before the delete item", () => {
    const writeText = renderBusyGraph();
    fireEvent.contextMenu(screen.getByTitle("Tag v2.0"));
    const names = screen.getAllByRole("menuitem").map((item) => item.textContent);
    expect(names.slice(-2)).toEqual(["Copy tag name", "Delete tag…"]);
    const item = screen.getByRole("menuitem", { name: "Copy tag name" });
    expect(item).toBeEnabled();
    fireEvent.click(item);
    expect(writeText).toHaveBeenCalledWith("v2.0");
  });
});

describe("CommitGraph lines-changed badges (#620)", () => {
  const baseProps = {
    ...historyProps,
    status,
    commits,
    selectedRow: "uncommitted" as const,
    operationsBlocked: false,
    onSelectRow: vi.fn(),
    onBranchFromCommit: vi.fn(),
    onRebaseFromCommit: vi.fn(),
  };
  const counted = { filesChanged: 2, insertions: 12, deletions: 3, files: [] };

  it("shows a counted commit's badge, named for screen readers, and nothing for the others", () => {
    const lineStats = new Map([
      ["aaa111...", counted],
      ["bbb222...", null],
    ]);
    render(<CommitGraph {...baseProps} lineStats={lineStats} />);

    const rows = screen.getAllByRole("option").slice(1);
    expect(within(rows[0]).getByTestId("line-stats")).toHaveTextContent("12 lines added, 3 removed+12−3");
    expect(rows[0]).toHaveAccessibleName(expect.stringContaining("12 lines added, 3 removed"));
    // Over the cap: no badge rather than a wrong number.
    expect(within(rows[1]).queryByTestId("line-stats")).toBeNull();
  });

  it("looks the same as without badges until the numbers arrive", () => {
    const { container: without } = render(<CommitGraph {...baseProps} />);
    const plain = without.innerHTML;
    cleanup();
    const { container } = render(<CommitGraph {...baseProps} lineStats={new Map()} />);
    expect(container.innerHTML).toBe(plain);
  });

  it("reports the rows on screen, in graph order, as they come into view", () => {
    const observed: Element[] = [];
    let report: (records: Partial<IntersectionObserverEntry>[]) => void = () => {};
    const original = globalThis.IntersectionObserver;
    globalThis.IntersectionObserver = class {
      constructor(callback: (records: Partial<IntersectionObserverEntry>[]) => void) {
        report = callback;
      }
      observe(element: Element) {
        observed.push(element);
      }
      disconnect() {}
    } as unknown as typeof IntersectionObserver;
    try {
      const onCommitsVisible = vi.fn();
      render(<CommitGraph {...baseProps} onCommitsVisible={onCommitsVisible} />);
      expect(observed.map((row) => row.getAttribute("data-commit-id"))).toEqual(["aaa111...", "bbb222..."]);
      expect(onCommitsVisible).not.toHaveBeenCalled();

      report([{ target: observed[1], isIntersecting: true }]);
      expect(onCommitsVisible).toHaveBeenLastCalledWith(["bbb222..."]);
      report([
        { target: observed[0], isIntersecting: true },
        { target: observed[1], isIntersecting: false },
      ]);
      expect(onCommitsVisible).toHaveBeenLastCalledWith(["aaa111..."]);
    } finally {
      globalThis.IntersectionObserver = original;
    }
  });

  it("counts every loaded row as visible without IntersectionObserver", () => {
    const onCommitsVisible = vi.fn();
    render(<CommitGraph {...baseProps} onCommitsVisible={onCommitsVisible} />);
    expect(onCommitsVisible).toHaveBeenCalledWith(["aaa111...", "bbb222..."]);
  });
});

describe("CommitGraph collapsed merges (#617)", () => {
  const graphCommit = (id: string, parentIds: string[], extra: Partial<GraphCommit> = {}): GraphCommit => ({
    id,
    shortId: id.slice(0, 7),
    summary: `summary ${id}`,
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 0,
    parentIds,
    branchRefs: [],
    remoteBranchRefs: [],
    ...extra,
  });
  const graphRows = [
    graphCommit("after", ["merge"]),
    graphCommit("merge", ["direct", "f3"], { mergedCount: 3 }),
    graphCommit("direct", ["base"]),
    graphCommit("base", []),
  ];
  const side = [graphCommit("f3", ["f2"]), graphCommit("f2", ["f1"]), graphCommit("f1", ["base"])];

  function Harness({
    initialRow = "uncommitted",
    initialExpanded = {},
    onSelectRow = vi.fn(),
    onExpandMerge = vi.fn(),
    onCollapseMerge = vi.fn(),
    onCommitsVisible,
  }: {
    initialRow?: SelectedRow;
    initialExpanded?: Record<string, MergeExpansion>;
    onSelectRow?: (row: SelectedRow, kind?: VisitKind) => void;
    onExpandMerge?: (mergeId: string, atLeast?: number) => void;
    onCollapseMerge?: (mergeId: string) => void;
    onCommitsVisible?: (ids: string[]) => void;
  }) {
    const [selectedRow, setSelectedRow] = useState<SelectedRow>(initialRow);
    const [expanded, setExpanded] = useState<Record<string, MergeExpansion>>(initialExpanded);
    return (
      <CommitGraph
        {...historyProps}
        status={[]}
        commits={withExpandedMerges(graphRows, expanded)}
        selectedRow={selectedRow}
        operationsBlocked={false}
        onSelectRow={(...args) => {
          onSelectRow(...args);
          setSelectedRow(args[0]);
        }}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        expandedMerges={expanded}
        onExpandMerge={(mergeId, atLeast) => {
          onExpandMerge(mergeId, atLeast);
          setExpanded((prev) => ({ ...prev, [mergeId]: { commits: side, limit: 100 } }));
        }}
        onCollapseMerge={(mergeId) => {
          onCollapseMerge(mergeId);
          setExpanded((prev) => Object.fromEntries(Object.entries(prev).filter(([id]) => id !== mergeId)));
        }}
        onCommitsVisible={onCommitsVisible}
      />
    );
  }

  const rowIds = () =>
    screen
      .getAllByRole("option")
      .slice(1)
      .map((row) => row.getAttribute("data-commit-id"));
  const list = () => screen.getByRole("listbox", { name: "Commit history" });

  it("offers +N on a merge row only, and expands and collapses its commits below it", () => {
    const onExpandMerge = vi.fn();
    const onCollapseMerge = vi.fn();
    render(<Harness onExpandMerge={onExpandMerge} onCollapseMerge={onCollapseMerge} />);

    expect(rowIds()).toEqual(["after", "merge", "direct", "base"]);
    const toggle = screen.getByRole("button", { name: "Show the 3 commits this merge brought in" });
    expect(toggle).toHaveTextContent("+3");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.getAllByRole("button", { name: /this merge brought in/ })).toHaveLength(1);

    fireEvent.click(toggle);
    expect(onExpandMerge).toHaveBeenCalledWith("merge", undefined);
    expect(rowIds()).toEqual(["after", "merge", "f3", "f2", "f1", "direct", "base"]);
    const hide = screen.getByRole("button", { name: "Hide the 3 commits this merge brought in" });
    expect(hide).toHaveTextContent("−3");
    expect(hide).toHaveAttribute("aria-expanded", "true");
    for (const id of ["f3", "f2", "f1"]) {
      expect(document.querySelector(`[data-commit-id="${id}"]`)).toHaveClass(styles.mergedRow);
    }

    fireEvent.click(hide);
    expect(onCollapseMerge).toHaveBeenCalledWith("merge");
    expect(rowIds()).toEqual(["after", "merge", "direct", "base"]);
  });

  it("clicking the control does not select the row", () => {
    const onSelectRow = vi.fn();
    render(<Harness onSelectRow={onSelectRow} />);

    fireEvent.click(screen.getByRole("button", { name: /Show the 3 commits/ }));

    expect(onSelectRow).not.toHaveBeenCalled();
  });

  it("shows no control without an expand handler", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={[]}
        commits={graphRows}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.queryByRole("button", { name: /this merge brought in/ })).toBeNull();
  });

  it("expands with ArrowRight, steps through the merged rows, and goes back with ArrowLeft", () => {
    const onExpandMerge = vi.fn();
    const onCollapseMerge = vi.fn();
    const onSelectRow = vi.fn();
    render(
      <Harness
        initialRow={{ commitId: "merge" }}
        onSelectRow={onSelectRow}
        onExpandMerge={onExpandMerge}
        onCollapseMerge={onCollapseMerge}
      />,
    );

    fireEvent.keyDown(list(), { key: "ArrowRight" });
    expect(onExpandMerge).toHaveBeenCalledWith("merge", undefined);
    // ArrowRight on an expanded merge does nothing more.
    fireEvent.keyDown(list(), { key: "ArrowRight" });
    expect(onExpandMerge).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(list(), { key: "ArrowDown" });
    fireEvent.keyDown(list(), { key: "ArrowDown" });
    expect(onSelectRow).toHaveBeenLastCalledWith({ commitId: "f2" }, "step");
    expect(screen.getAllByRole("option")[4]).toHaveAttribute("aria-selected", "true");

    // From a merged commit, ArrowLeft goes to its merge; on the merge, it collapses.
    fireEvent.keyDown(list(), { key: "ArrowLeft" });
    expect(onSelectRow).toHaveBeenLastCalledWith({ commitId: "merge" }, "step");
    expect(onCollapseMerge).not.toHaveBeenCalled();
    fireEvent.keyDown(list(), { key: "ArrowLeft" });
    expect(onCollapseMerge).toHaveBeenCalledWith("merge");
    expect(rowIds()).toEqual(["after", "merge", "direct", "base"]);
  });

  it("opens the commit menu on a merged commit like on any row", () => {
    render(<Harness initialRow={{ commitId: "f2" }} initialExpanded={{ merge: { commits: side, limit: 100 } }} />);

    fireEvent.keyDown(list(), { key: "Enter" });

    expect(screen.getByRole("menuitem", { name: "Branch from here" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Copy SHA" })).toBeInTheDocument();
  });

  it("selects the merge when it collapses with the selection inside it", () => {
    const onSelectRow = vi.fn();
    render(
      <Harness
        initialRow={{ commitId: "f1" }}
        initialExpanded={{ merge: { commits: side, limit: 100 } }}
        onSelectRow={onSelectRow}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Hide the 3 commits/ }));

    expect(onSelectRow).toHaveBeenLastCalledWith({ commitId: "merge" });
    expect(screen.getAllByRole("option")[2]).toHaveAttribute("aria-selected", "true");
  });

  it("offers More while a long side branch has more commits than it loaded", () => {
    const onExpandMerge = vi.fn();
    render(
      <Harness initialExpanded={{ merge: { commits: side.slice(0, 2), limit: 2 } }} onExpandMerge={onExpandMerge} />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Show more merged commits (2 of 3 shown)" }));

    expect(onExpandMerge).toHaveBeenCalledWith("merge", 102);
  });

  it("reports only the rows shown for the lines-changed badges, never a collapsed merge's commits", () => {
    const onCommitsVisible = vi.fn();
    render(<Harness onCommitsVisible={onCommitsVisible} />);
    expect(onCommitsVisible).toHaveBeenLastCalledWith(["after", "merge", "direct", "base"]);

    fireEvent.click(screen.getByRole("button", { name: /Show the 3 commits/ }));

    expect(onCommitsVisible).toHaveBeenLastCalledWith(["after", "merge", "f3", "f2", "f1", "direct", "base"]);
  });
});

// #685: only the rows in view render, inside the scrolling history pane.
describe("CommitGraph — windowed rows", () => {
  const ROW = 26;
  const many: GraphCommit[] = Array.from({ length: 1000 }, (_, n) => ({
    ...commits[0],
    id: `id${n}`,
    shortId: `id${n}`,
    summary: `commit ${n}`,
    parentIds: n < 999 ? [`id${n + 1}`] : [],
  }));

  // jsdom has no layout: every `li` is ROW tall, and the pane (260px high, at the top of the
  // page) scrolls the list, which starts at its top, up by its `scrollTop`.
  function layout() {
    const original = Element.prototype.getBoundingClientRect;
    Element.prototype.getBoundingClientRect = function (this: Element) {
      const height = this.tagName === "LI" ? ROW : 0;
      const top = this.tagName === "UL" ? -(this.parentElement?.scrollTop ?? 0) : 0;
      return { top, left: 0, right: 0, bottom: top + height, width: 0, height, x: 0, y: top, toJSON: () => ({}) };
    };
    return () => {
      Element.prototype.getBoundingClientRect = original;
    };
  }

  function renderInPane(props: Partial<ComponentProps<typeof CommitGraph>> = {}) {
    const restore = layout();
    const pane = document.createElement("div");
    pane.style.overflowY = "auto";
    Object.defineProperty(pane, "clientHeight", { configurable: true, value: 260 });
    document.body.appendChild(pane);
    const utils = render(
      <CommitGraph
        {...historyProps}
        status={[]}
        commits={many}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        {...props}
      />,
      { container: pane },
    );
    const scrollTo = (top: number) => {
      pane.scrollTop = top;
      fireEvent.scroll(pane);
    };
    return { ...utils, pane, scrollTo, restore };
  }

  it("renders the rows in view plus a margin, and pads for the rest", () => {
    const { restore } = renderInPane();
    try {
      const list = screen.getByRole("listbox", { name: "Commit history" });
      const rendered = within(list).getAllByRole("option");
      // Rows 0–9 are in view, 20 more below them.
      expect(rendered).toHaveLength(30);
      expect(rendered[0]).toHaveTextContent("Uncommitted changes");
      expect(rendered[1]).toHaveAttribute("aria-posinset", "2");
      expect(rendered[1]).toHaveAttribute("aria-setsize", "1001");
      expect(list.style.paddingTop).toBe("0px");
      expect(list.style.paddingBottom).toBe(`${(1001 - 30) * ROW}px`);
    } finally {
      restore();
    }
  });

  it("renders the rows of the scrolled-to part", () => {
    const { scrollTo, restore } = renderInPane();
    try {
      scrollTo(ROW * 500);
      const list = screen.getByRole("listbox", { name: "Commit history" });
      const rendered = within(list).getAllByRole("option");
      expect(rendered[0]).toHaveAttribute("data-row-index", "480");
      expect(rendered.at(-1)).toHaveAttribute("data-row-index", "529");
      expect(screen.queryByText("Uncommitted changes")).toBeNull();
      expect(list.style.paddingTop).toBe(`${480 * ROW}px`);
    } finally {
      restore();
    }
  });

  it("scrolls a selected row far down the list into view", () => {
    const { pane, rerender, restore } = renderInPane();
    try {
      rerender(
        <CommitGraph
          {...historyProps}
          status={[]}
          commits={many}
          selectedRow={{ commitId: "id700" }}
          operationsBlocked={false}
          onSelectRow={vi.fn()}
          onBranchFromCommit={vi.fn()}
          onRebaseFromCommit={vi.fn()}
        />,
      );
      // Row 701 (after "Uncommitted changes") ends at the pane's bottom edge.
      expect(pane.scrollTop).toBe(702 * ROW - 260);
      expect(screen.getByText(/commit 700$/)).toBeInTheDocument();
    } finally {
      restore();
    }
  });

  it("loads the next page before the last loaded row comes into view", () => {
    const onLoadMore = vi.fn();
    const { scrollTo, restore } = renderInPane({ hasMore: true, onLoadMore });
    try {
      expect(onLoadMore).not.toHaveBeenCalled();
      scrollTo(ROW * 940);
      expect(onLoadMore).toHaveBeenCalled();
    } finally {
      restore();
    }
  });

  it("doesn't ask for another page while one is loading", () => {
    const onLoadMore = vi.fn();
    const { scrollTo, restore } = renderInPane({ hasMore: true, loadingMore: true, onLoadMore });
    try {
      scrollTo(ROW * 940);
      expect(onLoadMore).not.toHaveBeenCalled();
    } finally {
      restore();
    }
  });
});

// #687: an open search dims the rows that don't match, in the graph.
describe("CommitGraph — search matches", () => {
  function renderWithMatches(matches: ReadonlySet<string> | null, onStepMatch = vi.fn()) {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        operationsBlocked={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        searchMatches={matches}
        onStepMatch={onStepMatch}
      />,
    );
    return onStepMatch;
  }
  const row = (text: RegExp) => screen.getByText(text).closest("li");

  it("dims the rows that don't match and marks the matches", () => {
    renderWithMatches(new Set(["bbb222..."]));
    expect(row(/first commit/)).toHaveClass(styles.searchMatch);
    expect(row(/second commit/)).toHaveClass(styles.searchDimmed);
    expect(row(/Uncommitted changes/)).toHaveClass(styles.searchDimmed);
  });

  it("dims nothing without a search", () => {
    renderWithMatches(null);
    expect(row(/second commit/)).not.toHaveClass(styles.searchDimmed);
    expect(row(/second commit/)).not.toHaveClass(styles.searchMatch);
  });

  it("goes to the next and previous match with F3 and Shift+F3", () => {
    const onStepMatch = renderWithMatches(new Set(["bbb222..."]));
    const list = screen.getByRole("listbox", { name: "Commit history" });
    fireEvent.keyDown(list, { key: "F3" });
    fireEvent.keyDown(list, { key: "F3", shiftKey: true });
    expect(onStepMatch.mock.calls).toEqual([[1], [-1]]);
  });
});
