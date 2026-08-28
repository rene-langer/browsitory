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

  it("closes on mouse leave, matching the menu it replaces in CommitGraph", () => {
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={onClose} items={[{ label: "X", onSelect: () => {} }]} />);
    fireEvent.mouseLeave(screen.getByRole("menu"));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
