import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";
import { ConfirmDialog } from "./ConfirmDialog";

describe("ContextMenu", () => {
  it("renders checked items as menuitemcheckbox with aria-checked", () => {
    const onSelect = vi.fn();
    render(
      <ContextMenu
        x={0}
        y={0}
        onClose={() => {}}
        items={[
          { label: "Plain", onSelect: () => {} },
          { label: "Prune", checked: true, onSelect },
          { label: "Other", checked: false, onSelect: () => {} },
        ]}
      />,
    );
    expect(screen.getByRole("menuitem", { name: "Plain" })).toBeInTheDocument();
    expect(screen.getByRole("menuitemcheckbox", { name: "Prune" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("menuitemcheckbox", { name: "Other" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("menuitem", { name: "Plain" })).not.toHaveAttribute("aria-checked");
    fireEvent.click(screen.getByRole("menuitemcheckbox", { name: "Prune" }));
    expect(onSelect).toHaveBeenCalled();
  });

  it("renders items at the given position and calls onSelect then onClose when one is clicked", () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(
      <ContextMenu
        x={10}
        y={20}
        onClose={onClose}
        items={[{ label: "Branch from here", onSelect }]}
      />,
    );
    const menu = screen.getByRole("menu");
    expect(menu).toHaveStyle({ left: "10px", top: "20px" });
    fireEvent.click(screen.getByRole("menuitem", { name: "Branch from here" }));
    expect(onSelect).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("disables an item marked disabled and does not call onSelect when clicked", () => {
    const onSelect = vi.fn();
    render(
      <ContextMenu x={0} y={0} onClose={() => {}} items={[{ label: "Rebase onto here", onSelect, disabled: true }]} />,
    );
    const item = screen.getByRole("menuitem", { name: "Rebase onto here" });
    expect(item).toBeDisabled();
    fireEvent.click(item);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("marks a destructive item for styling via a data attribute", () => {
    render(
      <ContextMenu x={0} y={0} onClose={() => {}} items={[{ label: "Remove remote", onSelect: () => {}, destructive: true }]} />,
    );
    expect(screen.getByRole("menuitem", { name: "Remove remote" })).toHaveAttribute("data-destructive", "true");
  });

  it("closes on Escape", () => {
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={onClose} items={[{ label: "X", onSelect: () => {} }]} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("closes on a click outside the menu", () => {
    const onClose = vi.fn();
    render(
      <div>
        <button>outside</button>
        <ContextMenu x={0} y={0} onClose={onClose} items={[{ label: "X", onSelect: () => {} }]} />
      </div>,
    );
    fireEvent.mouseDown(screen.getByRole("button", { name: "outside" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("stays open when the pointer leaves it (outside click and Escape close it)", () => {
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={onClose} items={[{ label: "X", onSelect: () => {} }]} />);
    fireEvent.mouseLeave(screen.getByRole("menu"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("clamps itself inside the viewport when opened near the bottom-right edge", () => {
    const rectSpy = vi
      .spyOn(HTMLElement.prototype, "getBoundingClientRect")
      .mockReturnValue({ width: 160, height: 100, top: 0, left: 0, right: 160, bottom: 100, x: 0, y: 0, toJSON: () => ({}) });
    try {
      render(
        <ContextMenu x={window.innerWidth - 10} y={window.innerHeight - 10} onClose={() => {}} items={[{ label: "X", onSelect: () => {} }]} />,
      );
      expect(screen.getByRole("menu")).toHaveStyle({
        left: `${window.innerWidth - 160 - 4}px`,
        top: `${window.innerHeight - 100 - 4}px`,
      });
    } finally {
      rectSpy.mockRestore();
    }
  });

  // WAI-ARIA APG menu pattern — AUD-2026-09-05-FE-001.
  describe("keyboard navigation", () => {
    it("focuses the first item as soon as the menu opens", () => {
      render(
        <ContextMenu
          x={0}
          y={0}
          onClose={() => {}}
          items={[
            { label: "First", onSelect: () => {} },
            { label: "Second", onSelect: () => {} },
          ]}
        />,
      );
      expect(screen.getByRole("menuitem", { name: "First" })).toHaveFocus();
    });

    it("skips a disabled item when placing initial focus", () => {
      render(
        <ContextMenu
          x={0}
          y={0}
          onClose={() => {}}
          items={[
            { label: "First", onSelect: () => {}, disabled: true },
            { label: "Second", onSelect: () => {} },
          ]}
        />,
      );
      expect(screen.getByRole("menuitem", { name: "Second" })).toHaveFocus();
    });

    it("ArrowDown/ArrowUp move focus between items, wrapping at each end", () => {
      render(
        <ContextMenu
          x={0}
          y={0}
          onClose={() => {}}
          items={[
            { label: "First", onSelect: () => {} },
            { label: "Second", onSelect: () => {} },
            { label: "Third", onSelect: () => {} },
          ]}
        />,
      );
      const menu = screen.getByRole("menu");
      expect(screen.getByRole("menuitem", { name: "First" })).toHaveFocus();

      fireEvent.keyDown(menu, { key: "ArrowDown" });
      expect(screen.getByRole("menuitem", { name: "Second" })).toHaveFocus();

      fireEvent.keyDown(menu, { key: "ArrowDown" });
      expect(screen.getByRole("menuitem", { name: "Third" })).toHaveFocus();

      fireEvent.keyDown(menu, { key: "ArrowDown" });
      expect(screen.getByRole("menuitem", { name: "First" })).toHaveFocus();

      fireEvent.keyDown(menu, { key: "ArrowUp" });
      expect(screen.getByRole("menuitem", { name: "Third" })).toHaveFocus();
    });

    it("ArrowDown skips disabled items", () => {
      render(
        <ContextMenu
          x={0}
          y={0}
          onClose={() => {}}
          items={[
            { label: "First", onSelect: () => {} },
            { label: "Second", onSelect: () => {}, disabled: true },
            { label: "Third", onSelect: () => {} },
          ]}
        />,
      );
      const menu = screen.getByRole("menu");
      fireEvent.keyDown(menu, { key: "ArrowDown" });
      expect(screen.getByRole("menuitem", { name: "Third" })).toHaveFocus();
    });

    it("Home/End jump to the first/last item", () => {
      render(
        <ContextMenu
          x={0}
          y={0}
          onClose={() => {}}
          items={[
            { label: "First", onSelect: () => {} },
            { label: "Second", onSelect: () => {} },
            { label: "Third", onSelect: () => {} },
          ]}
        />,
      );
      const menu = screen.getByRole("menu");
      fireEvent.keyDown(menu, { key: "End" });
      expect(screen.getByRole("menuitem", { name: "Third" })).toHaveFocus();

      fireEvent.keyDown(menu, { key: "Home" });
      expect(screen.getByRole("menuitem", { name: "First" })).toHaveFocus();
    });

    it("only one item is in the tab order at a time (roving tabindex)", () => {
      render(
        <ContextMenu
          x={0}
          y={0}
          onClose={() => {}}
          items={[
            { label: "First", onSelect: () => {} },
            { label: "Second", onSelect: () => {} },
          ]}
        />,
      );
      expect(screen.getByRole("menuitem", { name: "First" })).toHaveAttribute("tabindex", "0");
      expect(screen.getByRole("menuitem", { name: "Second" })).toHaveAttribute("tabindex", "-1");

      fireEvent.keyDown(screen.getByRole("menu"), { key: "ArrowDown" });

      expect(screen.getByRole("menuitem", { name: "First" })).toHaveAttribute("tabindex", "-1");
      expect(screen.getByRole("menuitem", { name: "Second" })).toHaveAttribute("tabindex", "0");
    });
  });
});

function Host({ items, onClose = () => {} }: { items: ContextMenuItem[]; onClose?: () => void }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>Trigger</button>
      <button>After</button>
      {open && (
        <ContextMenu
          x={0}
          y={0}
          items={items}
          onClose={() => {
            onClose();
            setOpen(false);
          }}
        />
      )}
    </>
  );
}

function openFromTrigger() {
  const trigger = screen.getByRole("button", { name: "Trigger" });
  trigger.focus();
  fireEvent.click(trigger);
  return trigger;
}

describe("ContextMenu focus and keys", () => {
  it("returns focus to the trigger on Escape", () => {
    render(<Host items={[{ label: "One", onSelect: () => {} }]} />);
    const trigger = openFromTrigger();
    expect(screen.getByRole("menuitem", { name: "One" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("Tab closes the menu, refocuses the trigger and lets the browser move on", () => {
    render(<Host items={[{ label: "One", onSelect: () => {} }]} />);
    const trigger = openFromTrigger();
    const notPrevented = fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key: "Tab" });
    expect(notPrevented).toBe(true);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("refocuses the trigger before running the item, so a dialog it opens returns there", () => {
    function DialogHost() {
      const [dialog, setDialog] = useState(false);
      return (
        <>
          <Host items={[{ label: "Ask", onSelect: () => setDialog(true) }]} />
          {dialog && (
            <ConfirmDialog
              ariaLabel="Sure"
              message="Sure?"
              confirmLabel="Yes"
              onConfirm={() => setDialog(false)}
              onCancel={() => setDialog(false)}
            />
          )}
        </>
      );
    }
    render(<DialogHost />);
    const trigger = openFromTrigger();
    fireEvent.click(screen.getByRole("menuitem", { name: "Ask" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel", hidden: true }));
    expect(screen.queryByRole("dialog", { hidden: true })).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("does not move focus on an outside click", () => {
    render(<Host items={[{ label: "One", onSelect: () => {} }]} />);
    openFromTrigger();
    const after = screen.getByRole("button", { name: "After" });
    fireEvent.mouseDown(after);
    after.focus();
    expect(screen.queryByRole("menu")).toBeNull();
    expect(after).toHaveFocus();
  });

  it("still closes when the trigger was removed meanwhile", () => {
    const trigger = document.createElement("button");
    document.body.appendChild(trigger);
    trigger.focus();
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} items={[{ label: "One", onSelect: () => {} }]} onClose={onClose} />);
    trigger.remove();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it.each(["Enter", " "])("%j activates the focused item once and closes", (key) => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(<Host items={[{ label: "One", onSelect }]} onClose={onClose} />);
    const trigger = openFromTrigger();
    const prevented = !fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key });
    expect(prevented).toBe(true);
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(trigger).toHaveFocus();
  });

  it("Enter on a menu without enabled items does nothing", () => {
    const onSelect = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={() => {}} items={[{ label: "Off", onSelect, disabled: true }]} />);
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Enter" });
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("typeahead jumps to the next enabled item starting with the letter, wrapping", () => {
    render(
      <ContextMenu
        x={0}
        y={0}
        onClose={() => {}}
        items={[
          { label: "Alpha", onSelect: () => {} },
          { label: "Beta", onSelect: () => {} },
          { label: "Blocked", onSelect: () => {}, disabled: true },
          { label: "Bravo", onSelect: () => {} },
        ]}
      />,
    );
    const menu = screen.getByRole("menu");
    fireEvent.keyDown(menu, { key: "b" });
    expect(screen.getByRole("menuitem", { name: "Beta" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "B" });
    expect(screen.getByRole("menuitem", { name: "Bravo" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "b" });
    expect(screen.getByRole("menuitem", { name: "Beta" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "a", ctrlKey: true });
    expect(screen.getByRole("menuitem", { name: "Beta" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "z" });
    expect(screen.getByRole("menuitem", { name: "Beta" })).toHaveFocus();
  });

  it("swallows contextmenu events on its items (no native menu, no second row menu)", () => {
    const onRowMenu = vi.fn();
    render(
      <div onContextMenu={onRowMenu}>
        <ContextMenu x={0} y={0} onClose={() => {}} items={[{ label: "One", onSelect: () => {} }]} />
      </div>,
    );
    const allowed = fireEvent.contextMenu(screen.getByRole("menuitem", { name: "One" }));
    expect(allowed).toBe(false);
    expect(onRowMenu).not.toHaveBeenCalled();
  });

  it("focuses the menu itself when every item is disabled, and Escape still closes", () => {
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={onClose} items={[{ label: "Off", onSelect: () => {}, disabled: true }]} />);
    const menu = screen.getByRole("menu");
    expect(menu).toHaveFocus();
    fireEvent.keyDown(menu, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("returnFocusTo overrides the captured trigger", () => {
    const other = document.createElement("button");
    document.body.appendChild(other);
    const onClose = vi.fn();
    render(
      <ContextMenu x={0} y={0} onClose={onClose} returnFocusTo={other} items={[{ label: "One", onSelect: () => {} }]} />,
    );
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key: "Escape" });
    expect(other).toHaveFocus();
    other.remove();
  });

  it("Escape inside the menu does not reach ancestor key handlers", () => {
    const onAncestorKey = vi.fn();
    render(
      <div onKeyDown={onAncestorKey}>
        <ContextMenu x={0} y={0} onClose={() => {}} items={[{ label: "One", onSelect: () => {} }]} />
      </div>,
    );
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "One" }), { key: "Escape" });
    expect(onAncestorKey).not.toHaveBeenCalled();
  });

  it("swallows the Menu key and Shift+F10 inside the menu", () => {
    const onAncestorKey = vi.fn();
    render(
      <div onKeyDown={onAncestorKey}>
        <ContextMenu x={0} y={0} onClose={() => {}} items={[{ label: "One", onSelect: () => {} }]} />
      </div>,
    );
    const item = screen.getByRole("menuitem", { name: "One" });
    expect(fireEvent.keyDown(item, { key: "ContextMenu" })).toBe(false);
    expect(fireEvent.keyDown(item, { key: "F10", shiftKey: true })).toBe(false);
    expect(onAncestorKey).not.toHaveBeenCalled();
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });
});
