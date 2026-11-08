import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { SubmoduleInfo } from "../ipc/RepoClient";
import { SubmodulePanel } from "./SubmodulePanel";

const uninitializedSubmodule: SubmoduleInfo = {
  path: "deps/child",
  url: "https://example.com/child.git",
  gitlinkId: "0123456789abcdef",
  initialized: false,
  headId: null,
  workdir: null,
};

const initializedSubmodule: SubmoduleInfo = {
  ...uninitializedSubmodule,
  path: "deps/ready-child",
  initialized: true,
  headId: "fedcba9876543210",
  workdir: "/repo/deps/ready-child",
};

function renderPanel(
  overrides: Partial<Parameters<typeof SubmodulePanel>[0]> = {},
) {
  localStorage.removeItem("sidebar-submodules");
  const result = render(
    <SubmodulePanel
      submodules={[uninitializedSubmodule, initializedSubmodule]}
      onInit={vi.fn().mockResolvedValue(undefined)}
      onUpdate={vi.fn().mockResolvedValue(undefined)}
      onOpenInNewTab={vi.fn()}
      operationDisabled={false}
      operationDisabledReason={null}
      {...overrides}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Submodules" }));
  return result;
}

describe("SubmodulePanel", () => {
  it("shows an empty state when there are no submodules", () => {
    renderPanel({ submodules: [] });

    expect(screen.getByText(/No submodules/)).toBeInTheDocument();
  });

  it("shows each submodule's URL, recorded gitlink, and initialization state", () => {
    renderPanel();

    expect(screen.getAllByText("https://example.com/child.git")).toHaveLength(2);
    expect(screen.getAllByText("0123456789abcdef")).toHaveLength(2);
    expect(screen.getByText("Not initialized")).toBeInTheDocument();
    expect(screen.getByText("Initialized")).toBeInTheDocument();
    expect(screen.getByText("fedcba9876543210")).toBeInTheDocument();
  });

  it("initializes the selected submodule and updates without recursion until explicitly checked", async () => {
    const onInit = vi.fn().mockResolvedValue(undefined);
    const onUpdate = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onInit, onUpdate });

    fireEvent.click(screen.getByRole("button", { name: "Initialize deps/child" }));
    await waitFor(() => expect(onInit).toHaveBeenCalledWith("deps/child"));

    fireEvent.click(screen.getByRole("button", { name: "Update deps/ready-child" }));
    await waitFor(() => expect(onUpdate).toHaveBeenCalledWith("deps/ready-child", false));

    fireEvent.click(screen.getByRole("checkbox", { name: "Update recursively" }));
    fireEvent.click(screen.getByRole("button", { name: "Update deps/ready-child" }));
    await waitFor(() => expect(onUpdate).toHaveBeenLastCalledWith("deps/ready-child", true));
  });

  it("disables a row while its mutation is pending and exposes no configuration controls", async () => {
    let finishUpdate: (() => void) | undefined;
    const onUpdate = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finishUpdate = resolve;
        }),
    );
    renderPanel({ onUpdate });

    const updateButton = screen.getByRole("button", { name: "Update deps/ready-child" });
    fireEvent.click(updateButton);

    expect(updateButton).toBeDisabled();
    expect(screen.getByRole("button", { name: "Initialize deps/ready-child" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: /edit|delete|remove/i })).not.toBeInTheDocument();

    finishUpdate?.();
  });

  it("opens a checked-out submodule's folder in a new tab from its row menu", () => {
    const onOpenInNewTab = vi.fn();
    renderPanel({ onOpenInNewTab });

    fireEvent.click(screen.getByRole("button", { name: "Actions for deps/ready-child" }));
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Open in new tab",
      "Update",
    ]);
    fireEvent.click(screen.getByRole("menuitem", { name: "Open in new tab" }));

    expect(onOpenInNewTab).toHaveBeenCalledWith("/repo/deps/ready-child");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("disables Open in new tab with a reason for a submodule that isn't initialised, and offers Initialize", async () => {
    const onInit = vi.fn().mockResolvedValue(undefined);
    const onOpenInNewTab = vi.fn();
    renderPanel({ onInit, onOpenInNewTab });

    fireEvent.click(screen.getByRole("button", { name: "Actions for deps/child" }));
    const open = screen.getByRole("menuitem", { name: "Open in new tab" });
    expect(open).toBeDisabled();
    expect(open).toHaveAttribute("title", "Not initialized: initialize and update it to check it out first.");
    fireEvent.click(open);
    expect(onOpenInNewTab).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("menuitem", { name: "Initialize" }));
    await waitFor(() => expect(onInit).toHaveBeenCalledWith("deps/child"));
  });

  it("disables Open in new tab with a reason for an initialised submodule that isn't checked out", () => {
    renderPanel({ submodules: [{ ...initializedSubmodule, headId: null, workdir: null }] });

    fireEvent.click(screen.getByRole("button", { name: "Actions for deps/ready-child" }));
    const open = screen.getByRole("menuitem", { name: "Open in new tab" });
    expect(open).toBeDisabled();
    expect(open).toHaveAttribute("title", "Not checked out: update it to check it out first.");
    expect(screen.queryByRole("menuitem", { name: "Initialize" })).toBeNull();
    expect(screen.getByRole("menuitem", { name: "Update" })).toBeEnabled();
  });

  it("disables the row menu's actions while an operation runs, with its reason", () => {
    renderPanel({ operationDisabled: true, operationDisabledReason: "A rebase is in progress." });

    fireEvent.click(screen.getByRole("button", { name: "Actions for deps/ready-child" }));
    for (const name of ["Open in new tab", "Update"]) {
      const item = screen.getByRole("menuitem", { name });
      expect(item).toBeDisabled();
      expect(item).toHaveAttribute("title", "A rebase is in progress.");
    }
  });

  it("opens the row menu by right-click and from the keyboard (Shift+F10, Menu key)", () => {
    renderPanel();

    const row = screen.getByText("deps/ready-child").closest("li");
    if (row === null) throw new Error("expected the submodule row");
    fireEvent.contextMenu(row);
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Open in new tab" }), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();

    const actions = screen.getByRole("button", { name: "Actions for deps/ready-child" });
    expect(actions).toHaveAttribute("aria-keyshortcuts", "Shift+F10 ContextMenu");
    fireEvent.keyDown(actions, { key: "F10", shiftKey: true });
    expect(screen.getByRole("menuitem", { name: "Open in new tab" })).toBeEnabled();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Open in new tab" }), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();

    fireEvent.keyDown(screen.getByRole("button", { name: "Update deps/ready-child" }), { key: "ContextMenu" });
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });
});
