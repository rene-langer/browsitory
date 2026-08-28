import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BranchTree } from "./BranchTree";

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
