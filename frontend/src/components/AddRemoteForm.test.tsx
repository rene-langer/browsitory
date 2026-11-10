import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderTree } from "../test/branchTreeProps";

const oneRemote = [{ name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null }];

describe("AddRemoteForm (in BranchTree)", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("Add remote shows a remote-name problem inline and keeps Add remote disabled", () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    renderTree({ addRemoteDraftOpen: true, onAddRemote });
    fireEvent.change(screen.getByLabelText("Remote name"), { target: { value: "my fork" } });
    fireEvent.change(screen.getByTestId("add-remote-fetch-url"), { target: { value: "../fork.git" } });

    expect(screen.getByText("Remote names can't contain spaces.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add remote" })).toBeDisabled();
    expect(onAddRemote).not.toHaveBeenCalled();
  });

  it("Add remote refuses an invalid name derived from the URL with the same message", async () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    // "origin" is taken, so the name comes from the URL's last part.
    renderTree({
      addRemoteDraftOpen: true,
      onAddRemote,
      remotes: [{ name: "origin", fetchUrl: "u", pushUrl: null, authMode: null, authUsername: null }],
    });
    fireEvent.change(screen.getByTestId("add-remote-fetch-url"), { target: { value: "../my fork.git" } });
    fireEvent.change(screen.getByLabelText("Remote name"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));

    expect(await screen.findByText("Remote names can't contain spaces.")).toBeInTheDocument();
    expect(onAddRemote).not.toHaveBeenCalled();
  });

  it("keeps the add-remote form hidden until addRemoteDraftOpen is true", () => {
    renderTree({ addRemoteDraftOpen: false });
    expect(screen.queryByLabelText("Fetch URL")).not.toBeInTheDocument();
  });

  it("addRemoteDraftOpen shows the add-remote form; submitting calls onAddRemote", async () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    renderTree({ addRemoteDraftOpen: true, onAddRemote });
    fireEvent.change(screen.getByTestId("add-remote-fetch-url"), {
      target: { value: "git@github.com:user/repo.git" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    expect(onAddRemote).toHaveBeenCalledWith("origin", "git@github.com:user/repo.git", null);
  });

  it("adds a remote with a custom name and push URL via the add-remote form", () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    renderTree({ addRemoteDraftOpen: true, onAddRemote });
    fireEvent.change(screen.getByLabelText("Remote name"), { target: { value: "backup" } });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    fireEvent.click(screen.getByText("Push URL (optional)"));
    fireEvent.change(screen.getByLabelText("Push URL"), { target: { value: "../push-backup.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    expect(onAddRemote).toHaveBeenCalledWith("backup", "../backup.git", "../push-backup.git");
  });

  it("shows example placeholder text on the add-remote fetch URL field", () => {
    renderTree({ addRemoteDraftOpen: true });
    expect(screen.getByLabelText("Fetch URL")).toHaveAttribute("placeholder", "git@github.com:user/repo.git");
  });

  it("auto-derives the remote name as origin when no remote is named origin yet", () => {
    renderTree({ addRemoteDraftOpen: true, remotes: [] });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "git@github.com:user/repo.git" } });
    expect(screen.getByLabelText("Remote name")).toHaveValue("origin");
  });

  it("auto-derives the remote name from the URL slug when origin already exists", () => {
    renderTree({ addRemoteDraftOpen: true, remotes: oneRemote });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "git@github.com:user/repo.git" } });
    expect(screen.getByLabelText("Remote name")).toHaveValue("repo");
  });

  // Real typing fires one `change` per keystroke, which is what broke the original
  // `newName === ""` guard: the first keystroke derived a one-character name, and every keystroke
  // after that saw a non-empty name and stopped deriving.
  it("keeps deriving the remote name while the URL is typed character by character", () => {
    renderTree({ addRemoteDraftOpen: true, remotes: oneRemote });
    const fetchUrl = screen.getByLabelText("Fetch URL");
    const url = "git@github.com:user/repo.git";
    for (let length = 1; length <= url.length; length += 1) {
      fireEvent.change(fetchUrl, { target: { value: url.slice(0, length) } });
    }
    expect(screen.getByLabelText("Remote name")).toHaveValue("repo");
  });

  it("does not overwrite a manually entered remote name when the URL changes", () => {
    renderTree({ addRemoteDraftOpen: true, remotes: oneRemote });
    fireEvent.change(screen.getByLabelText("Remote name"), { target: { value: "custom" } });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "git@github.com:user/repo.git" } });
    expect(screen.getByLabelText("Remote name")).toHaveValue("custom");
  });

  it("keeps the Push URL field collapsed behind a disclosure by default", () => {
    renderTree({ addRemoteDraftOpen: true });
    expect(screen.queryByLabelText("Push URL")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("Push URL (optional)"));
    expect(screen.getByLabelText("Push URL")).toBeInTheDocument();
  });

  // `useAppState`'s `addRemote` never rejects — it reports failure by *resolving* to the message.
  it("shows a failed add-remote's message and keeps the entered values", async () => {
    const onAddRemote = vi.fn().mockResolvedValue("invalid fetch URL");
    renderTree({ addRemoteDraftOpen: true, remotes: oneRemote, onAddRemote });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "not-a-url" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("invalid fetch URL");
    expect(screen.getByLabelText("Fetch URL")).toHaveValue("not-a-url");
    expect(screen.getByLabelText("Remote name")).toHaveValue("not-a-url");
  });

  it("clears the add-remote failure message once the Fetch URL is edited again", async () => {
    const onAddRemote = vi.fn().mockResolvedValue("invalid fetch URL");
    renderTree({ addRemoteDraftOpen: true, remotes: oneRemote, onAddRemote });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "not-a-url" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    await screen.findByRole("alert");
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("clears the form and re-collapses the Push URL disclosure after a successful add", async () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    renderTree({ addRemoteDraftOpen: true, onAddRemote });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    fireEvent.click(screen.getByText("Push URL (optional)"));
    fireEvent.change(screen.getByLabelText("Push URL"), { target: { value: "../push-backup.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    await waitFor(() => expect(screen.getByLabelText("Fetch URL")).toHaveValue(""));
    expect(screen.getByLabelText("Remote name")).toHaveValue("");
    expect(screen.queryByLabelText("Push URL")).not.toBeInTheDocument();
  });

  it("Cancel in the add-remote form calls onCloseAddRemoteDraft without adding a remote", () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    const onCloseAddRemoteDraft = vi.fn();
    renderTree({ addRemoteDraftOpen: true, onAddRemote, onCloseAddRemoteDraft });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCloseAddRemoteDraft).toHaveBeenCalledOnce();
    expect(onAddRemote).not.toHaveBeenCalled();
  });

  it("focuses the add-remote name field on open and Escape cancels", () => {
    const { props } = renderTree({ addRemoteDraftOpen: true });
    const input = screen.getByPlaceholderText("origin");
    expect(input).toHaveFocus();
    fireEvent.keyDown(input, { key: "Escape" });
    expect(props.onCloseAddRemoteDraft).toHaveBeenCalled();
  });
});
