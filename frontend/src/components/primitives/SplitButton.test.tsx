import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SplitButton } from "./SplitButton";

function renderSplit(overrides: Partial<Parameters<typeof SplitButton>[0]> = {}) {
  const onClick = vi.fn();
  const onAll = vi.fn();
  const onMore = vi.fn();
  render(
    <SplitButton
      label="Stash"
      onClick={onClick}
      menuLabel="More stash options"
      items={[
        { label: "Stash all", onSelect: onAll },
        { label: "Stash…", onSelect: onMore },
      ]}
      {...overrides}
    />,
  );
  return { onClick, onAll, onMore };
}

describe("SplitButton", () => {
  it("runs the main action from the main button", () => {
    const { onClick } = renderSplit();
    fireEvent.click(screen.getByRole("button", { name: "Stash" }));
    expect(onClick).toHaveBeenCalled();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("opens the menu from the chevron, runs the picked item and closes", () => {
    const { onMore, onClick } = renderSplit();
    const chevron = screen.getByRole("button", { name: "More stash options" });
    expect(chevron).toHaveAttribute("aria-haspopup", "menu");
    expect(chevron).toHaveAttribute("aria-expanded", "false");

    fireEvent.click(chevron);
    expect(chevron).toHaveAttribute("aria-expanded", "true");
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Stash all", "Stash…"]);
    fireEvent.click(screen.getByRole("menuitem", { name: "Stash…" }));

    expect(onMore).toHaveBeenCalled();
    expect(onClick).not.toHaveBeenCalled();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("disables both buttons with the title", () => {
    renderSplit({ disabled: true, title: "Nothing to stash" });
    expect(screen.getByRole("button", { name: "Stash" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Stash" })).toHaveAttribute("title", "Nothing to stash");
    expect(screen.getByRole("button", { name: "More stash options" })).toBeDisabled();
  });
});
