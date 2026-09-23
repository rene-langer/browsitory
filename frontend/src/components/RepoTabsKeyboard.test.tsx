import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RepoTabs } from "./RepoTabs";
import { repoPanelId } from "./repoTabIds";

const repos = [
  { path: "/repos/a", displayName: "a", workspaceId: null },
  { path: "/repos/b", displayName: "b", workspaceId: null },
  { path: "/repos/c", displayName: "c", workspaceId: null },
];

function setup(activePath = "/repos/b", onClose = vi.fn(), busyPaths: ReadonlySet<string> = new Set()) {
  const onSwitchTo = vi.fn();
  render(
    <RepoTabs
      openRepos={repos}
      activePath={activePath}
      busyPaths={busyPaths}
      workspaceNames={{}}
      onSwitchTo={onSwitchTo}
      onClose={onClose}
      onCloseGroup={vi.fn()}
      onAddTab={vi.fn()}
    />,
  );
  return onSwitchTo;
}

describe("RepoTabs keyboard and ARIA", () => {
  it("makes only the selected tab a tab stop and links each tab to its panel", () => {
    setup();
    expect(screen.getByRole("tab", { name: "a" })).toHaveAttribute("tabindex", "-1");
    expect(screen.getByRole("tab", { name: "b" })).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("tab", { name: "b" })).toHaveAttribute("aria-controls", repoPanelId("/repos/b"));
  });

  it("ArrowRight/ArrowLeft move to and activate the neighbouring tab, wrapping", () => {
    const onSwitchTo = setup();
    fireEvent.keyDown(screen.getByRole("tab", { name: "b" }), { key: "ArrowRight" });
    expect(onSwitchTo).toHaveBeenLastCalledWith("/repos/c");
    expect(screen.getByRole("tab", { name: "c" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("tab", { name: "c" }), { key: "ArrowRight" });
    expect(onSwitchTo).toHaveBeenLastCalledWith("/repos/a");
    fireEvent.keyDown(screen.getByRole("tab", { name: "a" }), { key: "ArrowLeft" });
    expect(onSwitchTo).toHaveBeenLastCalledWith("/repos/c");
  });

  it("Delete closes the focused tab, even when it isn't the active one, and keeps focus in the strip", () => {
    const onClose = vi.fn();
    const onSwitchTo = setup("/repos/b", onClose);
    const tabA = screen.getByRole("tab", { name: "a" });
    tabA.focus();

    fireEvent.keyDown(tabA, { key: "Delete" });

    expect(onClose).toHaveBeenCalledExactlyOnceWith("/repos/a");
    // Closing isn't switching: the active tab stays b.
    expect(onSwitchTo).not.toHaveBeenCalled();
    expect(screen.getByRole("tab", { name: "b" })).toHaveFocus();
  });

  it("Delete moves focus to the previous tab when closing the last one", () => {
    const onClose = vi.fn();
    setup("/repos/c", onClose);
    const tabC = screen.getByRole("tab", { name: "c" });
    tabC.focus();

    fireEvent.keyDown(tabC, { key: "Delete" });

    expect(onClose).toHaveBeenCalledExactlyOnceWith("/repos/c");
    expect(screen.getByRole("tab", { name: "b" })).toHaveFocus();
  });

  it("Delete does nothing on a tab whose repo has an operation in progress", () => {
    const onClose = vi.fn();
    setup("/repos/b", onClose, new Set(["/repos/b"]));

    fireEvent.keyDown(screen.getByRole("tab", { name: "b" }), { key: "Delete" });

    expect(onClose).not.toHaveBeenCalled();
  });

  it("advertises Delete as each tab's keyboard shortcut", () => {
    setup();
    for (const tab of screen.getAllByRole("tab")) {
      expect(tab).toHaveAttribute("aria-keyshortcuts", "Delete");
    }
  });

  it("Home and End jump to the first and last tab", () => {
    const onSwitchTo = setup();
    fireEvent.keyDown(screen.getByRole("tab", { name: "b" }), { key: "End" });
    expect(onSwitchTo).toHaveBeenLastCalledWith("/repos/c");
    fireEvent.keyDown(screen.getByRole("tab", { name: "c" }), { key: "Home" });
    expect(onSwitchTo).toHaveBeenLastCalledWith("/repos/a");
  });
});
