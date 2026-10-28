import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
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

  it("ignores Enter and Space bubbling from a nested control", () => {
    const onClick = vi.fn();
    const onDoubleClick = vi.fn();
    render(
      <ul>
        <ListRow onClick={onClick} onDoubleClick={onDoubleClick}>
          <button type="button">inner</button>
        </ListRow>
      </ul>,
    );
    const inner = screen.getByText("inner");
    fireEvent.keyDown(inner, { key: "Enter" });
    fireEvent.keyDown(inner, { key: " " });
    expect(onDoubleClick).not.toHaveBeenCalled();
    expect(onClick).not.toHaveBeenCalled();
  });
});

describe("ListRow onOpenMenu", () => {
  afterEach(() => vi.restoreAllMocks());

  function rect(left: number, top: number, width: number, height: number) {
    return {
      left,
      top,
      right: left + width,
      bottom: top + height,
      width,
      height,
      x: left,
      y: top,
      toJSON: () => ({}),
    } as DOMRect;
  }

  function renderRow(onOpenMenu = vi.fn(), selected?: boolean) {
    render(
      <ul>
        <ListRow onClick={vi.fn()} selected={selected} onOpenMenu={onOpenMenu}>
          row content <button>inner</button> <input aria-label="field" />
        </ListRow>
      </ul>,
    );
    return { onOpenMenu, row: screen.getByText(/row content/).closest("li") as HTMLElement };
  }

  it.each([[{ key: "ContextMenu" }], [{ key: "F10", shiftKey: true }]])("opens at the row's bottom-left on %j", (init) => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(10, 20, 200, 22));
    const { onOpenMenu, row } = renderRow();
    const prevented = !fireEvent.keyDown(row, init);
    expect(prevented).toBe(true);
    expect(onOpenMenu).toHaveBeenCalledWith({ x: 10, y: 42 });
  });

  it("opens from a key pressed on a nested button", () => {
    const { onOpenMenu } = renderRow();
    fireEvent.keyDown(screen.getByRole("button", { name: "inner" }), { key: "ContextMenu" });
    expect(onOpenMenu).toHaveBeenCalledTimes(1);
  });

  it("ignores the key inside a text field", () => {
    const { onOpenMenu } = renderRow();
    fireEvent.keyDown(screen.getByRole("textbox", { name: "field" }), { key: "F10", shiftKey: true });
    expect(onOpenMenu).not.toHaveBeenCalled();
  });

  it("opens at the pointer on right-click", () => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(0, 0, 300, 30));
    const { onOpenMenu, row } = renderRow();
    const allowed = fireEvent.contextMenu(row, { clientX: 50, clientY: 12 });
    expect(allowed).toBe(false);
    expect(onOpenMenu).toHaveBeenCalledWith({ x: 50, y: 12 });
  });

  it("advertises the menu", () => {
    const { row } = renderRow();
    expect(row).toHaveAttribute("aria-haspopup", "menu");
    expect(row).toHaveAttribute("aria-keyshortcuts", "Shift+F10 ContextMenu");
  });

  it("leaves keys to the container on a container-owned row", () => {
    const { onOpenMenu, row } = renderRow(vi.fn(), false);
    expect(row).not.toHaveAttribute("aria-keyshortcuts");
    fireEvent.keyDown(row, { key: "ContextMenu" });
    expect(onOpenMenu).not.toHaveBeenCalled();
    fireEvent.contextMenu(row);
    expect(onOpenMenu).toHaveBeenCalledTimes(1);
  });

  it("still activates with Enter when it has a menu", () => {
    const onClick = vi.fn();
    render(
      <ul>
        <ListRow onClick={onClick} onOpenMenu={vi.fn()}>
          row
        </ListRow>
      </ul>,
    );
    fireEvent.keyDown(screen.getByRole("button", { name: "row" }), { key: "Enter" });
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});
