import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RepoTabs } from "./RepoTabs";

const repos = [
  { path: "/repos/widget", displayName: "widget", workspaceId: null },
  { path: "/repos/gadget", displayName: "gadget", workspaceId: null },
];

const noneBusy = new Set<string>();

describe("RepoTabs", () => {
  it("renders one tab per open repo, marking the active one", () => {
    render(
      <RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={vi.fn()} onClose={vi.fn()} onCloseGroup={vi.fn()} onAddTab={vi.fn()} />,
    );
    const active = screen.getByRole("tab", { name: /gadget/i, selected: true });
    expect(active).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /widget/i, selected: false })).toBeInTheDocument();
  });

  it("clicking a tab calls onSwitchTo with its path", () => {
    const onSwitchTo = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={onSwitchTo} onClose={vi.fn()} onCloseGroup={vi.fn()} onAddTab={vi.fn()} />);
    screen.getByRole("tab", { name: /widget/i }).click();
    expect(onSwitchTo).toHaveBeenCalledWith("/repos/widget");
  });

  it("clicking a tab's close control calls onClose with its path, not onSwitchTo", () => {
    const onClose = vi.fn();
    const onSwitchTo = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={onSwitchTo} onClose={onClose} onCloseGroup={vi.fn()} onAddTab={vi.fn()} />);
    screen.getByRole("button", { name: /close widget/i }).click();
    expect(onClose).toHaveBeenCalledWith("/repos/widget");
    expect(onSwitchTo).not.toHaveBeenCalled();
  });

  it("the trailing add button calls onAddTab", () => {
    const onAddTab = vi.fn();
    render(<RepoTabs openRepos={repos} activePath="/repos/gadget" busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={vi.fn()} onClose={vi.fn()} onCloseGroup={vi.fn()} onAddTab={onAddTab} />);
    screen.getByRole("button", { name: "Open another repository" }).click();
    expect(onAddTab).toHaveBeenCalled();
  });

  it("renders nothing when no repos are open", () => {
    const { container } = render(
      <RepoTabs openRepos={[]} activePath={null} busyPaths={noneBusy} workspaceNames={{}} onSwitchTo={vi.fn()} onClose={vi.fn()} onCloseGroup={vi.fn()} onAddTab={vi.fn()} />,
    );
    expect(container.firstElementChild).toBeNull();
  });

  it("disables the close button for a tab in busyPaths, and clicking it does not call onClose", () => {
    const onClose = vi.fn();
    render(
      <RepoTabs
        openRepos={repos}
        activePath="/repos/gadget"
        busyPaths={new Set(["/repos/widget"])}
        workspaceNames={{}}
        onSwitchTo={vi.fn()}
        onClose={onClose}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );
    const closeButton = screen.getByRole("button", { name: /close widget/i });
    expect(closeButton).toBeDisabled();
    closeButton.click();
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe("RepoTabs grouping", () => {
  const grouped = [
    { path: "/repos/widget", displayName: "widget", workspaceId: "ws-1" },
    { path: "/repos/gadget", displayName: "gadget", workspaceId: "ws-1" },
    { path: "/repos/solo", displayName: "solo", workspaceId: null },
  ];

  it("wraps a contiguous run of same-workspace tabs in a chip labeled with the workspace name", () => {
    render(
      <RepoTabs
        openRepos={grouped}
        activePath="/repos/widget"
        busyPaths={noneBusy}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );

    expect(screen.getByText("Services")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /widget/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /gadget/i })).toBeInTheDocument();
  });

  it("a standalone tab (no workspaceId) renders with no chip", () => {
    render(
      <RepoTabs
        openRepos={grouped}
        activePath="/repos/widget"
        busyPaths={noneBusy}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );

    expect(screen.getByRole("tab", { name: /solo/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /close services/i })).toBeInTheDocument();
  });

  it("clicking the chip's close-all control calls onCloseGroup with every path in that run", () => {
    const onCloseGroup = vi.fn();
    render(
      <RepoTabs
        openRepos={grouped}
        activePath="/repos/widget"
        busyPaths={noneBusy}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={onCloseGroup}
        onAddTab={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /close services/i }));

    expect(onCloseGroup).toHaveBeenCalledWith(["/repos/widget", "/repos/gadget"]);
  });

  it("disables close-all when any repo in the workspace run is busy", () => {
    const onCloseGroup = vi.fn();
    render(
      <RepoTabs
        openRepos={grouped}
        activePath="/repos/widget"
        busyPaths={new Set(["/repos/gadget"])}
        workspaceNames={{ "ws-1": "Services" }}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={onCloseGroup}
        onAddTab={vi.fn()}
      />,
    );

    const closeGroupButton = screen.getByRole("button", { name: /close services/i });
    expect(closeGroupButton).toBeDisabled();
    fireEvent.click(closeGroupButton);
    expect(onCloseGroup).not.toHaveBeenCalled();
  });

  it("a tab whose workspaceId has no matching name in workspaceNames renders standalone", () => {
    render(
      <RepoTabs
        openRepos={[{ path: "/repos/orphan", displayName: "orphan", workspaceId: "deleted-ws" }]}
        activePath="/repos/orphan"
        busyPaths={noneBusy}
        workspaceNames={{}}
        onSwitchTo={vi.fn()}
        onClose={vi.fn()}
        onCloseGroup={vi.fn()}
        onAddTab={vi.fn()}
      />,
    );

    expect(screen.getByRole("tab", { name: /orphan/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /close deleted-ws/i })).not.toBeInTheDocument();
  });
});
