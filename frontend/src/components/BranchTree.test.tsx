import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, expectTypeOf, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { BranchTree } from "./BranchTree";

type IsOptional<T, K extends keyof T> = Pick<T, K> extends Required<Pick<T, K>> ? false : true;

const baseBranches = [
  { name: "main", isCurrent: true },
  { name: "feat/foo", isCurrent: false },
];

type BranchTreeProps = Parameters<typeof BranchTree>[0];

function renderTree(overrides: Partial<BranchTreeProps> = {}) {
  const props: BranchTreeProps = {
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
    ...overrides,
  };
  return { ...render(<BranchTree {...props} />), props };
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
    expect(screen.getByText("feat/foo")).toBeInTheDocument();
  });

  it("clicking a non-current branch switches to it", () => {
    const { props } = renderTree();
    fireEvent.click(screen.getByRole("button", { name: "feat/foo" }));
    expect(props.onSwitchBranch).toHaveBeenCalledWith("feat/foo");
  });

  it("right-clicking a branch opens a context menu with Rename/Delete, and Merge for non-current branches", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    expect(screen.getByRole("menuitem", { name: "Rename" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Merge into current branch" })).toBeInTheDocument();
  });

  it("does not offer Merge from the current branch's context menu", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    expect(screen.queryByRole("menuitem", { name: "Merge into current branch" })).not.toBeInTheDocument();
  });

  it("Delete calls onDeleteBranch with force=false, then confirming Force Delete calls it again with force=true", async () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(props.onDeleteBranch).toHaveBeenCalledWith("feat/foo", false);
    fireEvent.click(await screen.findByRole("button", { name: "Force Delete" }));
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

    fireEvent.contextMenu(screen.getByText("feat/foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(props.onDeleteBranch).toHaveBeenCalledWith("feat/foo", false);
    await Promise.resolve();

    // Simulate the optimistic removal: re-render with the deleted branch gone from `branches`.
    rerender(<BranchTree {...props} branches={baseBranches.filter((b) => b.name !== "feat/foo")} />);

    expect(screen.queryByRole("dialog", { name: "Force delete feat/foo" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Force Delete" })).not.toBeInTheDocument();
  });

  it("Rename shows an inline input; Enter calls onRenameBranch", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const input = screen.getByDisplayValue("feat/foo");
    fireEvent.change(input, { target: { value: "feat/bar" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(props.onRenameBranch).toHaveBeenCalledWith("feat/foo", "feat/bar");
  });

  it("Merge into current branch calls onMergeBranch with that branch's name", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Merge into current branch" }));
    expect(props.onMergeBranch).toHaveBeenCalledWith("feat/foo");
  });

  it("disables Rename/Delete/Merge menu items while a rebase is in progress", () => {
    renderTree({ isRebasing: true });
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    expect(screen.getByRole("menuitem", { name: "Rename" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Merge into current branch" })).toBeDisabled();
  });

  it("with no saved graph selection, every branch's graph checkbox is checked by default", () => {
    renderTree();
    expect(screen.getByRole("checkbox", { name: "Show main in graph" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Show feat/foo in graph" })).toBeChecked();
  });

  it("unchecking a branch's graph checkbox while showing all calls onSetGraphBranchSelection with every other branch", () => {
    const { props } = renderTree();
    fireEvent.click(screen.getByRole("checkbox", { name: "Show feat/foo in graph" }));
    expect(props.onSetGraphBranchSelection).toHaveBeenCalledWith(["main"]);
  });

  it("the header '+' menu's New Branch opens the create-branch draft with startPoint HEAD", () => {
    const { props } = renderTree();
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "New Branch…" }));
    expect(props.onOpenCreateBranchDraft).toHaveBeenCalledWith("HEAD");
  });

  it("a branch's context menu Isolate branch calls onSetGraphBranchSelection with only that branch", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByText("feat/foo"));
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
  it("a branch absent from an explicit selection renders unchecked; checking it adds it back", () => {
    const onSetGraphBranchSelection = vi.fn();
    renderTree({ graphBranchSelection: ["main"], onSetGraphBranchSelection });

    expect(screen.getByRole("checkbox", { name: "Show main in graph" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Show feat/foo in graph" })).not.toBeChecked();

    fireEvent.click(screen.getByRole("checkbox", { name: "Show feat/foo in graph" }));

    expect(onSetGraphBranchSelection).toHaveBeenCalledWith(["main", "feat/foo"]);
  });

  // Transformation: open-popover-then-click-Delete-twice → right-click-then-click-Delete-menuitem.
  it("Cancel in the force-delete dialog dismisses it without deleting, leaving Delete in place", async () => {
    const { props } = renderTree();

    fireEvent.contextMenu(screen.getByText("feat/foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    await Promise.resolve();

    const dialog = await screen.findByRole("dialog", { name: "Force delete feat/foo" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(props.onDeleteBranch).toHaveBeenCalledTimes(1);
    expect(props.onDeleteBranch).not.toHaveBeenCalledWith("feat/foo", true);
    expect(screen.queryByRole("dialog", { name: "Force delete feat/foo" })).not.toBeInTheDocument();

    // "leaving Delete in place": right-clicking again still offers Delete, not a stale
    // force-delete state.
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument();
  });

  it("Enter on an empty/whitespace-only rename value does not call onRenameBranch", () => {
    const { props } = renderTree();

    fireEvent.contextMenu(screen.getByText("feat/foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const input = screen.getByDisplayValue("feat/foo");
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(props.onRenameBranch).not.toHaveBeenCalled();
  });

  it("disables the merge action while a merge is already in progress", () => {
    renderTree({ isMerging: true });

    fireEvent.contextMenu(screen.getByText("feat/foo"));

    expect(screen.getByRole("menuitem", { name: "Merge into current branch" })).toBeDisabled();
  });

  it("disables the merge action while another repository operation is pending", () => {
    renderTree({ operationDisabled: true });

    fireEvent.contextMenu(screen.getByText("feat/foo"));

    expect(screen.getByRole("menuitem", { name: "Merge into current branch" })).toBeDisabled();
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("explains why the merge action is disabled via its title", () => {
    renderTree({ operationDisabled: true, operationDisabledReason: "A rebase is in progress." });

    fireEvent.contextMenu(screen.getByText("feat/foo"));

    expect(screen.getByRole("menuitem", { name: "Merge into current branch" })).toHaveAttribute(
      "title",
      "A rebase is in progress.",
    );
  });

  it("does not switch branches while a rebase is in progress", () => {
    const { props } = renderTree({ isRebasing: true });

    fireEvent.click(screen.getByRole("button", { name: "feat/foo" }));

    expect(props.onSwitchBranch).not.toHaveBeenCalled();
  });

  // Replaces BranchSwitcher.test.tsx's "closing the popover clears a pending force-delete..." and
  // "...clears an in-progress rename..." — there is no popover to close in this interaction model.
  // The equivalent guard here is that starting a fresh interaction on another branch clears any
  // stale per-branch edit state rather than leaving it attached to the wrong row.
  it("right-clicking a different branch after starting a rename on one clears that rename's input", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByText("feat/foo"));
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
  const oneRemoteWithPush = [
    {
      name: "origin",
      fetchUrl: "git@github.com:user/repo.git",
      pushUrl: "git@github.com:user/push-repo.git",
      authMode: null,
      authUsername: null,
    },
  ];

  it("lists remote folders and lazily loads their branches on first expand", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main", "feat/foo"]);
    renderTree({ remotes: oneRemote, onListRemoteBranches });
    expect(onListRemoteBranches).not.toHaveBeenCalled();
    const remoteFolder = screen.getByRole("button", { name: "origin" }).closest("li")!;
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(onListRemoteBranches).toHaveBeenCalledWith("origin");
    // Scoped to the remote's own <li>, not `screen`: `baseBranches` (the default local branches)
    // already has a "feat/foo" branch, so an unscoped query would ambiguously match either row.
    expect(await within(remoteFolder).findByText("feat/foo")).toBeInTheDocument();
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

    expect(await within(remoteFolder).findByText("feat/new-on-remote")).toBeInTheDocument();
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
    fireEvent.contextMenu(await within(remoteFolder).findByText("feat/foo"));
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
    fireEvent.contextMenu(await within(remoteFolder).findByText("feat/foo"));
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
    fireEvent.contextMenu(await screen.findByText("feat/only-remote"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Checkout" }));
    await Promise.resolve();
    expect(onCreateBranch).toHaveBeenCalledWith("feat/only-remote", "origin/feat/only-remote");
    expect(onSetUpstream).toHaveBeenCalledWith("origin", "feat/only-remote");
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
    fireEvent.click(screen.getByRole("menuitem", { name: "Add Remote…" }));
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

  it("does not offer Set upstream… or Push on a non-current branch's context menu", () => {
    renderTree({ remotes: oneRemote, upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" } });
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    expect(screen.queryByRole("menuitem", { name: "Set upstream…" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /^Push to/ })).not.toBeInTheDocument();
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

  it("copies the fetch URL to the clipboard from the Edit remote dialog", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    renderTree({ remotes: oneRemote });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit remote" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit origin" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Copy fetch URL for origin" }));
    expect(writeText).toHaveBeenCalledWith("git@github.com:user/repo.git");
  });

  it("copies the push URL to the clipboard when one is set", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    renderTree({ remotes: oneRemoteWithPush });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit remote" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit origin" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Copy push URL for origin" }));
    expect(writeText).toHaveBeenCalledWith("git@github.com:user/push-repo.git");
  });

  it("does not render a push-URL copy button when the remote has no push URL", async () => {
    renderTree({ remotes: oneRemote });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit remote" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit origin" });
    expect(within(dialog).queryByRole("button", { name: "Copy push URL for origin" })).not.toBeInTheDocument();
  });

  it("uses the SSH agent without rendering a token input", async () => {
    const onSetRemoteAuthMode = vi.fn().mockResolvedValue(true);
    renderTree({ remotes: oneRemote, onSetRemoteAuthMode });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Manage credentials" }));
    const dialog = await screen.findByRole("dialog", { name: "Credentials for origin" });
    fireEvent.change(within(dialog).getByLabelText("Authentication for origin"), { target: { value: "SshAgent" } });
    expect(within(dialog).queryByLabelText("Access token")).not.toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Use SSH agent" }));
    await waitFor(() => expect(onSetRemoteAuthMode).toHaveBeenCalledWith("origin", "SshAgent", null));
  });

  it("explains what the SSH agent option does", async () => {
    renderTree({ remotes: oneRemote });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Manage credentials" }));
    const dialog = await screen.findByRole("dialog", { name: "Credentials for origin" });
    fireEvent.change(within(dialog).getByLabelText("Authentication for origin"), { target: { value: "SshAgent" } });
    expect(within(dialog).getByText(/uses your system's ssh agent/i)).toBeInTheDocument();
  });

  it("clears the token after a failed credential save, keeping the dialog open", async () => {
    const onSaveHttpsCredential = vi.fn().mockRejectedValue(new Error("keychain unavailable"));
    renderTree({ remotes: oneRemote, onSaveHttpsCredential });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Manage credentials" }));
    const dialog = await screen.findByRole("dialog", { name: "Credentials for origin" });
    fireEvent.change(within(dialog).getByLabelText("HTTPS username"), { target: { value: "rene" } });
    fireEvent.change(within(dialog).getByLabelText("Access token"), { target: { value: "token-123" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save HTTPS credential" }));
    await waitFor(() => expect(within(dialog).getByLabelText("Access token")).toHaveValue(""));
  });

  it("does not save a token when HTTPS authentication setup fails", async () => {
    const onSetRemoteAuthMode = vi.fn().mockResolvedValue(false);
    const onSaveHttpsCredential = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onSetRemoteAuthMode, onSaveHttpsCredential });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Manage credentials" }));
    const dialog = await screen.findByRole("dialog", { name: "Credentials for origin" });
    fireEvent.change(within(dialog).getByLabelText("HTTPS username"), { target: { value: "rene" } });
    fireEvent.change(within(dialog).getByLabelText("Access token"), { target: { value: "token-123" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save HTTPS credential" }));
    await waitFor(() => expect(onSetRemoteAuthMode).toHaveBeenCalledWith("origin", "HttpsToken", "rene"));
    expect(onSaveHttpsCredential).not.toHaveBeenCalled();
    await waitFor(() => expect(within(dialog).getByLabelText("Access token")).toHaveValue(""));
  });

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

  it("does not update an existing remote's URLs when rename fails, leaving the dialog open", async () => {
    const onRenameRemote = vi.fn().mockResolvedValue(false);
    const onUpdateRemoteUrls = vi.fn().mockResolvedValue(undefined);
    const backup = { name: "backup", fetchUrl: "../backup.git", pushUrl: "../push-backup.git", authMode: null, authUsername: null };
    renderTree({ remotes: [...oneRemote, backup], onRenameRemote, onUpdateRemoteUrls });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit remote" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit origin" });
    fireEvent.change(within(dialog).getByLabelText("Remote name"), { target: { value: "backup" } });
    fireEvent.change(within(dialog).getByLabelText("Fetch URL"), { target: { value: "../replacement.git" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote" }));
    await waitFor(() => expect(onRenameRemote).toHaveBeenCalledWith("origin", "backup"));
    expect(onUpdateRemoteUrls).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "Edit origin" })).toBeInTheDocument();
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
