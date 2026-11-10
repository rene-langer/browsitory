import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { StashEntry } from "../ipc/RepoClient";
import { StashPanel } from "./StashPanel";

// Deliberately distinct message stems so `getByText` on one never matches the other.
const stashes: StashEntry[] = [
  { index: 0, message: "WIP on main: stash0fix uncommitted edit", commitId: "stash0" },
  { index: 1, message: "WIP on main: stash1fix earlier edit", commitId: "stash1" },
];

function panel(overrides: Partial<Parameters<typeof StashPanel>[0]> = {}) {
  return (
    <StashPanel
      stashes={stashes}
      onSelectRow={vi.fn()}
      onApplyStash={vi.fn()}
      onPopStash={vi.fn()}
      onDropStash={vi.fn()}
      restoreIndex={true}
      onSetRestoreIndex={vi.fn()}
      blockers={{ apply: null, drop: null }}
      {...overrides}
    />
  );
}

function renderPanel(overrides: Partial<Parameters<typeof StashPanel>[0]> = {}) {
  localStorage.removeItem("sidebar-stashes");
  const result = render(panel(overrides));
  fireEvent.click(screen.getByRole("button", { name: "Stashes" }));
  return result;
}

const three: StashEntry[] = [
  ...stashes,
  { index: 2, message: "WIP on main: stash2fix oldest edit", commitId: "stash2" },
];

/** The list with `index` matching each entry's place, as the backend reports it. */
function renumber(list: StashEntry[]): StashEntry[] {
  return list.map((stash, index) => ({ ...stash, index }));
}

/** The "…" button of the stash whose message contains `stem`. */
function actionsFor(stem: string) {
  const row = screen.getByText(new RegExp(stem)).closest("li")!;
  return within(row).getByRole("button", { name: /^Actions for / });
}

/** Opens the actions menu of the stash row whose message matches `message`. */
function openMenu(message: RegExp) {
  const row = screen.getByText(message).closest("li")!;
  fireEvent.click(within(row).getByRole("button", { name: /^Actions for / }));
}

describe("StashPanel", () => {
  it("shows an empty state when there are no stashes", () => {
    renderPanel({ stashes: [] });

    expect(screen.getByText(/No stashes/)).toBeInTheDocument();
  });

  it("renders each stash's message", () => {
    renderPanel();

    expect(screen.getByText(/WIP on main: stash0fix uncommitted edit/)).toBeInTheDocument();
    expect(screen.getByText(/WIP on main: stash1fix earlier edit/)).toBeInTheDocument();
  });

  it("clicking a stash row calls onSelectRow with its commit id", () => {
    const onSelectRow = vi.fn();
    renderPanel({ onSelectRow });

    fireEvent.click(screen.getByText(/stash0fix/).closest("li")!);

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "stash0" });
  });

  it("shows the full stash message as a tooltip, since a long one is cut off", () => {
    renderPanel();

    expect(screen.getByText("WIP on main: stash0fix uncommitted edit")).toHaveAttribute(
      "title",
      "WIP on main: stash0fix uncommitted edit",
    );
  });

  it("gives each row one named actions button that opens a menu, not inline buttons", () => {
    renderPanel();

    const row = screen.getByText(/stash0fix/).closest("li")!;
    const trigger = within(row).getByRole("button", { name: "Actions for WIP on main: stash0fix uncommitted edit" });
    expect(trigger).toHaveAttribute("aria-haspopup", "menu");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(within(row).queryByRole("button", { name: "Apply" })).not.toBeInTheDocument();

    fireEvent.click(trigger);

    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("applying from the menu calls onApplyStash with its index, not onSelectRow", () => {
    const onApplyStash = vi.fn();
    const onSelectRow = vi.fn();
    renderPanel({ onApplyStash, onSelectRow });

    openMenu(/stash0fix/);
    fireEvent.click(screen.getByRole("menuitem", { name: "Apply" }));

    expect(onApplyStash).toHaveBeenCalledWith(0);
    expect(onSelectRow).not.toHaveBeenCalled();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("a second click on the open menu's button closes it", () => {
    renderPanel();

    const row = screen.getByText(/stash0fix/).closest("li")!;
    const trigger = within(row).getByRole("button", { name: /^Actions for / });
    fireEvent.click(trigger);
    expect(screen.getByRole("menu")).toBeInTheDocument();

    // A real click: the mousedown must not count as "outside" and close-then-reopen the menu.
    fireEvent.mouseDown(trigger);
    fireEvent.click(trigger);

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("a click elsewhere on the row closes the menu opened from its button", () => {
    renderPanel();

    const row = screen.getByText(/stash0fix/).closest("li")!;
    fireEvent.click(within(row).getByRole("button", { name: /^Actions for / }));
    fireEvent.mouseDown(screen.getByText(/stash1fix/));

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("a click on '…' closes the menu that Shift+F10 on its row opened", () => {
    renderPanel();

    const row = screen.getByText(/stash0fix/).closest("li")!;
    fireEvent.keyDown(row, { key: "F10", shiftKey: true });
    const trigger = within(row).getByRole("button", { name: /^Actions for / });
    expect(trigger).toHaveAttribute("aria-expanded", "true");

    fireEvent.mouseDown(trigger);
    fireEvent.click(trigger);

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("a click on '…' closes the menu that a right-click on its row opened", () => {
    renderPanel();

    const row = screen.getByText(/stash0fix/).closest("li")!;
    fireEvent.contextMenu(row);
    const trigger = within(row).getByRole("button", { name: /^Actions for / });

    fireEvent.mouseDown(trigger);
    fireEvent.click(trigger);

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("a right-button press on '…' closes its open menu", () => {
    renderPanel();

    const row = screen.getByText(/stash0fix/).closest("li")!;
    const trigger = within(row).getByRole("button", { name: /^Actions for / });
    fireEvent.click(trigger);
    fireEvent.mouseDown(trigger, { button: 2 });

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("opens the menu on right-click of the row", () => {
    renderPanel();

    fireEvent.contextMenu(screen.getByText(/stash1fix/).closest("li")!);

    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Apply", "Pop", "Drop…"]);
  });

  it("Escape closes the menu and returns focus to its button", () => {
    renderPanel();

    const row = screen.getByText(/stash0fix/).closest("li")!;
    const trigger = within(row).getByRole("button", { name: /^Actions for / });
    fireEvent.click(trigger);
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Apply" }), { key: "Escape" });

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("acts on the stash the menu was opened for when the list shifts while it is open", () => {
    const onApplyStash = vi.fn();
    const { rerender } = renderPanel({ onApplyStash });
    openMenu(/stash1fix/);

    // A new stash pushed on top moves the opened one from index 1 to index 2.
    const shifted: StashEntry[] = [{ index: 0, message: "WIP on main: newest", commitId: "stashN" }].concat(
      stashes.map((stash) => ({ ...stash, index: stash.index + 1 })),
    );
    rerender(panel({ onApplyStash, stashes: shifted }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Apply" }));

    expect(onApplyStash).toHaveBeenCalledWith(2);
  });

  it("closes the menu when its stash disappears from the list", () => {
    const { rerender } = renderPanel();
    openMenu(/stash1fix/);

    rerender(panel({ stashes: [stashes[0]] }));

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("moves focus to the '…' now in that place when the stash of a focused menu disappears", () => {
    const { rerender } = renderPanel({ stashes: three });
    openMenu(/stash1fix/);
    expect(screen.getByRole("menuitem", { name: "Apply" })).toHaveFocus();

    rerender(panel({ stashes: renumber([three[0], three[2]]) }));

    expect(actionsFor("stash2fix")).toHaveFocus();
  });

  it("moves focus to the last '…' when the last stash of a focused menu disappears", () => {
    const { rerender } = renderPanel({ stashes: three });
    openMenu(/stash2fix/);

    rerender(panel({ stashes: renumber([three[0], three[1]]) }));

    expect(actionsFor("stash1fix")).toHaveFocus();
  });

  it("finds that place in the latest list, after refreshes that moved the stash", () => {
    const { rerender } = renderPanel({ stashes: three });
    openMenu(/stash1fix/);
    const newest = { index: 0, message: "WIP on main: stashNfix newest", commitId: "stashN" };

    // A new stash on top moves stash1 from place 1 to place 2, then stash1 goes.
    rerender(panel({ stashes: renumber([newest, ...three]) }));
    rerender(panel({ stashes: renumber([newest, three[0], three[2]]) }));

    expect(actionsFor("stash2fix")).toHaveFocus();
  });

  it("moves focus to the '…' now in that place when the stash of a focused drop dialog disappears", () => {
    const { rerender } = renderPanel({ stashes: three });
    openMenu(/stash1fix/);
    fireEvent.click(screen.getByRole("menuitem", { name: "Drop…" }));
    expect(screen.getByRole("dialog", { name: /Drop stash/ })).toBeInTheDocument();

    rerender(panel({ stashes: renumber([three[0], three[2]]) }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(actionsFor("stash2fix")).toHaveFocus();
  });

  it("moves focus to the '…' now in that place once a confirmed drop removes the stash", () => {
    // Still running: the app removes the stash (optimistically) before the drop settles.
    const onDropStash = vi.fn(() => new Promise<void>(() => {}));
    const { rerender } = renderPanel({ stashes: three, onDropStash });
    openMenu(/stash1fix/);
    fireEvent.click(screen.getByRole("menuitem", { name: "Drop…" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: /Drop stash/ })).getByRole("button", { name: "Drop stash" }));
    expect(onDropStash).toHaveBeenCalledWith(1);

    rerender(panel({ stashes: renumber([three[0], three[2]]), onDropStash }));

    expect(actionsFor("stash2fix")).toHaveFocus();
  });

  it("leaves focus elsewhere alone when a watched stash disappears", () => {
    const textarea = document.createElement("textarea");
    document.body.appendChild(textarea);
    try {
      const onDropStash = vi.fn(() => new Promise<void>(() => {}));
      const { rerender } = renderPanel({ stashes: three, onDropStash });
      openMenu(/stash1fix/);
      fireEvent.click(screen.getByRole("menuitem", { name: "Drop…" }));
      fireEvent.click(within(screen.getByRole("dialog", { name: /Drop stash/ })).getByRole("button", { name: "Drop stash" }));
      textarea.focus();

      rerender(panel({ stashes: renumber([three[0], three[2]]), onDropStash }));

      expect(textarea).toHaveFocus();
    } finally {
      textarea.remove();
    }
  });

  it("stops watching a drop that settled without removing the stash", async () => {
    const onDropStash = vi.fn(async () => {});
    const { rerender } = renderPanel({ stashes: three, onDropStash });
    openMenu(/stash1fix/);
    fireEvent.click(screen.getByRole("menuitem", { name: "Drop…" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: /Drop stash/ })).getByRole("button", { name: "Drop stash" }));
    await act(async () => {});

    // Later, something else (another window) removes it: not this panel's doing.
    rerender(panel({ stashes: renumber([three[0], three[2]]), onDropStash }));

    expect(document.body).toHaveFocus();
  });

  it("moves focus to the section header when the last stash of a focused menu disappears", () => {
    const { rerender } = renderPanel();
    openMenu(/stash0fix/);

    rerender(panel({ stashes: [] }));

    expect(screen.getByRole("button", { name: /^Stashes/ })).toHaveFocus();
  });

  it("opens the same menu with Shift+F10 on the row", () => {
    renderPanel();

    const row = screen.getByText(/stash1fix/).closest("li")!;
    fireEvent.keyDown(row, { key: "F10", shiftKey: true });

    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Apply", "Pop", "Drop…"]);
  });

  it("Drop… opens a confirmation naming the stash; confirming calls onDropStash with its index", () => {
    const onDropStash = vi.fn();
    const onSelectRow = vi.fn();
    renderPanel({ onDropStash, onSelectRow });

    openMenu(/stash1fix/);
    fireEvent.click(screen.getByRole("menuitem", { name: "Drop…" }));
    expect(onDropStash).not.toHaveBeenCalled();
    expect(onSelectRow).not.toHaveBeenCalled();

    const dialog = screen.getByRole("dialog", { name: /Drop stash/ });
    expect(within(dialog).getByText(/stash1fix earlier edit/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Drop stash" }));

    expect(onDropStash).toHaveBeenCalledWith(1);
  });

  it("Cancel in the drop confirmation dismisses it without dropping", () => {
    const onDropStash = vi.fn();
    renderPanel({ onDropStash });

    openMenu(/stash0fix/);
    fireEvent.click(screen.getByRole("menuitem", { name: "Drop…" }));
    const dialog = screen.getByRole("dialog", { name: /Drop stash/ });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(onDropStash).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("disables every menu action, with the reason as its title, while a transfer runs", () => {
    const reason = "A transfer is in progress.";
    renderPanel({ blockers: { apply: reason, drop: reason } });

    openMenu(/stash0fix/);
    for (const name of ["Apply", "Pop", "Drop…"]) {
      const item = screen.getByRole("menuitem", { name });
      expect(item).toBeDisabled();
      expect(item).toHaveAttribute("title", reason);
    }
  });

  it("keeps Drop… during a paused merge, which only blocks Apply and Pop (#521)", () => {
    const onDropStash = vi.fn();
    renderPanel({ onDropStash, blockers: { apply: "A merge is in progress.", drop: null } });

    openMenu(/stash0fix/);
    expect(screen.getByRole("menuitem", { name: "Apply" })).toBeDisabled();
    fireEvent.click(screen.getByRole("menuitem", { name: "Drop…" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: /Drop stash/ })).getByRole("button", { name: "Drop stash" }));

    expect(onDropStash).toHaveBeenCalledWith(0);
  });

  it("disables the open Drop confirmation, saying why, once an action is in progress (#521)", () => {
    const onDropStash = vi.fn();
    const { rerender } = renderPanel({ onDropStash });
    openMenu(/stash0fix/);
    fireEvent.click(screen.getByRole("menuitem", { name: "Drop…" }));

    rerender(panel({ onDropStash, blockers: { apply: null, drop: "Another action is in progress." } }));
    const dialog = screen.getByRole("dialog", { name: /Drop stash/ });
    const confirm = within(dialog).getByRole("button", { name: "Drop stash" });
    expect(confirm).toBeDisabled();
    expect(confirm).toHaveAttribute("title", "Another action is in progress.");
    expect(dialog).toHaveTextContent("Another action is in progress.");
    fireEvent.click(confirm);

    expect(onDropStash).not.toHaveBeenCalled();
  });
});

describe("StashPanel pop and restore staged changes", () => {
  it("pops a stash with its index, not selecting it", () => {
    const onPopStash = vi.fn();
    const onSelectRow = vi.fn();
    renderPanel({ onPopStash, onSelectRow });

    openMenu(/stash1fix/);
    fireEvent.click(screen.getByRole("menuitem", { name: "Pop" }));

    expect(onPopStash).toHaveBeenCalledWith(1);
    expect(onSelectRow).not.toHaveBeenCalled();
  });

  it("shows and toggles Restore staged changes", () => {
    const onSetRestoreIndex = vi.fn();
    renderPanel({ restoreIndex: true, onSetRestoreIndex });

    const checkbox = screen.getByRole("checkbox", { name: "Restore staged changes" });
    expect(checkbox).toBeChecked();
    fireEvent.click(checkbox);

    expect(onSetRestoreIndex).toHaveBeenCalledWith(false);
  });

  it("disables Apply and Pop, not Drop, with the apply-disabled reason", () => {
    renderPanel({ blockers: { apply: "Unstage or commit staged changes first.", drop: null } });

    openMenu(/stash0fix/);
    for (const name of ["Apply", "Pop"]) {
      const item = screen.getByRole("menuitem", { name });
      expect(item).toBeDisabled();
      expect(item).toHaveAttribute("title", "Unstage or commit staged changes first.");
    }
    expect(screen.getByRole("menuitem", { name: "Drop…" })).toBeEnabled();
  });
});
