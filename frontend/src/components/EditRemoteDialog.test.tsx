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

  // #418: unchanged fields make Save a Cancel; the name gets the Add remote form's check.
  it("closes without a backend call when nothing changed (spaces aside)", async () => {
    const { props, dialog } = renderDialog();
    fireEvent.change(within(dialog).getByLabelText("Remote name"), { target: { value: " origin " } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote", hidden: true }));
    await waitFor(() => expect(props.onClose).toHaveBeenCalled());
    expect(props.onRename).not.toHaveBeenCalled();
    expect(props.onUpdateUrls).not.toHaveBeenCalled();
  });

  it("trims the new name before renaming", async () => {
    const { props, dialog } = renderDialog();
    fireEvent.change(within(dialog).getByLabelText("Remote name"), { target: { value: "  upstream " } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote", hidden: true }));
    await waitFor(() => expect(props.onRename).toHaveBeenCalledWith("origin", "upstream"));
  });

  it("shows a remote-name problem inline and keeps Save disabled", () => {
    const { props, dialog } = renderDialog();
    const input = within(dialog).getByLabelText("Remote name");
    fireEvent.change(input, { target: { value: "up..stream" } });

    expect(within(dialog).getByText("Remote names can't contain .. or @{.")).toBeInTheDocument();
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(within(dialog).getByRole("button", { name: "Save remote", hidden: true })).toBeDisabled();
    fireEvent.submit(input.closest("form")!);
    expect(props.onRename).not.toHaveBeenCalled();
    expect(props.onUpdateUrls).not.toHaveBeenCalled();
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

  it("disables Save with the reason while the repository is busy (#561)", () => {
    const { props, dialog } = renderDialog({ blockedReason: "A merge is in progress." });
    fireEvent.change(within(dialog).getByLabelText("Push URL"), { target: { value: "../push.git" } });
    const save = within(dialog).getByRole("button", { name: "Save remote", hidden: true });
    expect(save).toBeDisabled();
    expect(save).toHaveAttribute("title", "A merge is in progress.");
    expect(within(dialog).getByText("A merge is in progress.")).toBeInTheDocument();
    fireEvent.submit(save.closest("form")!);
    expect(props.onUpdateUrls).not.toHaveBeenCalled();
    expect(props.onClose).not.toHaveBeenCalled();
  });

  it("Cancel closes without saving", () => {
    const { props, dialog } = renderDialog();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel", hidden: true }));
    expect(props.onClose).toHaveBeenCalled();
    expect(props.onUpdateUrls).not.toHaveBeenCalled();
  });
});
