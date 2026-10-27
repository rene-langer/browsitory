import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { MergeOptions } from "../ipc/RepoClient";
import { MergeDialog } from "./MergeDialog";

function renderDialog(loaded: MergeOptions, overrides: Partial<Parameters<typeof MergeDialog>[0]> = {}) {
  const onMerge = vi.fn();
  const onCancel = vi.fn();
  render(
    <MergeDialog
      branchName="feature"
      currentBranchName="main"
      loadOptions={async () => loaded}
      onMerge={onMerge}
      onCancel={onCancel}
      {...overrides}
    />,
  );
  return { onMerge, onCancel };
}

describe("MergeDialog", () => {
  it("preselects the loaded options and enables Merge once loaded", async () => {
    let resolve: (options: MergeOptions) => void = () => {};
    renderDialog({ fastForward: "Allow", squash: false }, {
      loadOptions: () => new Promise((r) => { resolve = r; }),
    });
    expect(screen.getByRole("button", { name: "Merge" })).toBeDisabled();

    resolve({ fastForward: "Only", squash: false });

    expect(await screen.findByRole("radio", { name: "Fast-forward only" })).toBeChecked();
    expect(screen.getByRole("button", { name: "Merge" })).toBeEnabled();
  });

  it("merges with the chosen options", async () => {
    const { onMerge } = renderDialog({ fastForward: "Allow", squash: false });
    await screen.findByRole("button", { name: "Merge" }, {});
    await vi.waitFor(() => expect(screen.getByRole("button", { name: "Merge" })).toBeEnabled());

    fireEvent.click(screen.getByRole("radio", { name: "Always create a merge commit" }));
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));

    expect(onMerge).toHaveBeenCalledWith({ fastForward: "Never", squash: false });
  });

  it("squash disables the fast-forward choices", async () => {
    const { onMerge } = renderDialog({ fastForward: "Only", squash: false });
    await vi.waitFor(() => expect(screen.getByRole("button", { name: "Merge" })).toBeEnabled());

    fireEvent.click(screen.getByRole("checkbox", { name: /Squash/ }));

    for (const name of ["Fast-forward if possible", "Always create a merge commit", "Fast-forward only"]) {
      expect(screen.getByRole("radio", { name })).toBeDisabled();
    }
    expect(screen.getByText("Stage the combined changes; you write the commit.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    expect(onMerge).toHaveBeenCalledWith({ fastForward: "Only", squash: true });
  });

  it("preselects a remembered squash", async () => {
    renderDialog({ fastForward: "Allow", squash: true });
    await vi.waitFor(() => expect(screen.getByRole("checkbox", { name: /Squash/ })).toBeChecked());
  });

  it("names both branches and cancels", async () => {
    const { onCancel } = renderDialog({ fastForward: "Allow", squash: false });
    expect(screen.getByRole("dialog", { name: "Merge feature" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Merge feature into main" })).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Fast-forward" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onCancel).toHaveBeenCalled();
  });

  it("falls back to the defaults when loading fails", async () => {
    renderDialog({ fastForward: "Allow", squash: false }, {
      loadOptions: async () => {
        throw new Error("boom");
      },
    });
    await vi.waitFor(() => expect(screen.getByRole("button", { name: "Merge" })).toBeEnabled());
    expect(screen.getByRole("radio", { name: "Fast-forward if possible" })).toBeChecked();
  });
});
