import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo } from "../ipc/RepoClient";
import { DeleteRemoteTagDialog } from "./DeleteRemoteTagDialog";
import { describedByText } from "../test/describedBy";

const remote = (name: string): RemoteInfo => ({ name, fetchUrl: name, pushUrl: null, authMode: null, authUsername: null });

function renderDialog(overrides: Partial<Parameters<typeof DeleteRemoteTagDialog>[0]> = {}) {
  const props = {
    tagName: "v1",
    remotes: [remote("origin")],
    disabled: false,
    disabledReason: null,
    onDelete: vi.fn(),
    onCancel: vi.fn(),
    ...overrides,
  };
  render(<DeleteRemoteTagDialog {...props} />);
  return props;
}

describe("DeleteRemoteTagDialog", () => {
  it("confirms against the only remote, says it can't be undone and focuses Cancel", () => {
    const { onDelete } = renderDialog();

    const dialog = screen.getByRole("dialog", { name: "Delete tag v1 from remote" });
    expect(dialog).toHaveTextContent("Delete tag v1 on origin? This can't be undone.");
    expect(screen.queryByLabelText("Remote")).toBeNull();
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();

    fireEvent.click(screen.getByRole("button", { name: "Delete from remote" }));
    expect(onDelete).toHaveBeenCalledWith("origin", false);
  });

  it("picks the remote when there are several, and can delete the local tag too", () => {
    const { onDelete } = renderDialog({ remotes: [remote("backup"), remote("origin")] });

    expect(screen.getByLabelText("Remote")).toHaveValue("origin");
    fireEvent.change(screen.getByLabelText("Remote"), { target: { value: "backup" } });
    expect(screen.getByRole("dialog")).toHaveTextContent("Delete tag v1 on backup?");
    fireEvent.click(screen.getByRole("checkbox", { name: "Also delete the local tag" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete from remote" }));

    expect(onDelete).toHaveBeenCalledWith("backup", true);
  });

  it("can't delete without a remote or while an operation runs", () => {
    renderDialog({ remotes: [] });
    expect(screen.getByRole("button", { name: "Delete from remote" })).toBeDisabled();
  });

  it("explains a disabled delete", () => {
    renderDialog({ disabled: true, disabledReason: "A transfer is in progress." });
    const button = screen.getByRole("button", { name: "Delete from remote" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "A transfer is in progress.");
    expect(describedByText(button)).toBe("A transfer is in progress.");
  });
});
