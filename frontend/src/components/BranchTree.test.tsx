import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, expectTypeOf, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { BranchTree } from "./BranchTree";
import { baseBranches, renderTree, treeProps } from "../test/branchTreeProps";
import { BranchTreeWithDialogs } from "../test/BranchTreeWithDialogs";
import type { RemoteDialogs } from "./RemoteDialogs";
import { describedByText } from "../test/describedBy";
import styles from "./BranchTree.module.css";

// The tree, its menus, inline rename and the remote dialogs. The forms, the upstream dialog and
// the lazy remote branch lists have their own test files, like their components (#492).

type IsOptional<T, K extends keyof T> = Pick<T, K> extends Required<Pick<T, K>> ? false : true;

const originOnly = [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null }];

describe("BranchTree — local branches", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("lists every local branch and marks the current one", () => {
    renderTree();
    // "main"'s row renders its name and " (current)" as a single run of text (see BranchTree.tsx),
    // so it is matched together via regex rather than as two independent exact-text queries.
    expect(screen.getByText(/^main.*\(current\)$/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "foo" })).toBeInTheDocument();
  });

  it("double-clicking a non-current branch switches to it", () => {
    const { props } = renderTree();
    fireEvent.doubleClick(screen.getByRole("button", { name: "foo" }));
    expect(props.onSwitchBranch).toHaveBeenCalledWith("feat/foo");
  });

  it("single-clicking a branch selects it and its tip in the graph without switching (#441)", () => {
    const onGoToBranch = vi.fn();
    const { props } = renderTree({ onGoToBranch });
    fireEvent.click(screen.getByRole("button", { name: "foo" }));
    expect(props.onSwitchBranch).not.toHaveBeenCalled();
    expect(onGoToBranch).toHaveBeenCalledWith("local", "feat/foo");

    // The swatch and the "…" keep their own meaning.
    fireEvent.click(screen.getByRole("button", { name: "Show feat/foo in graph" }));
    fireEvent.click(screen.getByRole("button", { name: "Actions for feat/foo" }));
    expect(onGoToBranch).toHaveBeenCalledOnce();
  });

  it("branches whose name contains a / are nested under a folder split on each segment", () => {
    renderTree();
    expect(screen.getByRole("button", { name: "feat" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "foo" })).toBeInTheDocument();
    expect(screen.queryByText("feat/foo")).not.toBeInTheDocument();
  });

  it("right-clicking a branch opens a context menu with Rename/Delete, and Merge for non-current branches", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    expect(screen.getByRole("menuitem", { name: "Rename" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Merge into current branch…" })).toBeInTheDocument();
  });

  it("does not offer Merge from the current branch's context menu", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    expect(screen.queryByRole("menuitem", { name: "Merge into current branch…" })).not.toBeInTheDocument();
  });

  it("Delete calls onDeleteBranch with force=false, then confirming Force Delete calls it again with force=true", async () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(props.onDeleteBranch).toHaveBeenCalledWith("feat/foo", false);
    fireEvent.click(await screen.findByRole("button", { name: "Force delete" }));
    expect(props.onDeleteBranch).toHaveBeenCalledWith("feat/foo", true);
  });

  // Regression test for a bug where the force-delete dialog fired after every *successful*
  // delete too: `handleDeleteClick` unconditionally arms `pendingForceFor` after the soft
  // delete, with no check for whether it actually failed. The app's real optimistic-update flow
  // removes a deleted branch from `branches` immediately on success (only rolling back on
  // failure), so a re-render with the branch gone from `branches` is what "the delete
  // succeeded" looks like from BranchTree's point of view.
  it("does not show the force-delete dialog after a successful delete (branch removed from branches)", async () => {
    const { props, rerender } = renderTree();

    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(props.onDeleteBranch).toHaveBeenCalledWith("feat/foo", false);
    await Promise.resolve();

    // Simulate the optimistic removal: re-render with the deleted branch gone from `branches`.
    rerender(<BranchTreeWithDialogs {...props} branches={baseBranches.filter((b) => b.name !== "feat/foo")} />);

    expect(screen.queryByRole("dialog", { name: "Force delete feat/foo" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Force delete" })).not.toBeInTheDocument();
  });

  it("Rename shows an inline input; Enter calls onRenameBranch", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const input = screen.getByDisplayValue("feat/foo");
    fireEvent.change(input, { target: { value: "feat/bar" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(props.onRenameBranch).toHaveBeenCalledWith("feat/foo", "feat/bar");
  });

  it("Merge into current branch calls onMergeBranch with that branch's name", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Merge into current branch…" }));
    expect(props.onMergeBranch).toHaveBeenCalledWith("feat/foo");
  });

  // #619
  it("Rebase current branch onto opens the planner with the branch's full ref name", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rebase current branch onto feat/foo…" }));
    expect(props.onRebaseOnto).toHaveBeenCalledWith("refs/heads/feat/foo");
  });

  it("offers no rebase onto the current branch, nor on a detached HEAD", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByText(/^main.*\(current\)$/));
    expect(screen.queryByRole("menuitem", { name: /^Rebase current branch onto/ })).toBeNull();
  });

  it("offers no rebase on a detached HEAD", () => {
    renderTree({ branches: [{ name: "main", isCurrent: false }, { name: "feat/foo", isCurrent: false }] });
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    expect(screen.getByRole("menuitem", { name: "Merge into current branch…" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /^Rebase current branch onto/ })).toBeNull();
  });

  it("disables Rebase current branch onto with the busy reason", () => {
    renderTree({ operationDisabled: true, operationDisabledReason: "A cherry-pick is in progress." });
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    const item = screen.getByRole("menuitem", { name: "Rebase current branch onto feat/foo…" });
    expect(item).toBeDisabled();
    expect(item).toHaveAttribute("title", "A cherry-pick is in progress.");
  });

  it("disables Rename/Delete/Merge menu items while a rebase is in progress", () => {
    renderTree({ operationDisabled: true, operationDisabledReason: "A rebase is in progress." });
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    expect(screen.getByRole("menuitem", { name: "Rename" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toHaveAttribute("title", "A rebase is in progress.");
    expect(screen.getByRole("menuitem", { name: "Merge into current branch…" })).toBeDisabled();
  });

  it("disables Delete with the reason whenever the repository is busy, not just while rebasing (#521)", () => {
    renderTree({ operationDisabled: true, operationDisabledReason: "A merge is in progress." });
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    const item = screen.getByRole("menuitem", { name: "Delete" });
    expect(item).toBeDisabled();
    expect(item).toHaveAttribute("title", "A merge is in progress.");
  });

  // #509: the palette's rule (`branchActions`); before, only a rebase run here disabled them.
  it("disables Switch to and Rename, and ignores a double-click, during a merge, like the palette", () => {
    const { props } = renderTree({ operationDisabled: true, operationDisabledReason: "A merge is in progress." });
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    for (const name of ["Switch to", "Rename"]) {
      const item = screen.getByRole("menuitem", { name });
      expect(item).toBeDisabled();
      expect(item).toHaveAttribute("title", "A merge is in progress.");
    }
    fireEvent.doubleClick(screen.getByRole("button", { name: "foo" }));
    expect(props.onSwitchBranch).not.toHaveBeenCalled();
  });

  it("blocks an open force-delete confirmation, saying why, once the repository is busy (#521)", async () => {
    const { props, rerender } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    await screen.findByRole("dialog", { name: "Force delete feat/foo" });

    rerender(<BranchTreeWithDialogs {...props} operationDisabled operationDisabledReason="A transfer is in progress." />);
    const dialog = screen.getByRole("dialog", { name: "Force delete feat/foo" });
    expect(dialog).toHaveTextContent("A transfer is in progress.");
    const confirm = within(dialog).getByRole("button", { name: "Force delete" });
    expect(confirm).toBeDisabled();
    expect(confirm).toHaveAttribute("title", "A transfer is in progress.");
    expect(describedByText(confirm)).toBe("A transfer is in progress.");
    fireEvent.click(confirm);
    expect(props.onDeleteBranch).not.toHaveBeenCalledWith("feat/foo", true);
  });

  // #673: the swatch takes the colour of the branch tip's lane in the graph.
  it("colours a swatch like its branch's lane, and a branch without a loaded tip neutral", () => {
    renderTree({ branchColors: { local: new Map([["main", "var(--color-lane-2)"]]), remote: new Map() } });
    const main = screen.getByRole("button", { name: "Show main in graph" });
    expect(main.style.getPropertyValue("--swatch")).toBe("var(--color-lane-2)");
    expect(main).not.toHaveClass(styles.swatchNoLane);
    const feature = screen.getByRole("button", { name: "Show feat/foo in graph" });
    expect(feature).toHaveClass(styles.swatchNoLane);
    expect(feature.style.getPropertyValue("--swatch")).toBe("var(--color-text-muted)");
    expect(feature).toHaveAttribute("title", expect.stringContaining("isn't in the loaded history"));
  });

  it("with no saved graph selection, every branch's graph swatch is pressed (shown) by default", () => {
    renderTree();
    expect(screen.getByRole("button", { name: "Show main in graph" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Show feat/foo in graph" })).toHaveAttribute("aria-pressed", "true");
  });

  it("toggling off a branch's graph swatch while showing all calls onSetGraphBranchSelection with every other branch", () => {
    const { props } = renderTree();
    fireEvent.click(screen.getByRole("button", { name: "Show feat/foo in graph" }));
    expect(props.onSetGraphBranchSelection).toHaveBeenCalledWith(["main"]);
  });

  it("the header '+' menu's New Branch opens the create-branch draft with startPoint HEAD", () => {
    const { props } = renderTree();
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "New branch…" }));
    expect(props.onOpenCreateBranchDraft).toHaveBeenCalledWith("HEAD");
  });

  it("a local branch's context menu lists Switch to first and calls onSwitchBranch", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    const items = screen.getAllByRole("menuitem");
    expect(items[0]).toHaveAccessibleName("Switch to");
    fireEvent.click(items[0]);
    expect(props.onSwitchBranch).toHaveBeenCalledWith("feat/foo");
  });

  it("Compare with current branch is offered on other local branches only", () => {
    const onCompareBranch = vi.fn();
    renderTree({ onCompareBranch });
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Compare with current branch" }));
    expect(onCompareBranch).toHaveBeenCalledWith("refs/heads/feat/foo", "feat/foo");

    fireEvent.contextMenu(screen.getByRole("button", { name: /main \(current\)/ }));
    expect(screen.queryByRole("menuitem", { name: "Compare with current branch" })).toBeNull();
  });

  it("says Compare with HEAD when no branch is checked out", () => {
    renderTree({
      branches: [
        { name: "main", isCurrent: false },
        { name: "feat/foo", isCurrent: false },
      ],
      onCompareBranch: vi.fn(),
    });
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    expect(screen.getByRole("menuitem", { name: "Compare with HEAD" })).toBeInTheDocument();
  });

  it("Switch to is disabled for the current branch", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: /main \(current\)/ }));
    expect(screen.getByRole("menuitem", { name: "Switch to" })).toBeDisabled();
  });

  it("a branch's context menu Isolate branch isolates only that local branch", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Isolate branch" }));
    expect(props.onIsolateGraphBranch).toHaveBeenCalledWith("local", "feat/foo");
    expect(props.onSetGraphBranchSelection).not.toHaveBeenCalled();
  });

  it("the header '+' menu's Show all branches is disabled with no saved selection, and restores the default graph when a filter is active", () => {
    const { props, rerender } = renderTree();
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(screen.getByRole("menuitem", { name: "Show all branches" })).toBeDisabled();

    // The open menu follows the new props.
    rerender(<BranchTreeWithDialogs {...props} graphBranchSelection={["feat/foo"]} />);
    fireEvent.click(screen.getByRole("menuitem", { name: "Show all branches" }));
    // #402: forgets the selection rather than saving today's branch names.
    expect(props.onShowAllGraphBranches).toHaveBeenCalledTimes(1);
    expect(props.onSetGraphBranchSelection).not.toHaveBeenCalled();
  });

  it("Show all branches is enabled when only a remote selection is saved", () => {
    const { props } = renderTree({ graphRemoteBranchSelection: [] });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Show all branches" }));
    expect(props.onShowAllGraphBranches).toHaveBeenCalledTimes(1);
  });

  it("a second click on a menu button closes its menu", () => {
    renderTree();
    const add = screen.getByRole("button", { name: "Add" });
    fireEvent.click(add);
    expect(screen.getByRole("menu")).toBeInTheDocument();

    fireEvent.mouseDown(add);
    fireEvent.click(add);

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(add).toHaveAttribute("aria-expanded", "false");
    expect(add).toHaveFocus();
  });

  it("a click on a branch's '…' closes the menu that Shift+F10 on its row opened", () => {
    renderTree();
    const actions = screen.getByRole("button", { name: "Actions for main" });
    const row = actions.closest("li")!;
    fireEvent.keyDown(row, { key: "F10", shiftKey: true });
    expect(screen.getByRole("menu")).toBeInTheDocument();

    fireEvent.mouseDown(actions);
    fireEvent.click(actions);

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  // The graph checkboxes stay visible inline regardless of the popover/menu, so this transfers
  // unchanged too.
  it("a branch absent from an explicit selection renders its swatch unpressed; toggling it adds it back", () => {
    const { props } = renderTree({
      branches: [
        { name: "main", isCurrent: true },
        { name: "feat/foo", isCurrent: false },
        { name: "dev", isCurrent: false },
      ],
      graphBranchSelection: ["main"],
    });

    expect(screen.getByRole("button", { name: "Show main in graph" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Show feat/foo in graph" })).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(screen.getByRole("button", { name: "Show feat/foo in graph" }));

    expect(props.onSetGraphBranchSelection).toHaveBeenCalledWith(["main", "feat/foo"]);
    expect(props.onClearGraphBranchSelection).not.toHaveBeenCalled();
  });

  // #402: a saved list of every branch would hide later branches, and with one branch walk
  // first parents only, so turning the last one back on forgets the selection instead.
  it("toggling on the last branch missing from the selection clears it instead of saving every name", () => {
    const { props } = renderTree({ graphBranchSelection: ["main"] });

    fireEvent.click(screen.getByRole("button", { name: "Show feat/foo in graph" }));

    expect(props.onClearGraphBranchSelection).toHaveBeenCalledTimes(1);
    expect(props.onSetGraphBranchSelection).not.toHaveBeenCalled();
  });

  it("in a repo with one branch, its dot off saves an empty selection and back on clears it", () => {
    const { props, rerender } = renderTree({ branches: [{ name: "main", isCurrent: true }] });

    fireEvent.click(screen.getByRole("button", { name: "Show main in graph" }));
    expect(props.onSetGraphBranchSelection).toHaveBeenCalledWith([]);

    rerender(<BranchTreeWithDialogs {...props} graphBranchSelection={[]} />);
    fireEvent.click(screen.getByRole("button", { name: "Show main in graph" }));
    expect(props.onClearGraphBranchSelection).toHaveBeenCalledTimes(1);
    expect(props.onSetGraphBranchSelection).toHaveBeenCalledTimes(1);
  });

  // Transformation: open-popover-then-click-Delete-twice → right-click-then-click-Delete-menuitem.
  it("Cancel in the force-delete dialog dismisses it without deleting, leaving Delete in place", async () => {
    const { props } = renderTree();

    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    await Promise.resolve();

    const dialog = await screen.findByRole("dialog", { name: "Force delete feat/foo" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(props.onDeleteBranch).toHaveBeenCalledTimes(1);
    expect(props.onDeleteBranch).not.toHaveBeenCalledWith("feat/foo", true);
    expect(screen.queryByRole("dialog", { name: "Force delete feat/foo" })).not.toBeInTheDocument();

    // "leaving Delete in place": right-clicking again still offers Delete, not a stale
    // force-delete state.
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument();
  });

  it("Enter on an empty/whitespace-only rename value does not call onRenameBranch", () => {
    const { props } = renderTree();

    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const input = screen.getByDisplayValue("feat/foo");
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(props.onRenameBranch).not.toHaveBeenCalled();
  });

  // #418: the inline rename follows the New branch form's rules.
  it("trims the new branch name before renaming", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const input = screen.getByDisplayValue("feat/foo");
    fireEvent.change(input, { target: { value: "  feat/bar " } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(props.onRenameBranch).toHaveBeenCalledWith("feat/foo", "feat/bar");
  });

  it("Enter on an unchanged name (spaces aside) closes the input without renaming", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const input = screen.getByDisplayValue("feat/foo");
    fireEvent.keyDown(input, { key: "Enter" });
    expect(screen.queryByRole("textbox", { name: "Rename feat/foo" })).not.toBeInTheDocument();

    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const again = screen.getByDisplayValue("feat/foo");
    fireEvent.change(again, { target: { value: "feat/foo  " } });
    fireEvent.keyDown(again, { key: "Enter" });
    expect(screen.queryByRole("textbox", { name: "Rename feat/foo" })).not.toBeInTheDocument();
    expect(props.onRenameBranch).not.toHaveBeenCalled();
  });

  it("shows the New branch form's message for an invalid name and keeps the input open on Enter", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const input = screen.getByDisplayValue("feat/foo");
    fireEvent.change(input, { target: { value: "feat bar" } });

    expect(screen.getByText("Branch names can't contain spaces.")).toBeInTheDocument();
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input).toHaveAccessibleDescription("Branch names can't contain spaces.");
    fireEvent.keyDown(input, { key: "Enter" });
    expect(props.onRenameBranch).not.toHaveBeenCalled();
    expect(screen.getByRole("textbox", { name: "Rename feat/foo" })).toBeInTheDocument();

    fireEvent.change(input, { target: { value: "feat/bar" } });
    expect(screen.queryByText("Branch names can't contain spaces.")).not.toBeInTheDocument();
  });

  it("disables the merge action while a merge is already in progress", () => {
    renderTree({ operationDisabled: true, operationDisabledReason: "A merge is in progress." });

    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));

    expect(screen.getByRole("menuitem", { name: "Merge into current branch…" })).toBeDisabled();
  });

  it("disables the merge action while another repository operation is pending", () => {
    renderTree({ operationDisabled: true });

    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));

    expect(screen.getByRole("menuitem", { name: "Merge into current branch…" })).toBeDisabled();
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("explains why the merge action is disabled via its title", () => {
    renderTree({ operationDisabled: true, operationDisabledReason: "A rebase is in progress." });

    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));

    expect(screen.getByRole("menuitem", { name: "Merge into current branch…" })).toHaveAttribute(
      "title",
      "A rebase is in progress.",
    );
  });

  it("does not switch branches while a rebase is in progress", () => {
    const { props } = renderTree({ operationDisabled: true, operationDisabledReason: "A rebase is in progress." });

    fireEvent.doubleClick(screen.getByRole("button", { name: "foo" }));

    expect(props.onSwitchBranch).not.toHaveBeenCalled();
  });

  // Replaces BranchSwitcher.test.tsx's "closing the popover clears a pending force-delete..." and
  // "...clears an in-progress rename..." — there is no popover to close in this interaction model.
  // The equivalent guard here is that starting a fresh interaction on another branch clears any
  // stale per-branch edit state rather than leaving it attached to the wrong row.
  it("right-clicking a different branch after starting a rename on one clears that rename's input", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    expect(screen.getByDisplayValue("feat/foo")).toBeInTheDocument();
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    expect(screen.queryByDisplayValue("feat/foo")).not.toBeInTheDocument();
  });
});

describe("BranchTree — remotes", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  const oneRemote = [{ name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null }];

  // #509: the palette's rule (`remoteActions`); before, Edit remote and Manage credentials stayed
  // enabled and Remove remote gave no reason.
  it("disables every remote action with the reason while the repository is busy, like the palette", () => {
    renderTree({ remotes: oneRemote, operationDisabled: true, operationDisabledReason: "A transfer is in progress." });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    for (const name of ["Fetch", "Push current branch here", "Edit remote", "Manage credentials", "Remove remote"]) {
      const item = screen.getByRole("menuitem", { name });
      expect(item).toBeDisabled();
      expect(item).toHaveAttribute("title", "A transfer is in progress.");
    }
  });

  // #494: the palette's "Browse branches on origin" asks through state; before, it clicked the
  // section and folder headers it found by their text.
  describe("reveal requests (#494)", () => {
    const originalScrollIntoView = Element.prototype.scrollIntoView;
    afterEach(() => {
      Element.prototype.scrollIntoView = originalScrollIntoView;
    });

    it("expands a collapsed section and the remote's folder, loads its branches and scrolls to it", async () => {
      localStorage.setItem("sidebar-branches", "closed");
      const scrollIntoView = vi.fn();
      Element.prototype.scrollIntoView = scrollIntoView;
      const onListRemoteBranches = vi.fn().mockResolvedValue(["develop"]);
      const props = treeProps({ remotes: oneRemote, onListRemoteBranches });
      const { rerender } = render(<BranchTree {...props} />);
      expect(screen.queryByRole("button", { name: "origin" })).not.toBeInTheDocument();

      rerender(<BranchTree {...props} revealRequest={{ nonce: 1, remote: "origin" }} />);
      const folder = screen.getByRole("button", { name: "origin" });
      expect(folder).toHaveAttribute("aria-expanded", "true");
      expect(screen.getByRole("button", { name: "Branches" })).toHaveAttribute("aria-expanded", "true");
      expect(onListRemoteBranches).toHaveBeenCalledWith("origin");
      expect(localStorage.getItem("sidebar-branches")).toBe("open");
      expect(scrollIntoView.mock.contexts.at(-1)).toBe(folder);
      expect(await within(folder.closest("li")!).findByText("develop")).toBeInTheDocument();
    });

    it("keeps an open folder open and reveals it again on the next request", () => {
      const scrollIntoView = vi.fn();
      Element.prototype.scrollIntoView = scrollIntoView;
      const props = treeProps({ remotes: oneRemote });
      const { rerender } = render(<BranchTree {...props} revealRequest={{ nonce: 1 }} />);
      fireEvent.click(screen.getByRole("button", { name: "origin" }));
      rerender(<BranchTree {...props} revealRequest={{ nonce: 2, remote: "origin" }} />);
      expect(screen.getByRole("button", { name: "origin" })).toHaveAttribute("aria-expanded", "true");
      expect(scrollIntoView).toHaveBeenCalledTimes(2); // the section, then the folder
    });
  });

  // With no saved remote selection, every remote branch present in the commit graph
  // (`remoteBranchesInGraph`, NOT the lazy per-remote-folder fetch cache) has its swatch pressed
  // (shown) by default — mirroring the local-branch swatch's default (see "with no saved graph
  // selection, every branch's graph swatch is pressed (shown) by default" above). So toggling off
  // origin/main here removes it from that set rather than adding it; origin/develop stays, which
  // also proves the qualified `${remoteName}/${branchName}` naming is computed correctly.
  it("single-clicking a remote branch selects its tip in the graph (#441)", async () => {
    const onGoToBranch = vi.fn();
    renderTree({ remotes: oneRemote, onGoToBranch, onListRemoteBranches: vi.fn().mockResolvedValue(["develop"]) });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(await screen.findByText("develop"));
    expect(onGoToBranch).toHaveBeenCalledWith("remote", "origin/develop");
  });

  it("toggles a remote branch's graph visibility via its swatch button", async () => {
    const onSetGraphRemoteBranchSelection = vi.fn();
    renderTree({
      remotes: oneRemote,
      graphRemoteBranchSelection: null,
      onSetGraphRemoteBranchSelection,
      remoteBranchesInGraph: ["origin/main", "origin/develop"],
      onListRemoteBranches: vi.fn().mockResolvedValue(["main", "develop"]),
    });

    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    await screen.findByText("main");

    expect(screen.getByRole("button", { name: "Show origin/main in graph" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    fireEvent.click(screen.getByRole("button", { name: "Show origin/main in graph" }));

    expect(onSetGraphRemoteBranchSelection).toHaveBeenCalledWith(["origin/develop"]);
  });

  // AUD-2026-09-24: the null-selection fallback used to derive from `remoteBranches` — this
  // component's own lazy per-remote-folder fetch cache, only populated for remotes whose folder
  // has been expanded — instead of `remoteBranchesInGraph`. With two remotes and only one's
  // folder ever expanded, the first toggle used to silently drop the never-expanded remote's
  // branches from the saved selection (they were never in the "known" set to begin with).
  it("does not drop badges for a remote whose folder was never expanded (null-selection fallback)", async () => {
    const onSetGraphRemoteBranchSelection = vi.fn();
    const remotes = [
      { name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null },
      { name: "upstream", fetchUrl: "git@github.com:user/upstream.git", pushUrl: null, authMode: null, authUsername: null },
    ];
    renderTree({
      remotes,
      graphRemoteBranchSelection: null,
      onSetGraphRemoteBranchSelection,
      remoteBranchesInGraph: ["origin/main", "upstream/feature"],
      onListRemoteBranches: vi.fn().mockResolvedValue(["main"]),
    });

    // Expand only "origin" — "upstream"'s folder is never touched, so `remoteBranches` (the lazy
    // fetch cache) never gets an entry for it.
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    await screen.findByText("main");

    expect(screen.getByRole("button", { name: "Show origin/main in graph" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    // Toggling off origin/main must save a selection that still includes upstream/feature: if the
    // fallback wrongly derived from the lazy fetch cache instead of `remoteBranchesInGraph`,
    // upstream/feature would silently vanish here even though its folder was never expanded.
    fireEvent.click(screen.getByRole("button", { name: "Show origin/main in graph" }));
    expect(onSetGraphRemoteBranchSelection).toHaveBeenCalledWith(["upstream/feature"]);
  });

  it("right-clicking a remote branch offers Switch to and Set as upstream", async () => {
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["feat/foo"]) });
    const remoteFolder = screen.getByRole("button", { name: "origin" }).closest("li")!;
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    // Scoped to the remote's own <li> — `baseBranches` already has a local "feat/foo".
    fireEvent.contextMenu(await within(remoteFolder).findByText("foo"));
    expect(screen.getByRole("menuitem", { name: "Switch to" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Set as upstream for current branch" })).toBeInTheDocument();
  });

  it("double-clicking a remote branch checks it out the same way the context menu's Switch to does", async () => {
    const onSwitchBranch = vi.fn();
    renderTree({
      branches: [...baseBranches],
      remotes: oneRemote,
      onListRemoteBranches: vi.fn().mockResolvedValue(["feat/foo"]),
      onSwitchBranch,
    });
    const remoteFolder = screen.getByRole("button", { name: "origin" }).closest("li")!;
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.doubleClick(await within(remoteFolder).findByText("foo"));
    expect(onSwitchBranch).toHaveBeenCalledWith("feat/foo");
  });

  // AUD-2026-09-05-FE-001: a remote-tracking branch row used to render as a bare `<span>`, which
  // is never a Tab stop — its context menu was mouse-only. It's now a `role="button"` row (via
  // `ListRow`), so it must land in the tab order.
  it("a remote-tracking branch row is a real focusable button, reachable in the tab order", async () => {
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["feat/foo"]) });
    const remoteFolder = screen.getByRole("button", { name: "origin" }).closest("li")!;
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const branchLabel = await within(remoteFolder).findByText("foo");
    const branchRow = branchLabel.closest('[role="button"]') as HTMLElement;
    expect(branchRow).not.toBeNull();
    expect(branchRow.tabIndex).not.toBe(-1);
    branchRow.focus();
    expect(branchRow).toHaveFocus();
  });

  // The row's context menu is otherwise only reachable via a native contextmenu event
  // (right-click, or Shift+F10/Menu-key). This "…" button is the visible, discoverable affordance
  // for the same actions, and a plain click on it must open the identical menu.
  it("a remote-tracking branch row exposes an explicit '…' actions button that opens its context menu", async () => {
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["feat/foo"]) });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const actionsButton = await screen.findByRole("button", { name: "Actions for origin/feat/foo" });
    expect(actionsButton).toHaveAttribute("aria-haspopup", "menu");
    fireEvent.click(actionsButton);
    expect(screen.getByRole("menuitem", { name: "Switch to" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Set as upstream for current branch" })).toBeInTheDocument();
  });

  it("names a remote branch's actions button with its remote, distinct from a local branch of the same name", async () => {
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["feat/foo"]) });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(await screen.findByRole("button", { name: "Actions for origin/feat/foo" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Actions for feat/foo" })).toBeInTheDocument();
  });

  it("Switch to switches to an existing same-named local branch instead of creating a new one", async () => {
    const onSwitchBranch = vi.fn();
    const onCreateBranch = vi.fn();
    renderTree({
      branches: [...baseBranches],
      remotes: oneRemote,
      onListRemoteBranches: vi.fn().mockResolvedValue(["feat/foo"]),
      onSwitchBranch,
      onCreateBranch,
    });
    const remoteFolder = screen.getByRole("button", { name: "origin" }).closest("li")!;
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    // Scoped to the remote's own <li> — `baseBranches` already has a local "feat/foo".
    fireEvent.contextMenu(await within(remoteFolder).findByText("foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Switch to" }));
    expect(onSwitchBranch).toHaveBeenCalledWith("feat/foo");
    expect(onCreateBranch).not.toHaveBeenCalled();
  });

  it("Switch to creates and tracks a new local branch when none exists locally", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue(null);
    const onSetUpstream = vi.fn().mockResolvedValue(undefined);
    renderTree({
      branches: [{ name: "main", isCurrent: true }],
      remotes: oneRemote,
      onListRemoteBranches: vi.fn().mockResolvedValue(["feat/only-remote"]),
      onCreateBranch,
      onSetUpstream,
    });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.contextMenu(await screen.findByText("only-remote"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Switch to" }));
    await Promise.resolve();
    expect(onCreateBranch).toHaveBeenCalledWith("feat/only-remote", "origin/feat/only-remote");
    expect(onSetUpstream).toHaveBeenCalledWith("origin", "feat/only-remote");
  });

  it("Compare with current branch on a remote-tracking branch passes its full ref", async () => {
    const onCompareBranch = vi.fn();
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["main"]), onCompareBranch });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.contextMenu(await screen.findByText("main"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Compare with current branch" }));
    expect(onCompareBranch).toHaveBeenCalledWith("refs/remotes/origin/main", "origin/main");
  });

  // #619
  it("Rebase current branch onto a remote-tracking branch passes its full ref", async () => {
    const onRebaseOnto = vi.fn();
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["main"]), onRebaseOnto });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.contextMenu(await screen.findByText("main"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rebase current branch onto origin/main…" }));
    expect(onRebaseOnto).toHaveBeenCalledWith("refs/remotes/origin/main");
  });

  it("Set as upstream for current branch calls onSetUpstream directly, no dialog", async () => {
    const onSetUpstream = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["main"]), onSetUpstream });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.contextMenu(await screen.findByText("main"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Set as upstream for current branch" }));
    expect(onSetUpstream).toHaveBeenCalledWith("origin", "main");
  });

  it("right-clicking a remote folder offers Fetch/Push/Edit/Credentials/Remove", () => {
    renderTree({ remotes: oneRemote });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    expect(screen.getByRole("menuitem", { name: "Fetch" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Push current branch here" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Edit remote" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Manage credentials" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Remove remote" })).toBeInTheDocument();
  });

  it("Fetch calls onFetchRemote with the remote's name", () => {
    const onFetchRemote = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onFetchRemote });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Fetch" }));
    expect(onFetchRemote).toHaveBeenCalledWith("origin");
  });

  it("Push current branch here calls onPushCurrentBranch with the remote's name", () => {
    const onPushCurrentBranch = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onPushCurrentBranch });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Push current branch here" }));
    expect(onPushCurrentBranch).toHaveBeenCalledWith("origin");
  });

  it("Edit remote opens a dialog prefilled with the remote's URLs; saving calls onUpdateRemoteUrls", async () => {
    const onUpdateRemoteUrls = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onUpdateRemoteUrls });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit remote" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit origin" });
    expect(within(dialog).getByDisplayValue("git@github.com:user/repo.git")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Remote name")).toHaveFocus();
    fireEvent.change(within(dialog).getByLabelText("Fetch URL"), { target: { value: "git@github.com:user/moved.git" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote" }));
    await waitFor(() =>
      expect(onUpdateRemoteUrls).toHaveBeenCalledWith("origin", "git@github.com:user/moved.git", null),
    );
  });

  it("Manage credentials opens a dialog; saving an HTTPS credential calls onSaveHttpsCredential", async () => {
    const onSetRemoteAuthMode = vi.fn().mockResolvedValue(true);
    const onSaveHttpsCredential = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onSetRemoteAuthMode, onSaveHttpsCredential });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Manage credentials" }));
    const dialog = await screen.findByRole("dialog", { name: "Credentials for origin" });
    fireEvent.change(within(dialog).getByLabelText("HTTPS username"), { target: { value: "me" } });
    fireEvent.change(within(dialog).getByLabelText("Access token"), { target: { value: "tok" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save HTTPS credential" }));
    expect(onSetRemoteAuthMode).toHaveBeenCalledWith("origin", "HttpsToken", "me");
    // `onSaveHttpsCredential` only fires after `onSetRemoteAuthMode`'s promise resolves, so it
    // needs a tick — unlike the synchronous call above.
    await waitFor(() => expect(onSaveHttpsCredential).toHaveBeenCalledWith("origin", "me", "tok"));
  });

  it("Remove remote opens the existing confirmation flow and calls onRemoveRemote", async () => {
    const onRemoveRemote = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onRemoveRemote });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Remove remote" }));
    fireEvent.click(await screen.findByRole("button", { name: "Confirm remove" }));
    expect(onRemoveRemote).toHaveBeenCalledWith("origin", false);
  });

  it("offers the explicit clear-upstreams removal route for a remote that has upstreams", async () => {
    renderTree({
      remotes: oneRemote,
      remoteUpstreams: { origin: [{ localBranch: "main", remoteName: "origin", remoteBranch: "main" }] },
    });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Remove remote" }));
    expect(await screen.findByText(/clear upstreams for main/)).toBeInTheDocument();
  });

  it("the header '+' menu's Add Remote opens the add-remote draft", () => {
    const onOpenAddRemoteDraft = vi.fn();
    renderTree({ onOpenAddRemoteDraft });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add remote…" }));
    expect(onOpenAddRemoteDraft).toHaveBeenCalledOnce();
  });

  it("shows the current branch's upstream status, with no Pull of its own (#637)", () => {
    renderTree({ upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" } });
    expect(screen.getByText(/main tracks origin\/main/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Pull" })).toBeNull();
  });

  it("does not offer Set upstream… or Push to <upstream remote> on a non-current branch's context menu", () => {
    renderTree({ remotes: oneRemote, upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" } });
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    expect(screen.queryByRole("menuitem", { name: "Set upstream…" })).not.toBeInTheDocument();
    // Only the dialog-based "Push to…" (any local branch, 4b); never the current branch's direct push.
    expect(screen.queryByRole("menuitem", { name: /^Push to (?!…$)/ })).not.toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Push to…" })).toBeInTheDocument();
  });

  it("offers Push to <remote> on the current branch's context menu when it has an upstream", () => {
    const onPushCurrentBranch = vi.fn().mockResolvedValue(undefined);
    renderTree({ upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" }, onPushCurrentBranch });
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    fireEvent.click(screen.getByRole("menuitem", { name: "Push to origin" }));
    expect(onPushCurrentBranch).toHaveBeenCalledWith("origin");
  });

  // ---- Ported from RemotePanel.test.tsx below, using the transformation demonstrated above:
  // right-click the remote folder (or remote-branch row) → click the equivalent menuitem → assert
  // against the resulting dialog/form. See task-8-report.md for the cases that don't map 1:1 and
  // why (single global credential/edit modals replace the old per-row inline forms, so a few
  // row-isolation and disclosure-default tests no longer have anything to attach to).

  it("disables Push current branch here while another operation is active", () => {
    renderTree({ remotes: oneRemote, operationDisabled: true });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    expect(screen.getByRole("menuitem", { name: "Push current branch here" })).toBeDisabled();
  });

  it("disables Fetch and the Add-remote submit button while another operation is active", () => {
    renderTree({ remotes: oneRemote, addRemoteDraftOpen: true, operationDisabled: true });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    expect(screen.getByRole("menuitem", { name: "Fetch" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add remote" })).toBeDisabled();
  });

  // A remote branch's Switch to can call `onSwitchBranch` (when a same-named local branch already
  // exists), which is exactly the hazard `isRebasing` already guards for local branches: switching
  // underneath a paused rebase silently retargets an unrelated branch once `rebase::finish` moves
  // the original branch ref (see BranchTree's local "does not switch branches while a rebase is in
  // progress" test and BranchSwitcher's original comment on the same hazard). `operationDisabled`
  // is the broader gate (rebase/merge/transfer/etc.), so both remote-branch actions must respect it
  // too, matching the sibling remote-folder items right above.
  it("disables Switch to and Set as upstream on a remote branch while another operation is active", async () => {
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["only-remote"]), operationDisabled: true });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.contextMenu(await screen.findByText("only-remote"));
    expect(screen.getByRole("menuitem", { name: "Switch to" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Set as upstream for current branch" })).toBeDisabled();
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("explains why Fetch/Push/Add remote are disabled via their title", () => {
    renderTree({
      remotes: oneRemote,
      addRemoteDraftOpen: true,
      operationDisabled: true,
      operationDisabledReason: "A rebase is in progress.",
    });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    expect(screen.getByRole("menuitem", { name: "Fetch" })).toHaveAttribute("title", "A rebase is in progress.");
    expect(screen.getByRole("menuitem", { name: "Push current branch here" })).toHaveAttribute(
      "title",
      "A rebase is in progress.",
    );
    expect(screen.getByRole("button", { name: "Add remote" })).toHaveAttribute("title", "A rebase is in progress.");
  });

  it("requires upstream discovery in both the client and RemoteDialogs contracts", () => {
    expectTypeOf<IsOptional<RepoClient, "getRemoteUpstreams">>().toEqualTypeOf<false>();
    expectTypeOf<IsOptional<Parameters<typeof RemoteDialogs>[0], "remoteUpstreams">>().toEqualTypeOf<false>();
  });

  it("shows all affected branches when clearing upstreams for a remote with multiple, and removes with clearUpstreams=true", async () => {
    const onRemoveRemote = vi.fn().mockResolvedValue(undefined);
    renderTree({
      remotes: oneRemote,
      remoteUpstreams: {
        origin: [
          { localBranch: "main", remoteName: "origin", remoteBranch: "main" },
          { localBranch: "topic", remoteName: "origin", remoteBranch: "topic" },
        ],
      },
      onRemoveRemote,
    });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Remove remote" }));
    expect(await screen.findByText(/clear upstreams for main, topic/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Confirm remove" }));
    expect(onRemoveRemote).toHaveBeenCalledWith("origin", true);
  });

  it("focuses Cancel when the remove-remote confirmation opens", async () => {
    renderTree({ remotes: oneRemote });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Remove remote" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove remote confirmation" });
    await waitFor(() => {
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    });
  });

  it("marks Remove remote as a destructive context-menu item", () => {
    renderTree({ remotes: oneRemote });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    expect(screen.getByRole("menuitem", { name: "Remove remote" })).toHaveAttribute("data-destructive", "true");
  });
});

describe("BranchTree — keys from inner controls", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("Enter and Space on the swatch do not switch branches", () => {
    const { props } = renderTree();
    const swatch = screen.getByRole("button", { name: "Show feat/foo in graph" });
    fireEvent.keyDown(swatch, { key: "Enter" });
    fireEvent.keyDown(swatch, { key: " " });
    expect(props.onSwitchBranch).not.toHaveBeenCalled();
  });

  it("Enter on a remote row's actions button does not check the branch out", async () => {
    const { props } = renderTree({
      remotes: [{ name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null }],
      onListRemoteBranches: vi.fn().mockResolvedValue(["feat/foo"]),
    });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const actions = await screen.findByRole("button", { name: "Actions for origin/feat/foo" });
    fireEvent.keyDown(actions, { key: "Enter" });
    expect(props.onSwitchBranch).not.toHaveBeenCalled();
  });
});

describe("BranchTree — row layout", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("a local row keeps its swatch and name in one container and titles the name with the full branch name", () => {
    renderTree();
    const swatch = screen.getByRole("button", { name: "Show feat/foo in graph" });
    const name = screen.getByTitle("feat/foo");
    expect(name).toHaveTextContent("foo");
    expect(swatch.parentElement).toBe(name.parentElement);
  });

  it("a remote row titles its name with the full branch name", async () => {
    renderTree({
      remotes: [{ name: "origin", fetchUrl: "u", pushUrl: null, authMode: null, authUsername: null }],
      onListRemoteBranches: vi.fn().mockResolvedValue(["feat/bar"]),
    });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(await screen.findByTitle("feat/bar")).toHaveTextContent("bar");
  });
});

describe("BranchTree — add button placement", () => {
  it("the Add button lives in the section header, not the scrolling body", () => {
    renderTree();
    const add = screen.getByRole("button", { name: "Add" });
    const heading = screen.getByRole("heading", { name: /Branches/ });
    expect(add.closest("section")?.firstElementChild).toContainElement(heading);
    expect(add.closest("section")?.firstElementChild).toContainElement(add);
  });
});

describe("BranchTree — local row actions button", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("a local row's '...' button opens the branch context menu", () => {
    renderTree();
    const actions = screen.getByRole("button", { name: "Actions for feat/foo" });
    expect(actions).toHaveAttribute("aria-haspopup", "menu");
    fireEvent.click(actions);
    expect(screen.getByRole("menuitem", { name: "Rename" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument();
  });

  it("the current branch is marked with a bold weight and a marker", () => {
    renderTree();
    const marker = screen.getByLabelText("current branch");
    expect(marker.closest("span[title='main']")).not.toBeNull();
  });
});

describe("BranchTree — inline forms focus and Escape", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  function startRename() {
    const utils = renderTree();
    fireEvent.click(screen.getByRole("button", { name: "Actions for feat/foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    return utils;
  }

  it("the rename input is labelled, focused, and Escape cancels", () => {
    startRename();
    const input = screen.getByRole("textbox", { name: "Rename feat/foo" });
    expect(input).toHaveFocus();
    fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryByRole("textbox", { name: "Rename feat/foo" })).not.toBeInTheDocument();
  });

  it("blurring the rename input with an unchanged value cancels, a changed value stays", () => {
    startRename();
    const input = screen.getByRole("textbox", { name: "Rename feat/foo" });
    fireEvent.change(input, { target: { value: "feat/bar" } });
    fireEvent.blur(input);
    expect(screen.getByRole("textbox", { name: "Rename feat/foo" })).toBeInTheDocument();
    fireEvent.change(input, { target: { value: "feat/foo" } });
    fireEvent.blur(input);
    expect(screen.queryByRole("textbox", { name: "Rename feat/foo" })).not.toBeInTheDocument();
  });
});

describe("BranchTree — remote list states and upstream block", () => {
  beforeEach(() => {
    localStorage.clear();
  });
  const up = { localBranch: "main", remoteName: "origin", remoteBranch: "main" };

  it("asks for confirmation before clearing the upstream", async () => {
    const { props } = renderTree({ upstream: up });
    fireEvent.click(screen.getByRole("button", { name: "Clear upstream" }));
    expect(props.onClearUpstream).not.toHaveBeenCalled();
    const dialog = await screen.findByRole("dialog", { name: "Clear upstream confirmation", hidden: true });
    fireEvent.click(within(dialog).getByRole("button", { name: "Clear upstream", hidden: true }));
    expect(props.onClearUpstream).toHaveBeenCalledOnce();
  });

  it("says why Clear upstream can't run while the repository is busy, there and in its confirmation (#748)", async () => {
    const { props, rerender } = renderTree({ upstream: up });
    fireEvent.click(screen.getByRole("button", { name: "Clear upstream" }));
    await screen.findByRole("dialog", { name: "Clear upstream confirmation", hidden: true });
    rerender(<BranchTreeWithDialogs {...props} operationDisabled operationDisabledReason="A merge is in progress." />);
    const dialog = screen.getByRole("dialog", { name: "Clear upstream confirmation", hidden: true });
    const confirm = within(dialog).getByRole("button", { name: "Clear upstream", hidden: true });
    expect(confirm).toBeDisabled();
    expect(describedByText(confirm)).toBe("A merge is in progress.");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel", hidden: true }));

    const button = screen.getByRole("button", { name: "Clear upstream" });
    expect(button).toHaveAttribute("aria-disabled", "true");
    expect(describedByText(button)).toBe("A merge is in progress.");
    fireEvent.click(button);
    expect(screen.queryByRole("dialog", { name: "Clear upstream confirmation", hidden: true })).toBeNull();
    expect(props.onClearUpstream).not.toHaveBeenCalled();
  });
});

describe("BranchTree — refs and remotes (4b)", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("shows only the non-zero ahead/behind parts with an accessible label", () => {
    renderTree({
      branches: [
        { name: "main", isCurrent: true, upstream: "origin/main", ahead: 2, behind: 0 },
        { name: "behind", isCurrent: false, upstream: "origin/behind", ahead: 0, behind: 3 },
        { name: "synced", isCurrent: false, upstream: "origin/synced", ahead: 0, behind: 0 },
        { name: "never-fetched", isCurrent: false, upstream: "origin/x", ahead: null, behind: null },
        { name: "untracked", isCurrent: false },
      ],
    });
    // role="img" so the label is exposed to screen readers (a bare span's aria-label is not).
    const counts = screen.getByRole("img", { name: "2 ahead, 0 behind origin/main" });
    expect(counts).toHaveTextContent("↑2");
    expect(counts).not.toHaveTextContent("↓");
    expect(counts).toHaveAttribute("title", "2 ahead, 0 behind origin/main");
    const behind = screen.getByRole("img", { name: "0 ahead, 3 behind origin/behind" });
    expect(behind).toHaveTextContent("↓3");
    expect(behind).not.toHaveTextContent("↑");
    expect(screen.queryByLabelText(/behind origin\/synced/)).toBeNull();
    expect(screen.queryByLabelText(/behind origin\/x/)).toBeNull();
  });

  it("offers Push to… on a non-current local branch", () => {
    const { props } = renderTree({ remotes: originOnly });
    fireEvent.click(screen.getByRole("button", { name: "Actions for feat/foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Push to…" }));
    expect(props.onOpenPushDialog).toHaveBeenCalledWith("feat/foo");
  });

  it("offers Push to… on the current branch too", () => {
    const { props } = renderTree({ remotes: originOnly });
    fireEvent.click(screen.getByRole("button", { name: "Actions for main" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Push to…" }));
    expect(props.onOpenPushDialog).toHaveBeenCalledWith("main");
  });

  it("disables Push to… without a remote", () => {
    renderTree({ remotes: [] });
    fireEvent.click(screen.getByRole("button", { name: "Actions for feat/foo" }));
    const item = screen.getByRole("menuitem", { name: "Push to…" });
    expect(item).toBeDisabled();
    expect(item).toHaveAttribute("title", "No remote configured.");
  });

  it("offers a destructive Delete <remote>/<branch>… on remote branches", async () => {
    const { props } = renderTree({
      remotes: originOnly,
      onListRemoteBranches: vi.fn().mockResolvedValue(["topic"]),
    });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const row = (await screen.findByRole("button", { name: "Show origin/topic in graph" })).closest("li")!;
    fireEvent.contextMenu(row);
    const item = screen.getByRole("menuitem", { name: "Delete origin/topic…" });
    expect(item).toHaveAttribute("data-destructive", "true");
    fireEvent.click(item);
    expect(props.onOpenDeleteRemoteBranch).toHaveBeenCalledWith("origin", "topic");
  });

  it("a remote branch's context menu Isolate branch isolates that remote branch (#385)", async () => {
    const { props } = renderTree({
      remotes: originOnly,
      onListRemoteBranches: vi.fn().mockResolvedValue(["topic"]),
    });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const row = (await screen.findByRole("button", { name: "Show origin/topic in graph" })).closest("li")!;
    fireEvent.contextMenu(row);
    fireEvent.click(screen.getByRole("menuitem", { name: "Isolate branch" }));
    expect(props.onIsolateGraphBranch).toHaveBeenCalledWith("remote", "origin/topic");
  });

  it("disables Delete <remote>/<branch>… while another operation runs", async () => {
    renderTree({
      remotes: originOnly,
      operationDisabled: true,
      operationDisabledReason: "A transfer is in progress.",
      onListRemoteBranches: vi.fn().mockResolvedValue(["topic"]),
    });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const row = (await screen.findByRole("button", { name: "Show origin/topic in graph" })).closest("li")!;
    fireEvent.contextMenu(row);
    expect(screen.getByRole("menuitem", { name: "Delete origin/topic…" })).toBeDisabled();
  });
});

describe("BranchTree — keyboard menus", () => {
  const origin = { name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null };
  function rect(left: number, top: number, width: number, height: number) {
    return {
      left,
      top,
      right: left + width,
      bottom: top + height,
      width,
      height,
      x: left,
      y: top,
      toJSON: () => ({}),
    } as DOMRect;
  }
  beforeEach(() => localStorage.clear());
  afterEach(() => vi.restoreAllMocks());

  it("Shift+F10 on a local branch row opens its menu at the row and Escape returns focus to the row", () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(8, 100, 240, 22));
    renderTree();
    const row = screen.getByTitle("feat/foo").closest('[role="button"]') as HTMLElement;
    row.focus();
    fireEvent.keyDown(row, { key: "F10", shiftKey: true });
    expect(screen.getByRole("menu")).toHaveStyle({ left: "8px", top: "122px" });
    expect(screen.getByRole("menuitem", { name: "Switch to" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Switch to" }), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(row).toHaveFocus();
  });

  it("the Menu key on the current branch focuses the first enabled item (Switch to is disabled)", () => {
    renderTree();
    const row = screen.getByTitle("main").closest('[role="button"]') as HTMLElement;
    row.focus();
    fireEvent.keyDown(row, { key: "ContextMenu" });
    expect(screen.getByRole("menuitem", { name: "Rename" })).toHaveFocus();
  });

  it("the Menu key on a row's '…' button opens the row's menu and Escape returns to the button", () => {
    renderTree();
    const actions = screen.getByRole("button", { name: "Actions for feat/foo" });
    actions.focus();
    fireEvent.keyDown(actions, { key: "ContextMenu" });
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Switch to" }), { key: "Escape" });
    expect(actions).toHaveFocus();
  });

  it("does not open the row menu from the rename field", () => {
    renderTree();
    const row = screen.getByTitle("feat/foo").closest('[role="button"]') as HTMLElement;
    row.focus();
    fireEvent.keyDown(row, { key: "ContextMenu" });
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Rename feat/foo" }), { key: "F10", shiftKey: true });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("remote-tracking branch rows are focusable and open their menu from the keyboard", async () => {
    renderTree({ remotes: [origin], onListRemoteBranches: vi.fn().mockResolvedValue(["topic"]) });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const row = (await screen.findByTitle("topic")).closest('[role="button"]') as HTMLElement;
    expect(row.tabIndex).toBe(0);
    row.focus();
    expect(row).toHaveFocus();
    fireEvent.keyDown(row, { key: "ContextMenu" });
    expect(screen.getByRole("menuitem", { name: "Switch to" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Switch to" }), { key: "Escape" });
    expect(row).toHaveFocus();
  });

  it("the Menu key on a remote folder header opens the remote's menu at the header", () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(4, 300, 240, 22));
    const { props } = renderTree({ remotes: [origin] });
    const header = screen.getByRole("button", { name: "origin" });
    header.focus();
    const prevented = !fireEvent.keyDown(header, { key: "ContextMenu" });
    expect(prevented).toBe(true);
    expect(screen.getByRole("menu")).toHaveStyle({ left: "4px", top: "322px" });
    expect(screen.getByRole("menuitem", { name: "Edit remote" })).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Fetch" }), { key: "Escape" });
    expect(header).toHaveFocus();
    // The header's own toggle did not run.
    expect(props.onListRemoteBranches).not.toHaveBeenCalled();
  });

  it("right-click on a remote folder header opens at the pointer", () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(0, 300, 240, 22));
    renderTree({ remotes: [origin] });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }), { clientX: 50, clientY: 310 });
    expect(screen.getByRole("menu")).toHaveStyle({ left: "50px", top: "310px" });
  });

  it("a remote folder has a '…' button for its actions", () => {
    renderTree({ remotes: [origin] });
    const actions = screen.getByRole("button", { name: "Actions for remote origin" });
    expect(actions).toHaveAttribute("aria-haspopup", "menu");
    expect(actions).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(actions);
    expect(actions).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("menuitem", { name: "Manage credentials" })).toBeInTheDocument();
  });

  // WebKit (macOS) does not focus a button on mouse click, so the menu cannot rely on the focused
  // element to know where it was opened from.
  it("a menu opened by clicking '…' returns focus to that button, even if the click did not focus it", () => {
    renderTree({ remotes: [origin] });
    for (const name of ["Actions for feat/foo", "Actions for remote origin", "Add"]) {
      const elsewhere = screen.getByRole("button", { name: "Show feat/foo in graph" });
      elsewhere.focus();
      const button = screen.getByRole("button", { name });
      fireEvent.click(button);
      fireEvent.keyDown(screen.getAllByRole("menuitem").find((item) => !item.hasAttribute("disabled"))!, {
        key: "Escape",
      });
      expect(button).toHaveFocus();
    }
  });

  it("a right-clicked row gets focus back when its menu closes", () => {
    renderTree();
    const row = screen.getByTitle("feat/foo").closest('[role="button"]') as HTMLElement;
    screen.getByRole("button", { name: "Add" }).focus();
    fireEvent.contextMenu(row);
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Switch to" }), { key: "Escape" });
    expect(row).toHaveFocus();
  });

  it("'…' and '+' report whether their menu is open", () => {
    renderTree();
    const actions = screen.getByRole("button", { name: "Actions for feat/foo" });
    const add = screen.getByRole("button", { name: "Add" });
    expect(actions).toHaveAttribute("aria-expanded", "false");
    expect(add).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(add);
    expect(add).toHaveAttribute("aria-expanded", "true");
    expect(actions).toHaveAttribute("aria-expanded", "false");
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "New branch…" }), { key: "Escape" });
    fireEvent.click(actions);
    expect(actions).toHaveAttribute("aria-expanded", "true");
    expect(add).toHaveAttribute("aria-expanded", "false");
  });

  it("the '+' menu opens under the button, not at (0,0) (regression, PR #72)", () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(180, 40, 20, 20));
    renderTree();
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(screen.getByRole("menu")).toHaveStyle({ left: "180px", top: "60px" });
  });
});

// #493: the tree holds no remote dialog state; it asks its owner to open one, and the owner
// (`WorkspaceDialogs`) renders it outside the sidebar, which is `hidden` while collapsed.
describe("BranchTree — remote dialogs belong to the owner", () => {
  const origin = { name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null };
  beforeEach(() => localStorage.clear());

  it.each([
    ["Edit remote", "edit"],
    ["Manage credentials", "credentials"],
    ["Remove remote", "remove"],
  ] as const)("%s asks onOpenRemoteDialog for the %s dialog and renders none itself", (item, action) => {
    const onOpenRemoteDialog = vi.fn();
    render(<BranchTree {...treeProps({ remotes: [origin], onOpenRemoteDialog })} />);
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: item }));
    expect(onOpenRemoteDialog).toHaveBeenCalledWith({ kind: "remote", action, remoteName: "origin" });
    expect(screen.queryByRole("dialog", { hidden: true })).toBeNull();
  });
});

// #614: always enabled, last before the destructive item, writing plain text.
describe("BranchTree — copy items (#614)", () => {
  let writeText: ReturnType<typeof vi.fn>;
  beforeEach(() => {
    localStorage.clear();
    writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  });

  const busy = { operationDisabled: true, operationDisabledReason: "A transfer is in progress." };
  const menuNames = () => screen.getAllByRole("menuitem").map((item) => item.textContent);

  it("copies a local branch's full name, even while an operation runs", () => {
    renderTree(busy);
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    expect(menuNames().slice(-2)).toEqual(["Copy branch name", "Delete"]);
    const item = screen.getByRole("menuitem", { name: "Copy branch name" });
    expect(item).toBeEnabled();
    fireEvent.click(item);
    expect(writeText).toHaveBeenCalledWith("feat/foo");
  });

  it("copies a remote branch's name as remote/branch", async () => {
    renderTree({ ...busy, remotes: originOnly, onListRemoteBranches: vi.fn().mockResolvedValue(["topic"]) });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const row = (await screen.findByRole("button", { name: "Show origin/topic in graph" })).closest("li")!;
    fireEvent.contextMenu(row);
    expect(menuNames().slice(-2)).toEqual(["Copy branch name", "Delete origin/topic…"]);
    const item = screen.getByRole("menuitem", { name: "Copy branch name" });
    expect(item).toBeEnabled();
    fireEvent.click(item);
    expect(writeText).toHaveBeenCalledWith("origin/topic");
  });

  it.each([
    ["no push URL", null],
    ["the same push URL", "https://example.com/r.git"],
  ])("copies a remote's fetch URL and offers no push URL with %s", (_label, pushUrl) => {
    const remote = { name: "origin", fetchUrl: "https://example.com/r.git", pushUrl, authMode: null, authUsername: null };
    renderTree({ ...busy, remotes: [remote] });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    expect(screen.queryByRole("menuitem", { name: "Copy push URL" })).toBeNull();
    const item = screen.getByRole("menuitem", { name: "Copy fetch URL" });
    expect(item).toBeEnabled();
    fireEvent.click(item);
    expect(writeText).toHaveBeenCalledWith("https://example.com/r.git");
  });

  it("offers Copy push URL when the push URL differs", () => {
    const remote = {
      name: "origin",
      fetchUrl: "https://example.com/r.git",
      pushUrl: "git@example.com:r.git",
      authMode: null,
      authUsername: null,
    };
    renderTree({ remotes: [remote] });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    expect(menuNames().slice(-3)).toEqual(["Copy fetch URL", "Copy push URL", "Remove remote"]);
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy push URL" }));
    expect(writeText).toHaveBeenCalledWith("git@example.com:r.git");
  });
});

// #744: a branch dragged onto a branch. jsdom has no layout, so each drag says which element is
// under the pointer.
describe("BranchTree — dragging a branch onto a branch", () => {
  const originalElementFromPoint = document.elementFromPoint;
  beforeEach(() => {
    localStorage.clear();
  });
  afterEach(() => {
    document.elementFromPoint = originalElementFromPoint;
    vi.useRealTimers();
  });

  const threeBranches = [
    { name: "main", isCurrent: true },
    { name: "feat/foo", isCurrent: false },
    { name: "other", isCurrent: false },
  ];

  const mainRow = () => screen.getByText(/main.*\(current\)/).closest("li") as HTMLElement;
  const fooRow = () => screen.getByRole("button", { name: "foo" });
  const otherRow = () => screen.getByRole("button", { name: "other" });

  function pressAndMove(source: HTMLElement, over: Element) {
    document.elementFromPoint = vi.fn(() => over);
    fireEvent.pointerDown(source, { clientX: 5, clientY: 5, button: 0 });
    fireEvent.pointerMove(window, { clientX: 5, clientY: 40 });
  }

  function drag(source: HTMLElement, target: HTMLElement) {
    pressAndMove(source, target);
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });
  }

  const menuNames = () => screen.getAllByRole("menuitem").map((item) => item.textContent);

  it("a branch dropped on the current branch offers Merge first, then Rebase, and runs the choice", () => {
    const { props } = renderTree({ branches: threeBranches });
    pressAndMove(fooRow(), mainRow());
    expect(mainRow()).toHaveAttribute("data-drop-position", "on");
    expect(fooRow()).toHaveAttribute("data-dragging", "true");
    expect(screen.getByRole("status")).toHaveTextContent(
      "Dragging feat/foo. Drop it on main to merge or rebase; Escape cancels.",
    );
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });

    expect(menuNames()).toEqual(["Merge feat/foo into main…", "Rebase main onto feat/foo…", "Cancel"]);
    expect(screen.getByRole("status")).toHaveTextContent(
      "Dropped feat/foo on main: Merge feat/foo into main…, or Rebase main onto feat/foo…",
    );
    fireEvent.click(screen.getByRole("menuitem", { name: "Merge feat/foo into main…" }));
    expect(props.onMergeBranch).toHaveBeenCalledWith("feat/foo");
    expect(props.onRebaseOnto).not.toHaveBeenCalled();
    expect(mainRow()).not.toHaveAttribute("data-drop-position");
  });

  it("Rebase in the drop menu opens the planner onto the dropped branch", () => {
    const { props } = renderTree({ branches: threeBranches });
    drag(fooRow(), mainRow());
    fireEvent.click(screen.getByRole("menuitem", { name: "Rebase main onto feat/foo…" }));
    expect(props.onRebaseOnto).toHaveBeenCalledWith("refs/heads/feat/foo");
    expect(props.onMergeBranch).not.toHaveBeenCalled();
  });

  it("the current branch dropped on a branch offers Rebase first", () => {
    renderTree({ branches: threeBranches });
    expect(screen.getByText(/main.*\(current\)/)).toBeInTheDocument();
    drag(mainRow(), otherRow());
    expect(menuNames()).toEqual(["Rebase main onto other…", "Merge other into main…", "Cancel"]);
    expect(screen.getByRole("status")).not.toHaveTextContent("Dragging");
  });

  it("Cancel in the drop menu runs nothing", () => {
    const { props } = renderTree({ branches: threeBranches });
    drag(fooRow(), mainRow());
    fireEvent.click(screen.getByRole("menuitem", { name: "Cancel" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(props.onMergeBranch).not.toHaveBeenCalled();
    expect(props.onRebaseOnto).not.toHaveBeenCalled();
  });

  it("a remote branch dropped on the current branch offers Rebase only", async () => {
    const { props } = renderTree({
      branches: threeBranches,
      remotes: originOnly,
      onListRemoteBranches: vi.fn().mockResolvedValue(["topic"]),
    });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const topicRow = await screen.findByRole("button", { name: "topic" });
    drag(topicRow, mainRow());
    expect(menuNames()).toEqual(["Rebase main onto origin/topic…", "Cancel"]);
    fireEvent.click(screen.getByRole("menuitem", { name: "Rebase main onto origin/topic…" }));
    expect(props.onRebaseOnto).toHaveBeenCalledWith("refs/remotes/origin/topic");
  });

  it("shows why a drop between two other branches can't run, and refuses it", () => {
    const { props } = renderTree({ branches: threeBranches });
    pressAndMove(fooRow(), otherRow());
    expect(otherRow()).toHaveAttribute("data-drop-position", "refused");
    const reason = "Only the current branch, main, can be merged into or rebased: drop on main, or drag main.";
    expect(screen.getByText(reason)).toBeInTheDocument();
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(`Can't drop here. ${reason}`);
    expect(props.onMergeBranch).not.toHaveBeenCalled();
  });

  it("refuses the drop with the busy reason while another operation runs", () => {
    renderTree({ branches: threeBranches, operationDisabled: true, operationDisabledReason: "A rebase is in progress." });
    pressAndMove(fooRow(), mainRow());
    expect(mainRow()).toHaveAttribute("data-drop-position", "refused");
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Can't drop here. A rebase is in progress.");
  });

  it("Escape cancels the drag, and a dropped row isn't also clicked", () => {
    const onGoToBranch = vi.fn();
    renderTree({ branches: threeBranches, onGoToBranch });
    pressAndMove(fooRow(), mainRow());
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Drag cancelled.");
    expect(mainRow()).not.toHaveAttribute("data-drop-position");

    drag(fooRow(), mainRow());
    fireEvent.click(fooRow());
    expect(onGoToBranch).not.toHaveBeenCalled();
  });

  it("a press on a row's buttons doesn't start a drag", () => {
    renderTree({ branches: threeBranches });
    pressAndMove(screen.getByRole("button", { name: "Show feat/foo in graph" }), mainRow());
    expect(mainRow()).not.toHaveAttribute("data-drop-position");
    fireEvent.pointerUp(window, { clientX: 5, clientY: 40 });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("opens a collapsed folder hovered for a moment during a drag, without remembering it open", () => {
    renderTree({ branches: threeBranches });
    const folder = screen.getByRole("button", { name: "feat" });
    fireEvent.click(folder);
    expect(screen.queryByRole("button", { name: "foo" })).not.toBeInTheDocument();
    const stored = localStorage.getItem("branchtree.folder.local/feat");

    vi.useFakeTimers();
    pressAndMove(mainRow(), folder);
    act(() => vi.advanceTimersByTime(300));
    expect(screen.queryByRole("button", { name: "foo" })).not.toBeInTheDocument();
    act(() => vi.advanceTimersByTime(400));
    expect(screen.getByRole("button", { name: "foo" })).toBeInTheDocument();
    expect(localStorage.getItem("branchtree.folder.local/feat")).toBe(stored);
    fireEvent.keyDown(window, { key: "Escape" });
  });
});
