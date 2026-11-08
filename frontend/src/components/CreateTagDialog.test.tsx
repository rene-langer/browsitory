import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo } from "../ipc/RepoClient";
import { CreateTagDialog } from "./CreateTagDialog";

const remote = (name: string): RemoteInfo => ({ name, fetchUrl: name, pushUrl: null, authMode: null, authUsername: null });

function renderDialog(overrides: Partial<Parameters<typeof CreateTagDialog>[0]> = {}) {
  const props = {
    shortId: "abc1234",
    summary: "Fix the thing",
    remotes: [remote("origin")],
    disabled: false,
    disabledReason: null,
    onCreate: vi.fn(async () => null as string | null),
    onCancel: vi.fn(),
    ...overrides,
  };
  render(<CreateTagDialog {...props} />);
  return props;
}

describe("CreateTagDialog", () => {
  it("shows the fixed target commit and focuses the name", () => {
    renderDialog();

    expect(screen.getByRole("dialog", { name: "Create tag" })).toHaveTextContent("Tag commit abc1234 Fix the thing");
    expect(screen.getByLabelText("Tag name")).toHaveFocus();
  });

  it("creates a lightweight tag with the trimmed name", async () => {
    const { onCreate } = renderDialog();

    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: " v1.0.0 " } });
    fireEvent.click(screen.getByRole("button", { name: "Create tag" }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledWith("v1.0.0", null, null));
  });

  it("requires a message for an annotated tag", async () => {
    const { onCreate } = renderDialog();

    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v2" } });
    fireEvent.click(screen.getByRole("radio", { name: "Annotated tag" }));
    expect(screen.getByRole("button", { name: "Create tag" })).toBeDisabled();

    fireEvent.change(screen.getByLabelText("Tag message"), { target: { value: "Second release" } });
    fireEvent.click(screen.getByRole("button", { name: "Create tag" }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledWith("v2", "Second release", null));
  });

  it("optionally pushes to the chosen remote", async () => {
    const { onCreate } = renderDialog({ remotes: [remote("backup"), remote("origin")] });

    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v3" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Push the tag after creating it" }));
    // `origin` is preselected when it exists.
    expect(screen.getByLabelText("Remote")).toHaveValue("origin");
    fireEvent.change(screen.getByLabelText("Remote"), { target: { value: "backup" } });
    fireEvent.click(screen.getByRole("button", { name: "Create and push to backup" }));

    await waitFor(() => expect(onCreate).toHaveBeenCalledWith("v3", null, "backup"));
  });

  it("offers no push without a remote", () => {
    renderDialog({ remotes: [] });

    expect(screen.queryByRole("checkbox", { name: "Push the tag after creating it" })).toBeNull();
  });

  it("shows a failure inline and keeps the input", async () => {
    renderDialog({ onCreate: vi.fn(async () => "tag v1 already exists") });

    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v1" } });
    fireEvent.click(screen.getByRole("button", { name: "Create tag" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("tag v1 already exists");
    expect(screen.getByLabelText("Tag name")).toHaveValue("v1");
    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v1.1" } });
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("is blocked while a repository operation runs, and says why", () => {
    renderDialog({ disabled: true, disabledReason: "A transfer is in progress." });

    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v1" } });
    const create = screen.getByRole("button", { name: "Create tag" });
    expect(create).toBeDisabled();
    expect(create).toHaveAttribute("title", "A transfer is in progress.");
  });
});
