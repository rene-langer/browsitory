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
};

const initializedSubmodule: SubmoduleInfo = {
  ...uninitializedSubmodule,
  path: "deps/ready-child",
  initialized: true,
  headId: "fedcba9876543210",
};

function renderPanel(
  overrides: Partial<Parameters<typeof SubmodulePanel>[0]> = {},
) {
  return render(
    <SubmodulePanel
      submodules={[uninitializedSubmodule, initializedSubmodule]}
      onInit={vi.fn().mockResolvedValue(undefined)}
      onUpdate={vi.fn().mockResolvedValue(undefined)}
      operationDisabled={false}
      {...overrides}
    />,
  );
}

describe("SubmodulePanel", () => {
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
});
