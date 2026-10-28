import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo } from "../ipc/RepoClient";
import { EditRemoteDialog } from "./EditRemoteDialog";

const origin: RemoteInfo = {
  name: "origin",
  fetchUrl: "git@github.com:user/repo.git",
  pushUrl: null,
  authMode: null,
  authUsername: null,
};

function renderDialog(overrides: Partial<Parameters<typeof EditRemoteDialog>[0]> = {}) {
  const props: Parameters<typeof EditRemoteDialog>[0] = {
    remote: origin,
    onRename: vi.fn().mockResolvedValue(true),
    onUpdateUrls: vi.fn().mockResolvedValue(undefined),
    onClose: vi.fn(),
    ...overrides,
  };
  render(<EditRemoteDialog {...props} />);
  return { props, dialog: screen.getByRole("dialog", { name: `Edit ${props.remote.name}`, hidden: true }) };
}

describe("EditRemoteDialog", () => {
  it("is prefilled from the remote and saves its URLs, then closes", async () => {
    const { props, dialog } = renderDialog();
    expect(within(dialog).getByLabelText("Remote name")).toHaveValue("origin");
    expect(within(dialog).getByLabelText("Fetch URL")).toHaveValue("git@github.com:user/repo.git");
    expect(within(dialog).getByLabelText("Remote name")).toHaveFocus();
    fireEvent.change(within(dialog).getByLabelText("Push URL"), { target: { value: " ../push.git " } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote", hidden: true }));
    await waitFor(() => expect(props.onClose).toHaveBeenCalled());
    expect(props.onRename).not.toHaveBeenCalled();
    expect(props.onUpdateUrls).toHaveBeenCalledWith("origin", "git@github.com:user/repo.git", "../push.git");
  });

  it("renames first, then updates the URLs under the new name", async () => {
    const { props, dialog } = renderDialog();
    fireEvent.change(within(dialog).getByLabelText("Remote name"), { target: { value: "upstream" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote", hidden: true }));
    await waitFor(() => expect(props.onUpdateUrls).toHaveBeenCalledWith("upstream", "git@github.com:user/repo.git", null));
    expect(props.onRename).toHaveBeenCalledWith("origin", "upstream");
  });

  it("does not update the URLs when the rename fails, leaving the dialog open", async () => {
    const { props, dialog } = renderDialog({ onRename: vi.fn().mockResolvedValue(false) });
    fireEvent.change(within(dialog).getByLabelText("Remote name"), { target: { value: "backup" } });
    fireEvent.change(within(dialog).getByLabelText("Fetch URL"), { target: { value: "../replacement.git" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote", hidden: true }));
    await waitFor(() => expect(props.onRename).toHaveBeenCalledWith("origin", "backup"));
    expect(props.onUpdateUrls).not.toHaveBeenCalled();
    expect(props.onClose).not.toHaveBeenCalled();
  });

  it("copies the fetch URL, and the push URL only when one is set", () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const { dialog } = renderDialog({ remote: { ...origin, pushUrl: "git@github.com:user/push-repo.git" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Copy fetch URL for origin", hidden: true }));
    expect(writeText).toHaveBeenCalledWith("git@github.com:user/repo.git");
    fireEvent.click(within(dialog).getByRole("button", { name: "Copy push URL for origin", hidden: true }));
    expect(writeText).toHaveBeenCalledWith("git@github.com:user/push-repo.git");
  });

  it("has no push-URL copy button when the remote has no push URL", () => {
    const { dialog } = renderDialog();
    expect(within(dialog).queryByRole("button", { name: "Copy push URL for origin", hidden: true })).toBeNull();
  });

  it("Cancel closes without saving", () => {
    const { props, dialog } = renderDialog();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel", hidden: true }));
    expect(props.onClose).toHaveBeenCalled();
    expect(props.onUpdateUrls).not.toHaveBeenCalled();
  });
});
