import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, expectTypeOf, it, vi } from "vitest";
import { useState } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { RemoteDialog } from "../state/useAppState";
import { BranchTree } from "./BranchTree";

type IsOptional<T, K extends keyof T> = Pick<T, K> extends Required<Pick<T, K>> ? false : true;

const baseBranches = [
  { name: "main", isCurrent: true },
  { name: "feat/foo", isCurrent: false },
];

type BranchTreeProps = Parameters<typeof BranchTree>[0];

function renderTree(overrides: Partial<BranchTreeProps> = {}) {
  const props = treeProps(overrides);
  return { ...render(<BranchTree {...props} />), props };
}

function renderTreeElement(overrides: Partial<BranchTreeProps>) {
  return <BranchTree {...treeProps(overrides)} />;
}

function treeProps(overrides: Partial<BranchTreeProps>): BranchTreeProps {
  return {
    branches: baseBranches,
    createBranchDraft: null,
    onSwitchBranch: vi.fn(),
    onCreateBranch: vi.fn().mockResolvedValue(null),
    onDeleteBranch: vi.fn().mockResolvedValue(undefined),
    onRenameBranch: vi.fn(),
    onOpenCreateBranchDraft: vi.fn(),
    onCloseCreateBranchDraft: vi.fn(),
    onMergeBranch: vi.fn(),
    isMerging: false,
    isRebasing: false,
    operationDisabled: false,
    operationDisabledReason: null,
    graphBranchSelection: null,
    onSetGraphBranchSelection: vi.fn(),
    graphRemoteBranchSelection: null,
    onSetGraphRemoteBranchSelection: vi.fn(),
    remoteBranchesInGraph: [],
    remotes: [],
    upstream: null,
    remoteUpstreams: {},
    onAddRemote: vi.fn().mockResolvedValue(null),
    onRenameRemote: vi.fn().mockResolvedValue(true),
    onUpdateRemoteUrls: vi.fn().mockResolvedValue(undefined),
    onRemoveRemote: vi.fn().mockResolvedValue(undefined),
    onSaveHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onForgetHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onSetRemoteAuthMode: vi.fn().mockResolvedValue(true),
    onSetUpstream: vi.fn().mockResolvedValue(undefined),
    onClearUpstream: vi.fn().mockResolvedValue(undefined),
    onListRemoteBranches: vi.fn().mockResolvedValue([]),
    onFetchRemote: vi.fn().mockResolvedValue(undefined),
    onPushCurrentBranch: vi.fn().mockResolvedValue(undefined),
    onPull: vi.fn().mockResolvedValue(undefined),
    pendingPull: null,
    pullOutcome: null,
    onMergePull: vi.fn().mockResolvedValue(undefined),
    onRebasePull: vi.fn(),
    onCancelPull: vi.fn(),
    addRemoteDraftOpen: false,
    onOpenAddRemoteDraft: vi.fn(),
    onCloseAddRemoteDraft: vi.fn(),
    onOpenPushDialog: vi.fn(),
    onOpenDeleteRemoteBranch: vi.fn(),
    refreshGeneration: 0,
    ...overrides,
  };
}

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

  it("single-clicking a branch selects it without switching", () => {
    const { props } = renderTree();
    fireEvent.click(screen.getByRole("button", { name: "foo" }));
    expect(props.onSwitchBranch).not.toHaveBeenCalled();
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
    rerender(<BranchTree {...props} branches={baseBranches.filter((b) => b.name !== "feat/foo")} />);

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

  it("disables Rename/Delete/Merge menu items while a rebase is in progress", () => {
    renderTree({ isRebasing: true });
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    expect(screen.getByRole("menuitem", { name: "Rename" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Merge into current branch…" })).toBeDisabled();
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

  it("a local branch's context menu lists Checkout first and calls onSwitchBranch", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    const items = screen.getAllByRole("menuitem");
    expect(items[0]).toHaveAccessibleName("Checkout");
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

  it("Checkout is disabled for the current branch", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: /main \(current\)/ }));
    expect(screen.getByRole("menuitem", { name: "Checkout" })).toBeDisabled();
  });

  it("a branch's context menu Isolate branch calls onSetGraphBranchSelection with only that branch", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByRole("button", { name: "foo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Isolate branch" }));
    expect(props.onSetGraphBranchSelection).toHaveBeenCalledWith(["feat/foo"]);
  });

  it("the header '+' menu's Show all branches is disabled with no saved selection, and restores every branch when a filter is active", () => {
    const { props, rerender } = renderTree();
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(screen.getByRole("menuitem", { name: "Show all branches" })).toBeDisabled();

    rerender(<BranchTree {...props} graphBranchSelection={["feat/foo"]} />);
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Show all branches" }));
    expect(props.onSetGraphBranchSelection).toHaveBeenCalledWith(["main", "feat/foo"]);
  });

  it("a non-null createBranchDraft shows the create form; submitting calls onCreateBranch with its startPoint", async () => {
    const { props } = renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    fireEvent.change(screen.getByPlaceholderText("New branch name"), { target: { value: "feat/new" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect(props.onCreateBranch).toHaveBeenCalledWith("feat/new", "HEAD");
  });

  // Ported from BranchSwitcher.test.tsx — the create-form failure/dismiss flow doesn't involve
  // the popover at all, so it transfers unchanged (createBranchDraft renders the form directly).
  it("shows a failed create-branch's message next to the draft form and keeps the entered name", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue("branch already exists");
    renderTree({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("branch already exists");
    expect(screen.getByPlaceholderText("New branch name")).toHaveValue("feature");
  });

  it("clears the create-branch failure message once the name is edited again", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue("branch already exists");
    renderTree({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await screen.findByRole("alert");

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature-2" },
    });

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("dismissing the create-branch failure message clears it", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue("branch already exists");
    renderTree({ createBranchDraft: { startPoint: "abc123" }, onCreateBranch });

    fireEvent.change(screen.getByPlaceholderText("New branch name"), {
      target: { value: "feature" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await screen.findByRole("alert");

    fireEvent.click(screen.getByRole("button", { name: "Dismiss error" }));

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("Cancel in the create form calls onCloseCreateBranchDraft", () => {
    const onCloseCreateBranchDraft = vi.fn();
    renderTree({ createBranchDraft: { startPoint: "HEAD" }, onCloseCreateBranchDraft });

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onCloseCreateBranchDraft).toHaveBeenCalled();
  });

  // The graph checkboxes stay visible inline regardless of the popover/menu, so this transfers
  // unchanged too.
  it("a branch absent from an explicit selection renders its swatch unpressed; toggling it adds it back", () => {
    const onSetGraphBranchSelection = vi.fn();
    renderTree({ graphBranchSelection: ["main"], onSetGraphBranchSelection });

    expect(screen.getByRole("button", { name: "Show main in graph" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Show feat/foo in graph" })).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(screen.getByRole("button", { name: "Show feat/foo in graph" }));

    expect(onSetGraphBranchSelection).toHaveBeenCalledWith(["main", "feat/foo"]);
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

  it("disables the merge action while a merge is already in progress", () => {
    renderTree({ isMerging: true });

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
    const { props } = renderTree({ isRebasing: true });

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

  it("lists remote folders and lazily loads their branches on first expand", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main", "feat/foo"]);
    renderTree({ remotes: oneRemote, onListRemoteBranches });
    expect(onListRemoteBranches).not.toHaveBeenCalled();
    const remoteFolder = screen.getByRole("button", { name: "origin" }).closest("li")!;
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(onListRemoteBranches).toHaveBeenCalledWith("origin");
    // Scoped to the remote's own <li>, not `screen`: `baseBranches` (the default local branches)
    // already has a "feat/foo" branch, so an unscoped query would ambiguously match either row.
    expect(await within(remoteFolder).findByText("foo")).toBeInTheDocument();
  });

  // With no saved remote selection, every remote branch present in the commit graph
  // (`remoteBranchesInGraph`, NOT the lazy per-remote-folder fetch cache) has its swatch pressed
  // (shown) by default — mirroring the local-branch swatch's default (see "with no saved graph
  // selection, every branch's graph swatch is pressed (shown) by default" above). So toggling off
  // origin/main here removes it from that set rather than adding it; origin/develop stays, which
  // also proves the qualified `${remoteName}/${branchName}` naming is computed correctly.
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

  it("does not re-fetch remote branches on a second expand", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main"]);
    renderTree({ remotes: oneRemote, onListRemoteBranches });
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand
    await screen.findByText("main");
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // collapse
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand again
    expect(onListRemoteBranches).toHaveBeenCalledOnce();
  });

  // The lazily-fetched `remoteBranches[remoteName]` cache was never invalidated after Fetch, so
  // the tree kept showing pre-fetch branches for the rest of the session (folder open state is
  // localStorage-persisted, so not even a restart mid-session helped). Fetch must drop the cache
  // entry and, if the folder is currently open, re-fetch immediately.
  it("Fetch drops the cached remote branch list and re-fetches when the folder is open", async () => {
    const onListRemoteBranches = vi
      .fn()
      .mockResolvedValueOnce(["main"])
      .mockResolvedValueOnce(["main", "feat/new-on-remote"]);
    const onFetchRemote = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onListRemoteBranches, onFetchRemote });

    const remoteFolder = screen.getByRole("button", { name: "origin" }).closest("li")!;
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand, first fetch
    await within(remoteFolder).findByText("main");
    expect(onListRemoteBranches).toHaveBeenCalledTimes(1);

    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Fetch" }));
    expect(onFetchRemote).toHaveBeenCalledWith("origin");

    expect(await within(remoteFolder).findByText("new-on-remote")).toBeInTheDocument();
    expect(onListRemoteBranches).toHaveBeenCalledTimes(2);
  });

  it("shows an inline error, without an unhandled rejection, when the lazy remote-branch fetch fails", async () => {
    const onListRemoteBranches = vi.fn().mockRejectedValue(new Error("network unreachable"));
    renderTree({ remotes: oneRemote, onListRemoteBranches });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not list branches for origin.");
  });

  it("right-clicking a remote branch offers Checkout and Set as upstream", async () => {
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["feat/foo"]) });
    const remoteFolder = screen.getByRole("button", { name: "origin" }).closest("li")!;
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    // Scoped to the remote's own <li> — `baseBranches` already has a local "feat/foo".
    fireEvent.contextMenu(await within(remoteFolder).findByText("foo"));
    expect(screen.getByRole("menuitem", { name: "Checkout" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Set as upstream for current branch" })).toBeInTheDocument();
  });

  it("double-clicking a remote branch checks it out the same way the context menu's Checkout does", async () => {
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
    const actionsButton = await within(screen.getByRole("button", { name: /^origin/ }).closest("li")!).findByRole("button", { name: "Actions for feat/foo" });
    expect(actionsButton).toHaveAttribute("aria-haspopup", "menu");
    fireEvent.click(actionsButton);
    expect(screen.getByRole("menuitem", { name: "Checkout" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Set as upstream for current branch" })).toBeInTheDocument();
  });

  it("Checkout switches to an existing same-named local branch instead of creating a new one", async () => {
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
    fireEvent.click(screen.getByRole("menuitem", { name: "Checkout" }));
    expect(onSwitchBranch).toHaveBeenCalledWith("feat/foo");
    expect(onCreateBranch).not.toHaveBeenCalled();
  });

  it("Checkout creates and tracks a new local branch when none exists locally", async () => {
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
    fireEvent.click(screen.getByRole("menuitem", { name: "Checkout" }));
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
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote" }));
    expect(onUpdateRemoteUrls).toHaveBeenCalledWith("origin", "git@github.com:user/repo.git", null);
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

  it("keeps the add-remote form hidden until addRemoteDraftOpen is true", () => {
    renderTree({ addRemoteDraftOpen: false });
    expect(screen.queryByLabelText("Fetch URL")).not.toBeInTheDocument();
  });

  it("the header '+' menu's Add Remote opens the add-remote draft", () => {
    const onOpenAddRemoteDraft = vi.fn();
    renderTree({ onOpenAddRemoteDraft });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add remote…" }));
    expect(onOpenAddRemoteDraft).toHaveBeenCalledOnce();
  });

  it("addRemoteDraftOpen shows the add-remote form; submitting calls onAddRemote", async () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    renderTree({ addRemoteDraftOpen: true, onAddRemote });
    fireEvent.change(screen.getByTestId("add-remote-fetch-url"), {
      target: { value: "git@github.com:user/repo.git" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    expect(onAddRemote).toHaveBeenCalledWith("origin", "git@github.com:user/repo.git", null);
  });

  it("shows the current branch's upstream status and a Pull button", () => {
    renderTree({ upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" } });
    expect(screen.getByText(/main tracks origin\/main/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Pull" })).toBeEnabled();
  });

  it("Set upstream… on the current branch's context menu opens a dialog; submitting calls onSetUpstream", async () => {
    const onSetUpstream = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onSetUpstream });
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    fireEvent.click(screen.getByRole("menuitem", { name: "Set upstream…" }));
    const dialog = await screen.findByRole("dialog", { name: "Set upstream for main" });
    fireEvent.change(within(dialog).getByLabelText("Upstream remote"), { target: { value: "origin" } });
    fireEvent.change(within(dialog).getByLabelText("Upstream branch"), { target: { value: "main" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Set upstream" }));
    expect(onSetUpstream).toHaveBeenCalledWith("origin", "main");
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

  it("offers merge or rebase only after a divergent pull, unchanged from RemotePanel", () => {
    renderTree({ pendingPull: { upstreamRef: "origin/main" } });
    expect(screen.getByRole("dialog", { name: "Pull has diverged" })).toBeInTheDocument();
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

  // A remote branch's Checkout can call `onSwitchBranch` (when a same-named local branch already
  // exists), which is exactly the hazard `isRebasing` already guards for local branches: switching
  // underneath a paused rebase silently retargets an unrelated branch once `rebase::finish` moves
  // the original branch ref (see BranchTree's local "does not switch branches while a rebase is in
  // progress" test and BranchSwitcher's original comment on the same hazard). `operationDisabled`
  // is the broader gate (rebase/merge/transfer/etc.), so both remote-branch actions must respect it
  // too, matching the sibling remote-folder items right above.
  it("disables Checkout and Set as upstream on a remote branch while another operation is active", async () => {
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["only-remote"]), operationDisabled: true });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.contextMenu(await screen.findByText("only-remote"));
    expect(screen.getByRole("menuitem", { name: "Checkout" })).toBeDisabled();
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

  it("keeps Pull disabled while a reconciliation choice is open and focuses Cancel", async () => {
    renderTree({
      upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" },
      pendingPull: { upstreamRef: "origin/main" },
    });
    expect(screen.getByRole("button", { name: "Pull" })).toBeDisabled();
    const dialog = screen.getByRole("dialog", { name: "Pull has diverged" });
    await waitFor(() => {
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    });
  });

  it("disables divergent pull choices while another repository operation is active", () => {
    renderTree({
      upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" },
      operationDisabled: true,
      pendingPull: { upstreamRef: "origin/main" },
    });
    const dialog = screen.getByRole("dialog", { name: "Pull has diverged" });
    expect(within(dialog).getByRole("button", { name: "Merge" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Rebase" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeEnabled();
  });

  it("renders a successful up-to-date pull outcome", () => {
    renderTree({
      upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" },
      pullOutcome: { kind: "UpToDate" },
    });
    expect(screen.getByRole("status")).toHaveTextContent("Already up to date.");
  });

  it("requires upstream discovery in both the client and BranchTree contracts", () => {
    expectTypeOf<IsOptional<RepoClient, "getRemoteUpstreams">>().toEqualTypeOf<false>();
    expectTypeOf<IsOptional<BranchTreeProps, "remoteUpstreams">>().toEqualTypeOf<false>();
  });

  it("adds a remote with a custom name and push URL via the add-remote form", () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    renderTree({ addRemoteDraftOpen: true, onAddRemote });
    fireEvent.change(screen.getByLabelText("Remote name"), { target: { value: "backup" } });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    fireEvent.click(screen.getByText("Push URL (optional)"));
    fireEvent.change(screen.getByLabelText("Push URL"), { target: { value: "../push-backup.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    expect(onAddRemote).toHaveBeenCalledWith("backup", "../backup.git", "../push-backup.git");
  });

  it("shows example placeholder text on the add-remote fetch URL field", () => {
    renderTree({ addRemoteDraftOpen: true });
    expect(screen.getByLabelText("Fetch URL")).toHaveAttribute("placeholder", "git@github.com:user/repo.git");
  });

  it("auto-derives the remote name as origin when no remote is named origin yet", () => {
    renderTree({ addRemoteDraftOpen: true, remotes: [] });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "git@github.com:user/repo.git" } });
    expect(screen.getByLabelText("Remote name")).toHaveValue("origin");
  });

  it("auto-derives the remote name from the URL slug when origin already exists", () => {
    renderTree({ addRemoteDraftOpen: true, remotes: oneRemote });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "git@github.com:user/repo.git" } });
    expect(screen.getByLabelText("Remote name")).toHaveValue("repo");
  });

  // Real typing fires one `change` per keystroke, which is what broke the original
  // `newName === ""` guard: the first keystroke derived a one-character name, and every keystroke
  // after that saw a non-empty name and stopped deriving.
  it("keeps deriving the remote name while the URL is typed character by character", () => {
    renderTree({ addRemoteDraftOpen: true, remotes: oneRemote });
    const fetchUrl = screen.getByLabelText("Fetch URL");
    const url = "git@github.com:user/repo.git";
    for (let length = 1; length <= url.length; length += 1) {
      fireEvent.change(fetchUrl, { target: { value: url.slice(0, length) } });
    }
    expect(screen.getByLabelText("Remote name")).toHaveValue("repo");
  });

  it("does not overwrite a manually entered remote name when the URL changes", () => {
    renderTree({ addRemoteDraftOpen: true, remotes: oneRemote });
    fireEvent.change(screen.getByLabelText("Remote name"), { target: { value: "custom" } });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "git@github.com:user/repo.git" } });
    expect(screen.getByLabelText("Remote name")).toHaveValue("custom");
  });

  it("keeps the Push URL field collapsed behind a disclosure by default", () => {
    renderTree({ addRemoteDraftOpen: true });
    expect(screen.queryByLabelText("Push URL")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("Push URL (optional)"));
    expect(screen.getByLabelText("Push URL")).toBeInTheDocument();
  });

  // `useAppState`'s `addRemote` never rejects — it reports failure by *resolving* to the message.
  it("shows a failed add-remote's message and keeps the entered values", async () => {
    const onAddRemote = vi.fn().mockResolvedValue("invalid fetch URL");
    renderTree({ addRemoteDraftOpen: true, remotes: oneRemote, onAddRemote });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "not-a-url" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("invalid fetch URL");
    expect(screen.getByLabelText("Fetch URL")).toHaveValue("not-a-url");
    expect(screen.getByLabelText("Remote name")).toHaveValue("not-a-url");
  });

  it("clears the add-remote failure message once the Fetch URL is edited again", async () => {
    const onAddRemote = vi.fn().mockResolvedValue("invalid fetch URL");
    renderTree({ addRemoteDraftOpen: true, remotes: oneRemote, onAddRemote });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "not-a-url" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    await screen.findByRole("alert");
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("clears the form and re-collapses the Push URL disclosure after a successful add", async () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    renderTree({ addRemoteDraftOpen: true, onAddRemote });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    fireEvent.click(screen.getByText("Push URL (optional)"));
    fireEvent.change(screen.getByLabelText("Push URL"), { target: { value: "../push-backup.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    await waitFor(() => expect(screen.getByLabelText("Fetch URL")).toHaveValue(""));
    expect(screen.getByLabelText("Remote name")).toHaveValue("");
    expect(screen.queryByLabelText("Push URL")).not.toBeInTheDocument();
  });

  it("Cancel in the add-remote form calls onCloseAddRemoteDraft without adding a remote", () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    const onCloseAddRemoteDraft = vi.fn();
    renderTree({ addRemoteDraftOpen: true, onAddRemote, onCloseAddRemoteDraft });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCloseAddRemoteDraft).toHaveBeenCalledOnce();
    expect(onAddRemote).not.toHaveBeenCalled();
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

  it("offers branch suggestions from the selected remote for the upstream-branch field", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main", "develop"]);
    renderTree({ remotes: oneRemote, onListRemoteBranches });
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    fireEvent.click(screen.getByRole("menuitem", { name: "Set upstream…" }));
    const dialog = await screen.findByRole("dialog", { name: "Set upstream for main" });
    fireEvent.change(within(dialog).getByLabelText("Upstream remote"), { target: { value: "origin" } });
    await waitFor(() => expect(onListRemoteBranches).toHaveBeenCalledWith("origin"));
    const branchInput = within(dialog).getByLabelText("Upstream branch");
    const datalistId = branchInput.getAttribute("list");
    expect(datalistId).not.toBeNull();
    const datalist = document.getElementById(datalistId!);
    expect(datalist).not.toBeNull();
    const optionValues = Array.from(datalist!.querySelectorAll("option")).map((option) =>
      option.getAttribute("value"),
    );
    expect(optionValues).toEqual(["main", "develop"]);
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
    const actions = await within(screen.getByRole("button", { name: /^origin/ }).closest("li")!).findByRole("button", { name: "Actions for feat/foo" });
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

  it("focuses the new-branch input on open and Escape cancels", () => {
    const { props } = renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    const input = screen.getByPlaceholderText("New branch name");
    expect(input).toHaveFocus();
    fireEvent.keyDown(input, { key: "Escape" });
    expect(props.onCloseCreateBranchDraft).toHaveBeenCalled();
  });

  it("focuses the add-remote name field on open and Escape cancels", () => {
    const { props } = renderTree({ addRemoteDraftOpen: true });
    const input = screen.getByPlaceholderText("origin");
    expect(input).toHaveFocus();
    fireEvent.keyDown(input, { key: "Escape" });
    expect(props.onCloseAddRemoteDraft).toHaveBeenCalled();
  });

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
  const remote = [{ name: "origin", fetchUrl: "u", pushUrl: null, authMode: null, authUsername: null }];
  const up = { localBranch: "main", remoteName: "origin", remoteBranch: "main" };

  it("shows a non-interactive Loading row while a remote's branches load", async () => {
    renderTree({ remotes: remote, onListRemoteBranches: vi.fn().mockReturnValue(new Promise(() => {})) });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    const row = await screen.findByText("Loading…");
    expect(row.closest("[role=button]")).toBeNull();
  });

  it("shows 'No branches' for an empty remote", async () => {
    renderTree({ remotes: remote, onListRemoteBranches: vi.fn().mockResolvedValue([]) });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(await screen.findByText("No branches")).toBeInTheDocument();
  });

  it("explains the disabled Pull button when there is no upstream", () => {
    renderTree();
    expect(screen.getByRole("button", { name: "Pull" })).toHaveAttribute(
      "title",
      "No upstream set for the current branch.",
    );
  });

  it("asks for confirmation before clearing the upstream", async () => {
    const { props } = renderTree({ upstream: up });
    fireEvent.click(screen.getByRole("button", { name: "Clear upstream" }));
    expect(props.onClearUpstream).not.toHaveBeenCalled();
    const dialog = await screen.findByRole("dialog", { name: "Clear upstream confirmation", hidden: true });
    fireEvent.click(within(dialog).getByRole("button", { name: "Clear upstream", hidden: true }));
    expect(props.onClearUpstream).toHaveBeenCalledOnce();
  });
});

describe("BranchTree — new branch form", () => {
  it("shows the base branch for HEAD and a short id for a commit", () => {
    const { unmount } = renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    expect(screen.getByText(/New branch from/)).toHaveTextContent("New branch from main");
    unmount();
    renderTree({ createBranchDraft: { startPoint: "abcdef1234567890abcdef1234567890abcdef12" } });
    expect(screen.getByText(/New branch from/)).toHaveTextContent("New branch from abcdef1");
  });

  it("validates the name inline and blocks Create, tied to the field", () => {
    const { props } = renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    const input = screen.getByRole("textbox", { name: "Branch name" });
    fireEvent.change(input, { target: { value: "bad name" } });
    expect(input).toHaveAccessibleDescription("Branch names can't contain spaces.");
    expect(screen.getByRole("button", { name: "Create" })).toBeDisabled();
    expect(props.onCreateBranch).not.toHaveBeenCalled();
  });

  it("checks out the new branch when the option is ticked", async () => {
    const { props } = renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    fireEvent.change(screen.getByRole("textbox", { name: "Branch name" }), { target: { value: "feat/x" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Check out after creating" }));
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(props.onSwitchBranch).toHaveBeenCalledWith("feat/x"));
  });
});

const originOnly = [{ name: "origin", fetchUrl: "x", pushUrl: null, authMode: null, authUsername: null }];

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

  it("reloads open remote folders when refreshGeneration changes", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValueOnce(["main", "topic"]).mockResolvedValueOnce(["main"]);
    const { props, rerender } = renderTree({ remotes: originOnly, onListRemoteBranches });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(await screen.findByRole("button", { name: "Show origin/topic in graph" })).toBeInTheDocument();

    rerender(<BranchTree {...props} refreshGeneration={1} />);

    // No "Loading…" flash: the old list stays until the new one lands.
    expect(screen.queryByText("Loading…")).toBeNull();
    await waitFor(() => expect(screen.queryByRole("button", { name: "Show origin/topic in graph" })).toBeNull());
    expect(screen.getByRole("button", { name: "Show origin/main in graph" })).toBeInTheDocument();
    expect(onListRemoteBranches).toHaveBeenCalledTimes(2);
  });

  it("forgets a closed folder's list on refresh so the next expand reloads it", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main"]);
    const { props, rerender } = renderTree({ remotes: originOnly, onListRemoteBranches });
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand
    await screen.findByRole("button", { name: "Show origin/main in graph" });
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // collapse

    rerender(<BranchTree {...props} refreshGeneration={1} />);
    expect(onListRemoteBranches).toHaveBeenCalledOnce(); // closed folders are not reloaded eagerly
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand again

    await waitFor(() => expect(onListRemoteBranches).toHaveBeenCalledTimes(2));
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
    expect(screen.getByRole("menuitem", { name: "Checkout" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Checkout" }), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(row).toHaveFocus();
  });

  it("the Menu key on the current branch focuses the first enabled item (Checkout is disabled)", () => {
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
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Checkout" }), { key: "Escape" });
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
    expect(screen.getByRole("menuitem", { name: "Checkout" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Checkout" }), { key: "Escape" });
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
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Checkout" }), { key: "Escape" });
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

describe("BranchTree — controlled remote dialogs", () => {
  const origin = { name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null };
  beforeEach(() => localStorage.clear());

  it.each([
    ["edit", "Edit origin"],
    ["credentials", "Credentials for origin"],
    ["remove", "Remove remote confirmation"],
  ] as const)("renders the %s dialog from props even while Branches is collapsed", async (kind, name) => {
    localStorage.setItem("sidebar-branches", "closed");
    renderTree({ remotes: [origin], remoteDialog: { kind, remoteName: "origin" }, onRemoteDialogChange: vi.fn() });
    expect(screen.queryByRole("button", { name: "origin" })).toBeNull(); // body not rendered
    expect(await screen.findByRole("dialog", { name, hidden: true })).toBeInTheDocument();
  });

  it("menu items report the dialog through onRemoteDialogChange when controlled", () => {
    const onRemoteDialogChange = vi.fn();
    renderTree({ remotes: [origin], remoteDialog: null, onRemoteDialogChange });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Manage credentials" }));
    expect(onRemoteDialogChange).toHaveBeenCalledWith({ kind: "credentials", remoteName: "origin" });
    expect(screen.queryByRole("dialog", { name: "Credentials for origin", hidden: true })).toBeNull();
  });

  it("closing a controlled dialog reports null", () => {
    const onRemoteDialogChange = vi.fn();
    renderTree({ remotes: [origin], remoteDialog: { kind: "edit", remoteName: "origin" }, onRemoteDialogChange });
    fireEvent.click(screen.getByRole("button", { name: "Cancel", hidden: true }));
    expect(onRemoteDialogChange).toHaveBeenCalledWith(null);
  });

  it("renders nothing for a dialog whose remote is gone", () => {
    renderTree({ remotes: [], remoteDialog: { kind: "edit", remoteName: "origin" }, onRemoteDialogChange: vi.fn() });
    expect(screen.queryByRole("dialog", { name: "Edit origin", hidden: true })).toBeNull();
  });

  it("asks to clear upstreams based on the upstreams when the confirmation renders", () => {
    const props = {
      remotes: [origin],
      remoteDialog: { kind: "remove" as const, remoteName: "origin" },
      onRemoteDialogChange: vi.fn(),
    };
    const { rerender, props: full } = renderTree({ ...props, remoteUpstreams: {} });
    expect(screen.getByText("Remove remote origin?")).toBeInTheDocument();
    rerender(
      <BranchTree
        {...full}
        remoteUpstreams={{ origin: [{ localBranch: "main", remoteName: "origin", remoteBranch: "main" }] }}
      />,
    );
    expect(screen.getByText(/clear upstreams for main/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Confirm remove", hidden: true }));
    expect(full.onRemoveRemote).toHaveBeenCalledWith("origin", true);
  });

  // `renameRemote` is optimistic: `remotes` already holds the new name while the request runs, and
  // a failure rolls it back. The dialog must stay mounted with what the user typed throughout.
  function RenameHarness({ result }: { result: boolean }) {
    const [remotes, setRemotes] = useState([origin]);
    const [dialog, setDialog] = useState<RemoteDialog | null>({ kind: "edit", remoteName: "origin" });
    const props = {
      remotes,
      remoteDialog: dialog,
      onRemoteDialogChange: setDialog,
      onRenameRemote: async (oldName: string, newName: string) => {
        setRemotes((prev) => prev.map((r) => (r.name === oldName ? { ...r, name: newName } : r)));
        await new Promise((resolve) => setTimeout(resolve, 0));
        if (!result) setRemotes((prev) => prev.map((r) => (r.name === newName ? { ...r, name: oldName } : r)));
        return result;
      },
      onUpdateRemoteUrls: updateUrls,
    };
    return renderTreeElement(props);
  }
  const updateUrls = vi.fn().mockResolvedValue(undefined);

  it.each([true, false])("the Edit dialog stays open with the typed values while a rename runs (succeeds: %s)", async (result) => {
    updateUrls.mockClear();
    render(<RenameHarness result={result} />);
    const dialog = screen.getByRole("dialog", { name: "Edit origin", hidden: true });
    fireEvent.change(within(dialog).getByLabelText("Remote name"), { target: { value: "upstream" } });
    fireEvent.change(within(dialog).getByLabelText("Fetch URL"), { target: { value: "../new.git" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote", hidden: true }));
    // Mid-rename (remotes already renamed optimistically): same dialog, same typed values.
    expect(screen.getByRole("dialog", { name: "Edit origin", hidden: true })).toBe(dialog);
    expect(within(dialog).getByLabelText("Fetch URL")).toHaveValue("../new.git");
    if (result) {
      await waitFor(() => expect(screen.queryByRole("dialog", { name: /^Edit /, hidden: true })).toBeNull());
      expect(updateUrls).toHaveBeenCalledWith("upstream", "../new.git", null);
    } else {
      await waitFor(() => expect(within(dialog).getByLabelText("Remote name")).toHaveValue("upstream"));
      await new Promise((resolve) => setTimeout(resolve, 10));
      expect(screen.getByRole("dialog", { name: "Edit origin", hidden: true })).toBe(dialog);
      expect(within(dialog).getByLabelText("Fetch URL")).toHaveValue("../new.git");
      expect(updateUrls).not.toHaveBeenCalled();
    }
  });
});
