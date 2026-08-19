import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ListRow } from "./ListRow";

describe("ListRow", () => {
  it("renders children and reflects the selected state", () => {
    render(
      <ul>
        <ListRow selected={true} onClick={vi.fn()}>
          row content
        </ListRow>
      </ul>,
    );
    const row = screen.getByText("row content").closest("li");
    expect(row).toHaveAttribute("aria-selected", "true");
  });

  it("omits aria-selected entirely when selected isn't passed", () => {
    render(
      <ul>
        <ListRow onClick={vi.fn()}>row content</ListRow>
      </ul>,
    );
    const row = screen.getByText("row content").closest("li");
    expect(row).not.toHaveAttribute("aria-selected");
  });

  it("calls onClick when clicked", () => {
    const onClick = vi.fn();
    render(
      <ul>
        <ListRow selected={false} onClick={onClick}>
          row content
        </ListRow>
      </ul>,
    );
    fireEvent.click(screen.getByText("row content"));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("calls onContextMenu when provided and right-clicked", () => {
    const onContextMenu = vi.fn();
    render(
      <ul>
        <ListRow selected={false} onClick={vi.fn()} onContextMenu={onContextMenu}>
          row content
        </ListRow>
      </ul>,
    );
    fireEvent.contextMenu(screen.getByText("row content"));
    expect(onContextMenu).toHaveBeenCalledOnce();
  });

  it("is a keyboard-operable button when onClick is given without a selection", () => {
    const onClick = vi.fn();
    render(
      <ul>
        <ListRow onClick={onClick}>row content</ListRow>
      </ul>,
    );
    const row = screen.getByRole("button", { name: "row content" });
    expect(row).toHaveAttribute("tabindex", "0");

    fireEvent.keyDown(row, { key: "Enter" });
    expect(onClick).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(row, { key: " " });
    expect(onClick).toHaveBeenCalledTimes(2);

    fireEvent.keyDown(row, { key: "a" });
    expect(onClick).toHaveBeenCalledTimes(2);
  });

  it("stays out of the tab order when the surrounding list owns selection", () => {
    render(
      <ul>
        <ListRow selected={false} onClick={vi.fn()}>
          row content
        </ListRow>
      </ul>,
    );
    const row = screen.getByText("row content").closest("li");
    expect(row).not.toHaveAttribute("tabindex");
  });

  it("gets role=\"option\" when the surrounding list owns selection, so aria-selected is valid ARIA", () => {
    render(
      <ul>
        <ListRow selected={false} onClick={vi.fn()}>
          row content
        </ListRow>
      </ul>,
    );
    const row = screen.getByText("row content").closest("li");
    expect(row).toHaveAttribute("role", "option");
  });

  it("does not get role=\"option\" in standalone (button) mode", () => {
    render(
      <ul>
        <ListRow onClick={vi.fn()}>row content</ListRow>
      </ul>,
    );
    const row = screen.getByText("row content").closest("li");
    expect(row).not.toHaveAttribute("role", "option");
  });

  it("applies a passed id to the row, for a container to reference via aria-activedescendant", () => {
    render(
      <ul>
        <ListRow id="row-3" selected={true} onClick={vi.fn()}>
          row content
        </ListRow>
      </ul>,
    );
    expect(document.getElementById("row-3")).toHaveTextContent("row content");
  });

  it("renders a plain non-interactive item when no onClick is given", () => {
    render(
      <ul>
        <ListRow>row content</ListRow>
      </ul>,
    );
    const row = screen.getByRole("listitem");
    expect(row).not.toHaveAttribute("tabindex");
    expect(row).not.toHaveAttribute("role");
    fireEvent.click(row);
  });

  it("applies a passed className alongside the row's own styling", () => {
    render(
      <ul>
        <ListRow selected={false} onClick={vi.fn()} className="commit-row">
          row content
        </ListRow>
      </ul>,
    );
    const row = screen.getByText("row content").closest("li");
    expect(row?.classList.contains("commit-row")).toBe(true);
    // The row's own CSS-module class must still be present alongside the passed-in one.
    expect(row?.className.trim().length).toBeGreaterThan("commit-row".length);
  });
});
