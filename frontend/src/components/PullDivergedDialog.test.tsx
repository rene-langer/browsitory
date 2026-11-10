import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PullDivergedDialog } from "./PullDivergedDialog";

function renderDialog(props: Partial<Parameters<typeof PullDivergedDialog>[0]> = {}) {
  const handlers = { onMerge: vi.fn(), onRebase: vi.fn(), onCancel: vi.fn() };
  render(
    <PullDivergedDialog
      upstreamRef="origin/main"
      operationDisabled={false}
      operationDisabledReason={null}
      {...handlers}
      {...props}
    />,
  );
  return { dialog: screen.getByRole("dialog", { name: "Pull has diverged" }), ...handlers };
}

describe("PullDivergedDialog", () => {
  it("offers Merge, Rebase and Cancel for the upstream, focusing Cancel", async () => {
    const { dialog, onMerge, onRebase, onCancel } = renderDialog();
    expect(dialog).toHaveTextContent("The pull has diverged from origin/main.");
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus());
    fireEvent.click(within(dialog).getByRole("button", { name: "Merge" }));
    expect(onMerge).toHaveBeenCalledWith("origin/main");
    fireEvent.click(within(dialog).getByRole("button", { name: "Rebase" }));
    expect(onRebase).toHaveBeenCalledWith("origin/main");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
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
});
