import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SwitchConflictDialog } from "./SwitchConflictDialog";

function renderDialog(props: Partial<Parameters<typeof SwitchConflictDialog>[0]> = {}) {
  const handlers = { onStashAndSwitch: vi.fn(), onCancel: vi.fn() };
  render(
    <SwitchConflictDialog
      branchName="feature"
      currentBranch="main"
      files={["a.txt", "src/b.rs"]}
      operationDisabled={false}
      operationDisabledReason={null}
      {...handlers}
      {...props}
    />,
  );
  return { dialog: screen.getByRole("dialog", { name: "Local changes are in the way" }), ...handlers };
}

describe("SwitchConflictDialog (#653)", () => {
  it("names the files and stashes and switches, re-applying by default", () => {
    const { dialog, onStashAndSwitch } = renderDialog();
    expect(dialog).toHaveTextContent("Switching to feature would overwrite your changes to these files:");
    const files = within(dialog).getByRole("list", { name: "Files in the way" });
    expect(within(files).getAllByRole("listitem").map((item) => item.textContent)).toEqual(["a.txt", "src/b.rs"]);
    expect(within(dialog).getByRole("checkbox", { name: "Re-apply the changes on feature" })).toBeChecked();
    expect(dialog).toHaveTextContent("Undo goes back to main with them.");
    fireEvent.click(within(dialog).getByRole("button", { name: "Stash and switch" }));
    expect(onStashAndSwitch).toHaveBeenCalledWith(true);
  });

  it("leaves the changes in the stash when Re-apply is unticked", () => {
    const { dialog, onStashAndSwitch } = renderDialog();
    fireEvent.click(within(dialog).getByRole("checkbox", { name: "Re-apply the changes on feature" }));
    expect(dialog).toHaveTextContent("The changes stay in the stash list");
    fireEvent.click(within(dialog).getByRole("button", { name: "Stash and switch" }));
    expect(onStashAndSwitch).toHaveBeenCalledWith(false);
  });

  it("lists at most eight files, then how many more", () => {
    const files = Array.from({ length: 11 }, (_, n) => `f${n}.txt`);
    const { dialog } = renderDialog({ files });
    const list = within(dialog).getByRole("list", { name: "Files in the way" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(9);
    expect(list).toHaveTextContent("and 3 more");
  });

  it("disables Stash and switch while the repository is busy, and Cancel closes", () => {
    const { dialog, onCancel } = renderDialog({ operationDisabled: true, operationDisabledReason: "A merge is in progress." });
    expect(within(dialog).getByRole("button", { name: "Stash and switch" })).toBeDisabled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
  });
});
