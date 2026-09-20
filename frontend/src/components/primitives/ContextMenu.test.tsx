import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ContextMenu } from "./ContextMenu";

describe("ContextMenu", () => {
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
