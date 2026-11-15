import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RemoteInfo } from "../ipc/RepoClient";
import { describedByText } from "../test/describedBy";
import { renderTree } from "../test/branchTreeProps";
import { CreateTagDialog } from "./CreateTagDialog";
import { DeleteRemoteBranchDialog } from "./DeleteRemoteBranchDialog";
import { EditRemoteDialog } from "./EditRemoteDialog";
import { IdentityDialog } from "./IdentityDialog";
import { RemoteCredentialsDialog } from "./RemoteCredentialsDialog";

// #1014: form dialogs preselect the obvious values, and a disabled primary button says why in
// visible text tied to it.

const remote = (name: string, extra: Partial<RemoteInfo> = {}): RemoteInfo => ({
  name,
  fetchUrl: `https://example.com/${name}.git`,
  pushUrl: null,
  authMode: null,
  authUsername: null,
  ...extra,
});

describe("Set upstream preselects (#1014)", () => {
  it("preselects the only remote and the same-named branch, ready to submit", async () => {
    const onSetUpstream = vi.fn().mockResolvedValue(undefined);
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main", "develop"]);
    renderTree({ remotes: [remote("origin")], onSetUpstream, onListRemoteBranches });
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    fireEvent.click(screen.getByRole("menuitem", { name: "Set upstream…" }));
    const dialog = await screen.findByRole("dialog", { name: "Set upstream for main" });
    expect(within(dialog).getByLabelText("Upstream remote")).toHaveValue("origin");
    expect(within(dialog).getByLabelText("Upstream branch")).toHaveValue("main");
    await waitFor(() => expect(onListRemoteBranches).toHaveBeenCalledWith("origin"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Set upstream" }));
    await waitFor(() => expect(onSetUpstream).toHaveBeenCalledWith("origin", "main"));
  });

  it("says why Set upstream is off until a remote is chosen", async () => {
    renderTree({ remotes: [remote("upstream"), remote("mirror")] });
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    fireEvent.click(screen.getByRole("menuitem", { name: "Set upstream…" }));
    const dialog = await screen.findByRole("dialog", { name: "Set upstream for main" });
    expect(within(dialog).getByLabelText("Upstream remote")).toHaveValue("");
    const submit = within(dialog).getByRole("button", { name: "Set upstream" });
    expect(submit).toBeDisabled();
    expect(describedByText(submit)).toBe("Choose a remote.");
    expect(within(dialog).getByText("Choose a remote.")).toBeVisible();
  });
});

describe("a disabled primary button says why (#1014)", () => {
  it("Identity: until a name and an email are entered", () => {
    render(<IdentityDialog retrying onSave={vi.fn(async () => null)} onCancel={vi.fn()} />);
    const save = screen.getByRole("button", { name: "Save and commit" });
    expect(save).toBeDisabled();
    expect(describedByText(save)).toBe("Enter a name and an email address.");
    expect(screen.getByText("Enter a name and an email address.")).toBeInTheDocument();
  });

  it("Create tag: until a name is entered, and an annotated tag's message", () => {
    render(
      <CreateTagDialog
        shortId="abc1234"
        summary="Fix the thing"
        existingTags={[]}
        remotes={[]}
        disabled={false}
        disabledReason={null}
        onCreate={vi.fn(async () => null)}
        onCancel={vi.fn()}
      />,
    );
    const create = screen.getByRole("button", { name: "Create tag" });
    expect(create).toBeDisabled();
    expect(describedByText(create)).toBe("Enter a tag name.");
    fireEvent.change(screen.getByLabelText("Tag name"), { target: { value: "v1.0" } });
    expect(create).toBeEnabled();
    fireEvent.click(screen.getByRole("radio", { name: "Annotated tag" }));
    expect(create).toBeDisabled();
    expect(describedByText(create)).toBe("Enter a message for the annotated tag.");
  });

  it("Delete remote branch: until a branch is chosen", async () => {
    render(
      <DeleteRemoteBranchDialog
        remotes={[remote("origin")]}
        initialRemote={null}
        initialBranch={null}
        onListRemoteBranches={vi.fn(async () => ["main", "topic"])}
        onDelete={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    const remove = screen.getByRole("button", { name: "Delete remote branch" });
    expect(remove).toBeDisabled();
    expect(describedByText(remove)).toBe("Choose a branch to delete.");
    expect(remove).toHaveAttribute("data-tone", "danger");
  });
});

describe("Edit remote's URL fields (#1014)", () => {
  it("says an empty push URL means the fetch URL, and labels the copy buttons", () => {
    render(
      <EditRemoteDialog
        remote={remote("origin")}
        onRename={vi.fn().mockResolvedValue(true)}
        onUpdateUrls={vi.fn().mockResolvedValue(undefined)}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByLabelText("Push URL")).toHaveAttribute("placeholder", "Same as fetch URL");
    expect(screen.getByRole("button", { name: "Copy fetch URL for origin", hidden: true })).toHaveTextContent("Copy");
  });
});

describe("Credentials (#1014)", () => {
  const props = {
    onSetAuthMode: vi.fn().mockResolvedValue(true),
    onSaveHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onForgetHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onClose: vi.fn(),
  };

  it("offers Forget only when a credential is saved, and a plain Cancel", () => {
    const { unmount } = render(<RemoteCredentialsDialog remote={remote("origin")} {...props} />);
    expect(screen.queryByRole("button", { name: "Forget HTTPS credential", hidden: true })).toBeNull();
    expect(screen.getByRole("button", { name: "Cancel", hidden: true })).toBeInTheDocument();
    unmount();

    render(<RemoteCredentialsDialog remote={remote("origin", { authMode: "HttpsToken", authUsername: "me" })} {...props} />);
    expect(screen.getByRole("button", { name: "Forget HTTPS credential", hidden: true })).toBeInTheDocument();
  });
});
