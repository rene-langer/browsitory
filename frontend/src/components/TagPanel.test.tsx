import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo, TagInfo } from "../ipc/RepoClient";
import { TagPanel } from "./TagPanel";

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
      "Checkout (detached)",
      "Delete from remote…",
      "Delete tag…",
    ]);
    fireEvent.click(screen.getByRole("menuitem", { name: "Push v1.0.0 to backup" }));
    expect(p.onPushTag).toHaveBeenCalledWith("backup", "v1.0.0");

    openMenu("v1.0.0");
    fireEvent.click(screen.getByRole("menuitem", { name: "Branch from tag" }));
    expect(p.onBranchFromCommit).toHaveBeenCalledWith("abc123");

    openMenu("v1.0.0");
    fireEvent.click(screen.getByRole("menuitem", { name: "Checkout (detached)" }));
    expect(p.onCheckoutCommit).toHaveBeenCalledWith("abc123");

    openMenu("v1.0.0");
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete from remote…" }));
    expect(p.onDeleteFromRemote).toHaveBeenCalledWith("v1.0.0");
  });

  it("leaves out the commit actions for a tag that doesn't point at a commit", () => {
    renderPanel({ tags: [{ ...tag, commitId: null }] });
    openMenu("v1.0.0");
    expect(screen.queryByRole("menuitem", { name: "Branch from tag" })).toBeNull();
    expect(screen.queryByRole("menuitem", { name: "Checkout (detached)" })).toBeNull();
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

    for (const name of ["Push v1.0.0 to origin", "Checkout (detached)", "Delete from remote…", "Delete tag…"]) {
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

  it("pushes only selected tags to the selected remote", () => {
    const { props: p } = renderPanel();

    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    fireEvent.click(screen.getByRole("button", { name: "Push selected tags" }));

    expect(p.onPush).toHaveBeenCalledWith("origin", ["v1.0.0"]);
  });

  it("offers a separate all-tags action rather than treating no selection as all tags", () => {
    const { props: p } = renderPanel();

    expect(screen.getByRole("button", { name: "Push selected tags" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Push all tags" }));

    expect(p.onPush).toHaveBeenCalledWith("origin", []);
  });

  it("selects a newly available remote after the remotes refresh", () => {
    const onPush = vi.fn();
    const { rerender } = renderPanel({ remotes: [], onPush });

    rerender(<TagPanel {...props({ remotes: [origin, backup], onPush })} />);
    fireEvent.click(screen.getByRole("button", { name: "Push all tags" }));

    expect(onPush).toHaveBeenCalledWith("origin", []);
  });

  it("prunes selections for tags removed by a refresh", () => {
    const onPush = vi.fn();
    const { rerender } = renderPanel({ onPush });

    fireEvent.click(screen.getByRole("checkbox", { name: "Select v1.0.0" }));
    rerender(<TagPanel {...props({ tags: [], onPush })} />);

    expect(screen.getByRole("button", { name: "Push selected tags" })).toBeDisabled();
  });

  // Disabled buttons went inert with no explanation — issue #31/UX-003.
  it("disables the push controls while an operation runs and explains why", () => {
    renderPanel({ pushDisabled: true, operationDisabledReason: "A transfer is in progress." });

    for (const name of ["Push selected tags", "Push all tags"]) {
      const button = screen.getByRole("button", { name });
      expect(button).toBeDisabled();
      expect(button).toHaveAttribute("title", "A transfer is in progress.");
    }
  });
});
