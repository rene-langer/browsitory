import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo } from "../ipc/RepoClient";
import { RemoteCredentialsDialog } from "./RemoteCredentialsDialog";

const origin: RemoteInfo = {
  name: "origin",
  fetchUrl: "https://example.com/repo.git",
  pushUrl: null,
  authMode: null,
  authUsername: null,
};

function renderDialog(overrides: Partial<Parameters<typeof RemoteCredentialsDialog>[0]> = {}) {
  const props: Parameters<typeof RemoteCredentialsDialog>[0] = {
    remote: origin,
    onSetAuthMode: vi.fn().mockResolvedValue(true),
    onSaveHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onForgetHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onClose: vi.fn(),
    ...overrides,
  };
  render(<RemoteCredentialsDialog {...props} />);
  return { props, dialog: screen.getByRole("dialog", { name: "Credentials for origin", hidden: true }) };
}

function fillHttps(dialog: HTMLElement, username: string, token: string) {
  fireEvent.change(within(dialog).getByLabelText("HTTPS username"), { target: { value: username } });
  fireEvent.change(within(dialog).getByLabelText("Access token"), { target: { value: token } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Save HTTPS credential", hidden: true }));
}

describe("RemoteCredentialsDialog", () => {
  it("starts from the remote's saved mode and username", () => {
    const { dialog } = renderDialog({ remote: { ...origin, authMode: "HttpsToken", authUsername: "rene" } });
    expect(within(dialog).getByLabelText("HTTPS username")).toHaveValue("rene");
  });

  it("saves an HTTPS credential after configuring the mode, then closes", async () => {
    const { props, dialog } = renderDialog();
    fillHttps(dialog, " me ", "tok");
    await waitFor(() => expect(props.onSaveHttpsCredential).toHaveBeenCalledWith("origin", "me", "tok"));
    expect(props.onSetAuthMode).toHaveBeenCalledWith("origin", "HttpsToken", "me");
    await waitFor(() => expect(props.onClose).toHaveBeenCalled());
  });

  it("uses the SSH agent without rendering a token input, and explains it", async () => {
    const { props, dialog } = renderDialog();
    fireEvent.change(within(dialog).getByLabelText("Authentication for origin"), { target: { value: "SshAgent" } });
    expect(within(dialog).queryByLabelText("Access token")).toBeNull();
    expect(within(dialog).getByText(/uses your system's ssh agent/i)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Use SSH agent", hidden: true }));
    await waitFor(() => expect(props.onSetAuthMode).toHaveBeenCalledWith("origin", "SshAgent", null));
    await waitFor(() => expect(props.onClose).toHaveBeenCalled());
  });

  it("clears the token after a failed save, keeping the dialog open", async () => {
    const { props, dialog } = renderDialog({
      onSaveHttpsCredential: vi.fn().mockRejectedValue(new Error("keychain unavailable")),
    });
    fillHttps(dialog, "rene", "token-123");
    await waitFor(() => expect(within(dialog).getByLabelText("Access token")).toHaveValue(""));
    expect(props.onClose).not.toHaveBeenCalled();
  });

  it("does not save a token when HTTPS setup fails", async () => {
    const { props, dialog } = renderDialog({ onSetAuthMode: vi.fn().mockResolvedValue(false) });
    fillHttps(dialog, "rene", "token-123");
    await waitFor(() => expect(props.onSetAuthMode).toHaveBeenCalledWith("origin", "HttpsToken", "rene"));
    expect(props.onSaveHttpsCredential).not.toHaveBeenCalled();
    await waitFor(() => expect(within(dialog).getByLabelText("Access token")).toHaveValue(""));
  });

  it("forgets the HTTPS credential", () => {
    const { props, dialog } = renderDialog();
    fireEvent.click(within(dialog).getByRole("button", { name: "Forget HTTPS credential", hidden: true }));
    expect(props.onForgetHttpsCredential).toHaveBeenCalledWith("origin");
  });
});
