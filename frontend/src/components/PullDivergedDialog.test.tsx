import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { shortRefName } from "../lib/shortRefName";
import { PullDivergedDialog } from "./PullDivergedDialog";

function renderDialog(props: Partial<Parameters<typeof PullDivergedDialog>[0]> = {}) {
  const handlers = { onMerge: vi.fn(), onRebase: vi.fn(), onCancel: vi.fn() };
  render(
    <PullDivergedDialog
      upstreamRef="refs/remotes/origin/main"
      branchName="main"
      operationDisabled={false}
      operationDisabledReason={null}
      {...handlers}
      {...props}
    />,
  );
  return { dialog: screen.getByRole("dialog", { name: "Pull has diverged" }), ...handlers };
}

describe("PullDivergedDialog", () => {
  it("names both branches by their short names and offers Merge, Rebase and Cancel, focusing Cancel", async () => {
    const { dialog, onMerge, onRebase, onCancel } = renderDialog();
    expect(dialog).toHaveTextContent("main and origin/main have both changed.");
    expect(dialog).not.toHaveTextContent("refs/remotes");
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus());
    fireEvent.click(within(dialog).getByRole("button", { name: "Merge" }));
    expect(onMerge).toHaveBeenCalledWith("refs/remotes/origin/main", false);
    fireEvent.click(within(dialog).getByRole("button", { name: "Rebase" }));
    expect(onRebase).toHaveBeenCalledWith("refs/remotes/origin/main", false);
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("passes Remember for this repository with the choice", () => {
    const { dialog, onRebase } = renderDialog();
    fireEvent.click(within(dialog).getByRole("checkbox", { name: "Remember for this repository" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Rebase" }));
    expect(onRebase).toHaveBeenCalledWith("refs/remotes/origin/main", true);
  });

  it("says why when only a fast-forward was allowed", () => {
    const { dialog } = renderDialog({ fastForwardOnly: true });
    expect(dialog).toHaveTextContent("main can't be fast-forwarded to origin/main");
  });

  it("disables Merge and Rebase, with the reason, while another operation runs", () => {
    const { dialog } = renderDialog({ operationDisabled: true, operationDisabledReason: "A rebase is in progress." });
    for (const name of ["Merge", "Rebase"]) {
      const button = within(dialog).getByRole("button", { name });
      expect(button).toBeDisabled();
      expect(button).toHaveAttribute("title", "A rebase is in progress.");
    }
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeEnabled();
  });

  it("shortens local and remote-tracking ref names", () => {
    expect(shortRefName("refs/remotes/origin/feat/x")).toBe("origin/feat/x");
    expect(shortRefName("refs/heads/main")).toBe("main");
    expect(shortRefName("origin/main")).toBe("origin/main");
  });
});
