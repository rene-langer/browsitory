import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RepoTabs } from "./RepoTabs";
import { repoPanelId } from "./repoTabIds";

const repos = [
  { path: "/repos/a", displayName: "a", workspaceId: null },
  { path: "/repos/b", displayName: "b", workspaceId: null },
  { path: "/repos/c", displayName: "c", workspaceId: null },
];

function setup(activePath = "/repos/b") {
  const onSwitchTo = vi.fn();
  render(
    <RepoTabs
      openRepos={repos}
      activePath={activePath}
      busyPaths={new Set()}
      workspaceNames={{}}
      onSwitchTo={onSwitchTo}
      onClose={vi.fn()}
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

  it("Home and End jump to the first and last tab", () => {
    const onSwitchTo = setup();
    fireEvent.keyDown(screen.getByRole("tab", { name: "b" }), { key: "End" });
    expect(onSwitchTo).toHaveBeenLastCalledWith("/repos/c");
    fireEvent.keyDown(screen.getByRole("tab", { name: "c" }), { key: "Home" });
    expect(onSwitchTo).toHaveBeenLastCalledWith("/repos/a");
  });
});
