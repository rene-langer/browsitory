import { useState, type ComponentProps } from "react";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { GraphCommit, StatusEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { CommitGraph } from "./CommitGraph";
import styles from "./CommitGraph.module.css";

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
  squashMessage: null,
  commitHooks: [] as string[], sequence: null, rebase: null,
} as const;
const historyProps = {
  headInfo: NO_HEAD,
  currentBranch: "main",
  onCherryPick: vi.fn(),
  onRevert: vi.fn(),
  onCheckoutCommit: vi.fn(),
  onResetToCommit: vi.fn(),
  onEditHeadMessage: vi.fn(),
};

describe("CommitGraph", () => {
  it("exposes the row list as a labeled listbox, matching ListRow's option rows", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByRole("listbox", { name: "Commit history" })).toBeInTheDocument();
  });

  it("renders the Uncommitted Changes row with a change-count badge", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByText("Uncommitted Changes (2)")).toBeInTheDocument();
  });

  it("renders each commit's short id and summary", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
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
        pending={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByText(/second commit/).closest("li")!);

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "aaa111..." });
  });

  it("ArrowDown moves from Uncommitted Changes to the first commit", () => {
    const onSelectRow = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "ArrowDown" });

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "aaa111..." });
  });

  it("ArrowUp from the first row does nothing (clamped, not wrapped)", () => {
    const onSelectRow = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "ArrowUp" });

    expect(onSelectRow).toHaveBeenCalledWith("uncommitted");
  });

  it("ArrowDown from the last commit does nothing (clamped)", () => {
    const onSelectRow = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow={{ commitId: "bbb222..." }}
        pending={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "ArrowDown" });

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "bbb222..." });
  });

  it("right-clicking a commit row shows a 'Branch from here' menu entry", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/second commit/).closest("li")!);

    expect(screen.getByText("Branch from here")).toBeInTheDocument();
  });

  it("clicking 'Branch from here' calls onBranchFromCommit with that commit's id and closes the menu", () => {
    const onBranchFromCommit = vi.fn();
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
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

  it("right-clicking the Uncommitted Changes row does not show the menu", () => {
    render(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={commits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    fireEvent.contextMenu(screen.getByText(/Uncommitted Changes/).closest("li")!);

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
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
      />,
    );

    expect(screen.getByText("main")).toBeInTheDocument();
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
        pending={false}
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
        pending={false}
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
        pending={false}
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
        pending={false}
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
        pending={false}
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
        pending={false}
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
        pending={false}
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
        pending={false}
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
        pending={false}
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
        pending={false}
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
        pending={false}
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
    // After the rebase items, which keep their order.
    const labels = within(toolbar)
      .getAllByRole("button")
      .map((button) => button.textContent);
    expect(labels.slice(-2)).toEqual(["Cherry-pick 3 commits", "Revert 3 commits"]);
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
    renderChain({ pending: true, onCherryPickMany: vi.fn(), onRevertMany: vi.fn() });
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
    renderChain({ pending: true, onCompareCommits: vi.fn() });
    fireEvent.click(row("C"));
    fireEvent.click(row("B"), { shiftKey: true });
    expect(screen.getByRole("button", { name: "Compare these commits" })).toBeEnabled();
  });

  it("offers Diff against working tree on a single commit", () => {
    const onCompareWithWorkingTree = vi.fn();
    renderChain({ pending: true, onCompareWithWorkingTree });
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
    renderChain({ pending: true });

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
    fireEvent.click(screen.getByText(/Uncommitted Changes/).closest("li")!);

    expect(screen.queryByRole("toolbar", { name: "Selected commits" })).not.toBeInTheDocument();
    expect(row("D")).toHaveAttribute("aria-selected", "false");
    expect(row("B")).toHaveAttribute("aria-selected", "false");
  });

  it("an external focus change collapses the selection, and it does not resurface later", () => {
    const props = {
      ...historyProps,
      status,
      commits: chainCommits,
      pending: false,
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
      pending: false,
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
        pending={true}
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
      <CommitGraph {...historyProps} status={status} commits={chain} selectedRow={selectedRow} pending={false} {...handlers} />,
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
        pending={false}
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

  it("Enter on the Uncommitted Changes row opens no menu", () => {
    const { list } = setup("uncommitted");
    fireEvent.keyDown(list, { key: "Enter" });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("Home and End jump to the first and last row", () => {
    const { list, onSelectRow } = setup({ commitId: "B" });
    fireEvent.keyDown(list, { key: "Home" });
    expect(onSelectRow).toHaveBeenLastCalledWith("uncommitted");
    fireEvent.keyDown(list, { key: "End" });
    expect(onSelectRow).toHaveBeenLastCalledWith({ commitId: "A" });
  });

  it("PageDown and PageUp move by a page, clamped to the ends", () => {
    const { list, onSelectRow } = setup({ commitId: "B" });
    fireEvent.keyDown(list, { key: "PageDown" });
    expect(onSelectRow).toHaveBeenLastCalledWith({ commitId: "A" });
    fireEvent.keyDown(list, { key: "PageUp" });
    expect(onSelectRow).toHaveBeenLastCalledWith("uncommitted");
  });

  it("Shift+ArrowDown extends the selection and the menu then offers Squash", () => {
    const { rerender, list, onSelectRow, onRebaseSelection, ...rest } = setup({ commitId: "C" });
    fireEvent.click(screen.getByText(/^C /).closest("li")!);
    fireEvent.keyDown(list, { key: "ArrowDown", shiftKey: true });
    expect(onSelectRow).toHaveBeenLastCalledWith({ commitId: "B" });
    rerender(
      <CommitGraph
        {...historyProps}
        status={status}
        commits={chain}
        selectedRow={{ commitId: "B" }}
        pending={false}
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
    pending: false,
    onSelectRow: vi.fn(),
    onBranchFromCommit: vi.fn(),
    onRebaseFromCommit: vi.fn(),
  };

  it("shows each commit's author and date in the row", () => {
    render(<CommitGraph {...baseProps} />);

    expect(screen.getAllByText("Rene")).toHaveLength(2);
    expect(screen.getAllByTestId("commit-date")).toHaveLength(2);
  });

  it("offers Load more when more history exists and calls onLoadMore", () => {
    const onLoadMore = vi.fn();
    render(<CommitGraph {...baseProps} hasMore onLoadMore={onLoadMore} />);

    expect(screen.getByText(/Showing latest 2 commits/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Load more" }));

    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });

  it("does not offer Load more when the whole history is loaded", () => {
    render(<CommitGraph {...baseProps} hasMore={false} onLoadMore={vi.fn()} />);

    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
  });

  it("loads more when keyboard navigation reaches the last row", () => {
    const onLoadMore = vi.fn();
    render(
      <CommitGraph {...baseProps} selectedRow={{ commitId: "bbb222..." }} hasMore onLoadMore={onLoadMore} />,
    );

    fireEvent.keyDown(screen.getByRole("listbox", { name: "Commit history" }), { key: "ArrowDown" });

    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });

  it("draws a hollow working-tree node in the lane column of the Uncommitted Changes row", () => {
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
      onResetToCommit: vi.fn(),
      onEditHeadMessage: vi.fn(),
    };
    render(
      <CommitGraph
        status={status}
        commits={chain}
        selectedRow="uncommitted"
        pending={false}
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
    const { openMenu, onCherryPick, onRevert, onCheckoutCommit, onResetToCommit } = setup();
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick" }));
    expect(onCherryPick).toHaveBeenCalledWith("B", null);
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Revert" }));
    expect(onRevert).toHaveBeenCalledWith("B", null);
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Checkout (detached)" }));
    expect(onCheckoutCommit).toHaveBeenCalledWith("B");
    openMenu("B");
    fireEvent.click(screen.getByRole("menuitem", { name: "Reset main to here…" }));
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    expect(onResetToCommit).toHaveBeenCalledWith("B", "mixed");
  });

  it("disables cherry-pick on HEAD and offers Edit commit message there", () => {
    const { openMenu, onEditHeadMessage } = setup();
    openMenu("C");
    expect(screen.getByRole("menuitem", { name: "Cherry-pick" })).toBeDisabled();
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit commit message…" }));
    expect(onEditHeadMessage).toHaveBeenCalled();
    openMenu("M");
    expect(screen.queryByRole("menuitem", { name: "Edit commit message…" })).not.toBeInTheDocument();
  });

  it("asks for the parent of a merge commit and passes it as the mainline", () => {
    const { openMenu, onCherryPick, onRevert } = setup();
    openMenu("M");
    expect(screen.queryByRole("menuitem", { name: "Cherry-pick" })).toBeNull();
    fireEvent.click(screen.getByRole("menuitem", { name: "Cherry-pick…" }));
    const dialog = screen.getByRole("dialog", { name: "Choose parent" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cherry-pick" }));
    expect(onCherryPick).toHaveBeenCalledWith("M", 1);
    expect(screen.queryByRole("dialog", { name: "Choose parent" })).toBeNull();

    openMenu("M");
    fireEvent.click(screen.getByRole("menuitem", { name: "Revert…" }));
    const revertDialog = screen.getByRole("dialog", { name: "Choose parent" });
    fireEvent.click(within(revertDialog).getAllByRole("radio")[1]);
    fireEvent.click(within(revertDialog).getByRole("button", { name: "Revert" }));
    expect(onRevert).toHaveBeenCalledWith("M", 2);

    openMenu("M");
    fireEvent.click(screen.getByRole("menuitem", { name: "Revert…" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Choose parent" })).getByRole("button", { name: "Cancel" }));
    expect(onRevert).toHaveBeenCalledTimes(1);
  });

  it("returns focus to the commit list after a dialog opened from the menu closes, wherever focus was before", async () => {
    const { openMenu } = setup();
    const outside = document.createElement("button");
    outside.textContent = "outside";
    document.body.appendChild(outside);
    try {
      const list = screen.getByRole("listbox", { name: "Commit history" });
      for (const [summary, item, dialogName] of [
        ["B", "Reset main to here…", "Reset branch"],
        ["M", "Cherry-pick…", "Choose parent"],
      ] as const) {
        outside.focus();
        const outsideFocus = vi.spyOn(outside, "focus");
        openMenu(summary);
        fireEvent.click(screen.getByRole("menuitem", { name: item }));
        // Focus goes straight to the list, never back through the element focused before.
        expect(outsideFocus).not.toHaveBeenCalled();
        outsideFocus.mockRestore();
        const dialog = screen.getByRole("dialog", { name: new RegExp(dialogName) });
        fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
        await waitFor(() => expect(list).toHaveFocus());
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
    const { openMenu } = setup({ pending: true });
    openMenu("B");
    for (const name of ["Cherry-pick", "Revert", "Checkout (detached)", "Reset main to here…"]) {
      expect(screen.getByRole("menuitem", { name })).toBeDisabled();
    }
  });

  it("disables reset on a detached HEAD", () => {
    const { openMenu } = setup({ headInfo: { ...NO_HEAD, headId: "C", detachedAt: "C" }, currentBranch: null });
    openMenu("B");
    expect(screen.getByRole("menuitem", { name: "Reset HEAD to here…" })).toBeDisabled();
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
      onResetToCommit: vi.fn(),
      onEditHeadMessage: vi.fn(),
    };
    render(
      <CommitGraph
        status={[]}
        commits={tagged}
        selectedRow="uncommitted"
        pending={false}
        headInfo={{ headId: null, detachedAt: null, headPushed: false, operation: null, squashMessage: null, commitHooks: [], sequence: null, rebase: null }}
        currentBranch="main"
        {...handlers}
        {...overrides}
      />,
    );
    return handlers;
  }

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
    fireEvent.click(screen.getByRole("menuitem", { name: "Checkout (detached)" }));
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
});
