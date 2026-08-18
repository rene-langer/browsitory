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
});
