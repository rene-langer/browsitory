import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo, TagInfo } from "../ipc/RepoClient";
import { TagPanel } from "./TagPanel";
import { describedByText } from "../test/describedBy";

const tag: TagInfo = {
  name: "v1.0.0",
  targetId: "abc123",
  commitId: "abc123",
  annotated: false,
  message: null,
  taggerName: null,
  timestamp: null,
};

const origin: RemoteInfo = {
  name: "origin",
  fetchUrl: "../origin.git",
  pushUrl: null,
  authMode: null,
  authUsername: null,
};

const backup: RemoteInfo = {
  name: "backup",
  fetchUrl: "../backup.git",
  pushUrl: null,
  authMode: null,
  authUsername: null,
};

type Props = Parameters<typeof TagPanel>[0];

function props(overrides: Partial<Props> = {}): Props {
  return {
    tags: [tag],
    remotes: [origin],
    onDelete: vi.fn().mockResolvedValue(undefined),
    onDeleteFromRemote: vi.fn(),
    onPush: vi.fn(),
    onPushTag: vi.fn(),
    onBranchFromCommit: vi.fn(),
    onCheckoutCommit: vi.fn(),
    onGoToTag: vi.fn(),
    pushDisabled: false,
    operationDisabledReason: null,
    ...overrides,
  };
}

function renderPanel(overrides: Partial<Props> = {}) {
  localStorage.removeItem("sidebar-tags");
  const all = props(overrides);
  const result = render(<TagPanel {...all} />);
  fireEvent.click(screen.getByRole("button", { name: "Tags" }));
  return { ...result, props: all };
}

/** The row of tag `name` (a `role="button"` `<li>`, see `ListRow`). */
const tagRow = (name: string) =>
  screen.getByRole("button", { name: `Actions for ${name}` }).closest("li") as HTMLElement;

const openMenu = (name: string) => fireEvent.click(screen.getByRole("button", { name: `Actions for ${name}` }));

describe("TagPanel", () => {
  // #438: tags are created from the graph and the palette; the panel is the tag list.
  it("has no create form and points to where tags are created when there are none", () => {
    renderPanel({ tags: [] });

    expect(screen.queryByRole("form", { name: "Create tag" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Create tag" })).toBeNull();
    expect(screen.getByText(/choose Create tag… in its menu in the graph/)).toBeInTheDocument();
  });

  it("shows one compact row per tag: checkbox, name in its own element, kind and an actions button", () => {
    renderPanel({ tags: [tag, { ...tag, name: "v2", annotated: true }] });

    const rows = Array.from(screen.getByRole("list", { name: "Tags" }).children) as HTMLElement[];
    expect(rows).toHaveLength(2);
    // A focusable row: a click or Enter goes to the tag's commit (#441).
    expect(rows[0]).toHaveAttribute("role", "button");
    expect(within(rows[0]).getByText("v1.0.0").tagName).toBe("SPAN");
    expect(within(rows[0]).getByText("Lightweight")).toBeInTheDocument();
    expect(within(rows[1]).getByText("Annotated")).toBeInTheDocument();
    // No inline action buttons besides the menu button.
    expect(within(rows[0]).getAllByRole("button")).toHaveLength(1);
    expect(within(rows[0]).getByRole("button", { name: "Actions for v1.0.0" })).toHaveAttribute("aria-haspopup", "menu");
  });

  it("filters the list by name, case-insensitively", () => {
    renderPanel({ tags: [tag, { ...tag, name: "release-2" }, { ...tag, name: "V1.1" }] });

    fireEvent.change(screen.getByLabelText("Filter tags"), { target: { value: "v1" } });

    const list = screen.getByRole("list", { name: "Tags" });
    expect(within(list).getByText("v1.0.0")).toBeInTheDocument();
    expect(within(list).getByText("V1.1")).toBeInTheDocument();
    expect(within(list).queryByText("release-2")).toBeNull();

    fireEvent.change(screen.getByLabelText("Filter tags"), { target: { value: "nothing" } });
    expect(screen.getByText("No tags match “nothing”.")).toBeInTheDocument();
  });

  it("offers the tag badge menu's actions in the row menu", () => {
    const { props: p } = renderPanel({ remotes: [origin, backup] });

    openMenu("v1.0.0");
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Push v1.0.0 to origin",
      "Push v1.0.0 to backup",
      "Branch from tag",
      "Switch to (detached)",
      "Copy tag name",
      "Delete from remote…",
      "Delete tag…",
    ]);
    fireEvent.click(screen.getByRole("menuitem", { name: "Push v1.0.0 to backup" }));
    expect(p.onPushTag).toHaveBeenCalledWith("backup", "v1.0.0");

    openMenu("v1.0.0");
    fireEvent.click(screen.getByRole("menuitem", { name: "Branch from tag" }));
    expect(p.onBranchFromCommit).toHaveBeenCalledWith("abc123");

    openMenu("v1.0.0");
    fireEvent.click(screen.getByRole("menuitem", { name: "Switch to (detached)" }));
    expect(p.onCheckoutCommit).toHaveBeenCalledWith("abc123");

    openMenu("v1.0.0");
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete from remote…" }));
    expect(p.onDeleteFromRemote).toHaveBeenCalledWith("v1.0.0");
  });

  it("copies the tag name, even while an operation runs (#614)", () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    renderPanel({ pushDisabled: true, operationDisabledReason: "A transfer is in progress." });

    openMenu("v1.0.0");
    const item = screen.getByRole("menuitem", { name: "Copy tag name" });
    expect(item).toBeEnabled();
    fireEvent.click(item);

    expect(writeText).toHaveBeenCalledWith("v1.0.0");
  });

  it("leaves out the commit actions for a tag that doesn't point at a commit", () => {
    renderPanel({ tags: [{ ...tag, commitId: null }] });
    openMenu("v1.0.0");
    expect(screen.queryByRole("menuitem", { name: "Branch from tag" })).toBeNull();
    expect(screen.queryByRole("menuitem", { name: "Switch to (detached)" })).toBeNull();
  });

  it("selects the tag's commit when its row is clicked or activated by key (#441)", () => {
    const { props: given } = renderPanel();
    const row = tagRow("v1.0.0");

    expect(row).toHaveAttribute("tabindex", "0");
    fireEvent.click(row);
    fireEvent.keyDown(row, { key: "Enter" });
    fireEvent.keyDown(row, { key: " " });
    expect(given.onGoToTag).toHaveBeenCalledTimes(3);
    expect(given.onGoToTag).toHaveBeenCalledWith(tag);

    // The checkbox and the "…" keep their own meaning.
    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    openMenu("v1.0.0");
    expect(given.onGoToTag).toHaveBeenCalledTimes(3);
  });

  it("opens the row menu from the focused row with Shift+F10 or the Menu key", () => {
    renderPanel();
    const row = tagRow("v1.0.0");

    expect(row).toHaveAttribute("aria-keyshortcuts", "Shift+F10 ContextMenu");
    fireEvent.keyDown(row, { key: "F10", shiftKey: true });
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    fireEvent.keyDown(row, { key: "ContextMenu" });
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("opens the row menu by right-click and from the keyboard (Shift+F10, Menu key)", () => {
    renderPanel();
    const row = screen.getByRole("list", { name: "Tags" }).querySelector("li")!;

    fireEvent.contextMenu(row);
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();

    fireEvent.keyDown(screen.getByRole("button", { name: "Actions for v1.0.0" }), { key: "F10", shiftKey: true });
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

    fireEvent.keyDown(screen.getByRole("checkbox", { name: "Select v1.0.0" }), { key: "ContextMenu" });
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("disables Delete from remote… without a remote, and explains why", () => {
    renderPanel({ remotes: [] });
    openMenu("v1.0.0");

    const item = screen.getByRole("menuitem", { name: "Delete from remote…" });
    expect(item).toBeDisabled();
    expect(item).toHaveAttribute("title", "No remote configured.");
  });

  it("disables the mutating menu items while an operation runs, with the reason as their title", () => {
    renderPanel({ pushDisabled: true, operationDisabledReason: "A transfer is in progress." });
    openMenu("v1.0.0");

    for (const name of ["Push v1.0.0 to origin", "Switch to (detached)", "Delete from remote…", "Delete tag…"]) {
      const item = screen.getByRole("menuitem", { name });
      expect(item).toBeDisabled();
      expect(item).toHaveAttribute("title", "A transfer is in progress.");
    }
  });

  it("deletes only after the local-delete confirmation", () => {
    const { props: p } = renderPanel();

    openMenu("v1.0.0");
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete tag…" }));
    const dialog = screen.getByRole("dialog", { name: "Delete local tag v1.0.0" });
    expect(p.onDelete).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete tag" }));

    expect(p.onDelete).toHaveBeenCalledWith("v1.0.0");
  });

  it("has no push form until tags are ticked, then pushes them from the selection bar (#674)", () => {
    const { props: p } = renderPanel();

    expect(screen.queryByRole("group", { name: "Selected tags" })).toBeNull();
    expect(screen.queryByRole("button", { name: /^Push/ })).toBeNull();
    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    const bar = screen.getByRole("group", { name: "Selected tags" });
    expect(bar).toHaveTextContent("1 selected");
    // One remote: no picker, the button names it.
    expect(within(bar).queryByRole("combobox")).toBeNull();
    fireEvent.click(within(bar).getByRole("button", { name: "Push to origin" }));

    expect(p.onPush).toHaveBeenCalledWith("origin", ["v1.0.0"]);
  });

  it("picks the remote in the selection bar when there are several", () => {
    const { props: p } = renderPanel({ remotes: [origin, backup] });

    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    const bar = screen.getByRole("group", { name: "Selected tags" });
    fireEvent.change(within(bar).getByRole("combobox", { name: "Remote" }), { target: { value: "backup" } });
    fireEvent.click(within(bar).getByRole("button", { name: "Push" }));

    expect(p.onPush).toHaveBeenCalledWith("backup", ["v1.0.0"]);
  });

  it("keeps the remote picker usable while pushing is blocked: the Push button says why (#748)", () => {
    renderPanel({ remotes: [origin, backup], pushDisabled: true, operationDisabledReason: "A transfer is in progress." });

    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    const bar = screen.getByRole("group", { name: "Selected tags" });
    const picker = within(bar).getByRole("combobox", { name: "Remote" });
    expect(picker).toBeEnabled();
    expect(picker).not.toHaveAttribute("title");
    expect(describedByText(within(bar).getByRole("button", { name: "Push" }))).toBe("A transfer is in progress.");
  });

  it("says why an open delete confirmation can't delete once the repository is busy (#748)", () => {
    const { props: p, rerender } = renderPanel();
    openMenu("v1.0.0");
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete tag…" }));
    rerender(<TagPanel {...p} pushDisabled operationDisabledReason="A transfer is in progress." />);
    const dialog = screen.getByRole("dialog", { name: "Delete local tag v1.0.0" });
    const confirm = within(dialog).getByRole("button", { name: "Delete tag" });
    expect(confirm).toBeDisabled();
    expect(describedByText(confirm)).toBe("A transfer is in progress.");
    expect(within(dialog).getAllByText("A transfer is in progress.")).toHaveLength(1);
    fireEvent.click(confirm);
    expect(p.onDelete).not.toHaveBeenCalled();
  });

  it("clears the selection, which hides the bar", () => {
    renderPanel();

    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    fireEvent.click(screen.getByRole("button", { name: "Clear selection" }));

    expect(screen.getByRole("checkbox", { name: "Select v1.0.0" })).not.toBeChecked();
    expect(screen.queryByRole("group", { name: "Selected tags" })).toBeNull();
  });

  it("selects every tag the filter shows with Select all, and only those", () => {
    const tags = ["v1.0.0", "v1.1.0", "v2.0.0"].map((name) => ({ ...tag, name }));
    const { props: p } = renderPanel({ tags });

    fireEvent.change(screen.getByRole("searchbox", { name: "Filter tags" }), { target: { value: "v1." } });
    const selectAll = screen.getByRole("checkbox", { name: "Select all shown" });
    fireEvent.click(selectAll);
    expect(selectAll).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "Push to origin" }));
    expect(p.onPush).toHaveBeenCalledWith("origin", ["v1.0.0", "v1.1.0"]);

    // Some but not all shown: mixed; a click selects the rest, the next clears them.
    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.1.0" }));
    expect(selectAll).not.toBeChecked();
    expect(selectAll).toHaveProperty("indeterminate", true);
    fireEvent.click(selectAll);
    expect(selectAll).toBeChecked();
    fireEvent.click(selectAll);
    expect(screen.queryByRole("group", { name: "Selected tags" })).toBeNull();
  });

  it("selects a newly available remote after the remotes refresh", () => {
    const onPush = vi.fn();
    const { rerender } = renderPanel({ remotes: [], onPush });

    rerender(<TagPanel {...props({ remotes: [origin, backup], onPush })} />);
    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    fireEvent.click(screen.getByRole("button", { name: "Push" }));

    expect(onPush).toHaveBeenCalledWith("origin", ["v1.0.0"]);
  });

  it("says why it can't push without a remote", () => {
    const { props: p } = renderPanel({ remotes: [] });

    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    const push = screen.getByRole("button", { name: "Push" });
    expect(push).toHaveAttribute("aria-disabled", "true");
    expect(push).toHaveAccessibleDescription("Add a remote to push tags to.");
    fireEvent.click(push);
    expect(p.onPush).not.toHaveBeenCalled();
  });

  it("prunes selections for tags removed by a refresh", () => {
    const onPush = vi.fn();
    const { rerender } = renderPanel({ onPush });

    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    rerender(<TagPanel {...props({ tags: [], onPush })} />);

    expect(screen.queryByRole("group", { name: "Selected tags" })).toBeNull();
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003, #664.
  it("blocks pushing while an operation runs and says why", () => {
    const { props: p } = renderPanel({ pushDisabled: true, operationDisabledReason: "A transfer is in progress." });

    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    const push = screen.getByRole("button", { name: "Push to origin" });
    expect(push).toHaveAttribute("aria-disabled", "true");
    expect(push).toHaveAccessibleDescription("A transfer is in progress.");
    fireEvent.click(push);
    expect(p.onPush).not.toHaveBeenCalled();
  });
});

describe("TagPanel — Create tag at HEAD… (#649)", () => {
  it("offers a + in the header that opens Create tag at HEAD, also with no tags yet", () => {
    const onCreateTagAtHead = vi.fn();
    render(<TagPanel {...props({ tags: [], onCreateTagAtHead })} />);
    fireEvent.click(screen.getByRole("button", { name: "Create tag at HEAD…" }));
    expect(onCreateTagAtHead).toHaveBeenCalledOnce();
  });

  it("has no + without a HEAD commit", () => {
    render(<TagPanel {...props()} />);
    expect(screen.queryByRole("button", { name: "Create tag at HEAD…" })).not.toBeInTheDocument();
  });
});
