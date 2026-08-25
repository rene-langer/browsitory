import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, expectTypeOf, it, vi } from "vitest";
import type { RemoteInfo, RepoClient, UpstreamInfo } from "../ipc/RepoClient";
import { RemotePanel } from "./RemotePanel";

type IsOptional<T, K extends keyof T> = Pick<T, K> extends Required<Pick<T, K>> ? false : true;

const origin: RemoteInfo = {
  name: "origin",
  fetchUrl: "../origin.git",
  pushUrl: "../push-origin.git",
  authMode: null,
  authUsername: null,
};

const backup: RemoteInfo = {
  name: "backup",
  fetchUrl: "../backup.git",
  pushUrl: "../push-backup.git",
  authMode: null,
  authUsername: null,
};

const upstream: UpstreamInfo = {
  localBranch: "main",
  remoteName: "origin",
  remoteBranch: "main",
};

function renderPanel(overrides: Partial<Parameters<typeof RemotePanel>[0]> = {}) {
  localStorage.removeItem("sidebar-remotes");

  // pendingPull is handled separately below: in real usage the Pull button only
  // exists inside an already-open panel, so pendingPull only ever transitions to
  // non-null while the accordion is already mounted open — never present at
  // initial mount. Applying it via rerender (after opening) instead of at the
  // initial render reproduces that real ordering, so the pull-diverged <dialog>'s
  // showModal() effect (which only runs when the dialog ref is already attached)
  // fires the same way it does in production.
  const { pendingPull, ...restOverrides } = overrides;
  const props: Parameters<typeof RemotePanel>[0] = {
    remotes: [origin],
    upstream: null,
    remoteUpstreams: { origin: [] },
    onAddRemote: vi.fn().mockResolvedValue(null),
    onRenameRemote: vi.fn().mockResolvedValue(true),
    onUpdateRemoteUrls: vi.fn(),
    onRemoveRemote: vi.fn().mockResolvedValue(undefined),
    onSaveHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onForgetHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onSetRemoteAuthMode: vi.fn().mockResolvedValue(true),
    onSetUpstream: vi.fn(),
    onClearUpstream: vi.fn(),
    onFetchRemote: vi.fn(),
    fetchDisabled: false,
    onPushCurrentBranch: vi.fn(),
    pushDisabled: false,
    onPull: vi.fn(),
    pullDisabled: false,
    pendingPull: null,
    pullOutcome: null,
    onMergePull: vi.fn(),
    onRebasePull: vi.fn(),
    onCancelPull: vi.fn(),
    ...restOverrides,
  };
  const result = render(<RemotePanel {...props} />);
  // Scoped to this render's container: some tests call renderPanel() twice
  // without unmounting the first instance, so an unscoped query would match
  // more than one "Remotes" toggle button.
  fireEvent.click(within(result.container).getByRole("button", { name: "Remotes" }));
  if (pendingPull !== undefined) {
    result.rerender(<RemotePanel {...props} pendingPull={pendingPull} />);
  }
  return result;
}

describe("RemotePanel", () => {
  it("copies the fetch URL to the clipboard", () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    renderPanel({});

    fireEvent.click(screen.getByRole("button", { name: "Copy fetch URL for origin" }));

    expect(writeText).toHaveBeenCalledWith("../origin.git");
  });

  it("copies the push URL to the clipboard when one is set", () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    renderPanel({});

    fireEvent.click(screen.getByRole("button", { name: "Copy push URL for origin" }));

    expect(writeText).toHaveBeenCalledWith("../push-origin.git");
  });

  it("does not render a push-URL copy button when the remote has no push URL", () => {
    renderPanel({ remotes: [{ ...origin, pushUrl: null }] });

    expect(screen.queryByRole("button", { name: "Copy push URL for origin" })).not.toBeInTheDocument();
  });

  it("submits the token only to the save callback and clears the input", async () => {
    const onSaveHttpsCredential = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onSaveHttpsCredential });

    fireEvent.click(screen.getByRole("button", { name: "Credentials for origin" }));
    fireEvent.change(screen.getByLabelText("HTTPS username"), { target: { value: "rene" } });
    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "token-123" } });
    fireEvent.click(screen.getByRole("button", { name: "Save HTTPS credential" }));

    await waitFor(() => {
      expect(onSaveHttpsCredential).toHaveBeenCalledWith("origin", "rene", "token-123");
    });
    expect(screen.getByLabelText("Access token")).toHaveValue("");
  });

  it("uses the SSH agent without rendering a token input", async () => {
    const onSetRemoteAuthMode = vi.fn().mockResolvedValue(true);
    renderPanel({ onSetRemoteAuthMode });

    fireEvent.click(screen.getByRole("button", { name: "Credentials for origin" }));
    fireEvent.change(screen.getByLabelText("Authentication for origin"), {
      target: { value: "SshAgent" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Use SSH agent" }));

    await waitFor(() => {
      expect(onSetRemoteAuthMode).toHaveBeenCalledWith("origin", "SshAgent", null);
    });
    expect(screen.queryByLabelText("Access token")).not.toBeInTheDocument();
  });

  it("clears the token after a failed credential save", async () => {
    const onSaveHttpsCredential = vi.fn().mockRejectedValue(new Error("keychain unavailable"));
    renderPanel({ onSaveHttpsCredential });

    fireEvent.click(screen.getByRole("button", { name: "Credentials for origin" }));
    fireEvent.change(screen.getByLabelText("HTTPS username"), { target: { value: "rene" } });
    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "token-123" } });
    fireEvent.click(screen.getByRole("button", { name: "Save HTTPS credential" }));

    await waitFor(() => {
      expect(screen.getByLabelText("Access token")).toHaveValue("");
    });
  });

  it("does not save a token when HTTPS authentication setup fails", async () => {
    const onSetRemoteAuthMode = vi.fn().mockResolvedValue(false);
    const onSaveHttpsCredential = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onSetRemoteAuthMode, onSaveHttpsCredential });

    fireEvent.click(screen.getByRole("button", { name: "Credentials for origin" }));
    fireEvent.change(screen.getByLabelText("HTTPS username"), { target: { value: "rene" } });
    fireEvent.change(screen.getByLabelText("Access token"), { target: { value: "token-123" } });
    fireEvent.click(screen.getByRole("button", { name: "Save HTTPS credential" }));

    await waitFor(() => {
      expect(onSetRemoteAuthMode).toHaveBeenCalledWith("origin", "HttpsToken", "rene");
    });
    expect(onSaveHttpsCredential).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Access token")).toHaveValue("");
  });

  it("renders the credentials form nested under its own row, not another remote's", () => {
    renderPanel({ remotes: [origin, backup] });

    fireEvent.click(screen.getByRole("button", { name: "Credentials for origin" }));

    const credentialsForm = screen.getByRole("form", { name: "Credentials for origin" });
    const originItem = credentialsForm.closest("li");
    const backupItem = screen.getByText("backup", { selector: "strong" }).closest("li");
    expect(originItem).not.toBeNull();
    expect(backupItem).not.toBeNull();
    expect(originItem).not.toBe(backupItem);
    expect(within(backupItem!).queryByLabelText("Authentication for origin")).not.toBeInTheDocument();
  });

  it("pushes the current branch to a chosen remote and disables Push during an operation", () => {
    const onPushCurrentBranch = vi.fn();
    renderPanel({ onPushCurrentBranch, pushDisabled: true });

    expect(screen.getByRole("button", { name: "Push branch to origin" })).toBeDisabled();

    renderPanel({ onPushCurrentBranch, pushDisabled: false });
    screen.getAllByRole("button", { name: "Push branch to origin" })[1].click();

    expect(onPushCurrentBranch).toHaveBeenCalledWith("origin");
  });

  it("fetches the selected remote and disables Fetch while an operation is active", () => {
    const onFetchRemote = vi.fn();
    renderPanel({ onFetchRemote, fetchDisabled: true });

    expect(screen.getByRole("button", { name: "Fetch origin" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add remote" })).toBeDisabled();

    renderPanel({ onFetchRemote, fetchDisabled: false });
    screen.getAllByRole("button", { name: "Fetch origin" })[1].click();
    expect(screen.getAllByRole("button", { name: "Add remote" })[1]).toBeEnabled();

    expect(onFetchRemote).toHaveBeenCalledWith("origin");
  });

  it("offers merge or rebase only after a divergent pull", () => {
    const onMergePull = vi.fn();
    const onRebasePull = vi.fn();
    const onCancelPull = vi.fn();
    renderPanel({
      pendingPull: { upstreamRef: "refs/remotes/origin/main" },
      onMergePull,
      onRebasePull,
      onCancelPull,
    });

    const dialog = screen.getByRole("dialog", { name: "Pull has diverged" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Merge" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Rebase" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(onMergePull).toHaveBeenCalledWith("refs/remotes/origin/main");
    expect(onRebasePull).toHaveBeenCalledWith("refs/remotes/origin/main");
    expect(onCancelPull).toHaveBeenCalledOnce();
  });

  it("keeps Pull disabled while a reconciliation choice is open and focuses Cancel", async () => {
    renderPanel({
      upstream,
      pendingPull: { upstreamRef: "refs/remotes/origin/main" },
    });

    expect(screen.getByRole("button", { name: "Pull" })).toBeDisabled();
    const dialog = screen.getByRole("dialog", { name: "Pull has diverged" });
    await waitFor(() => {
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    });
  });

  it("disables divergent pull choices while another repository operation is active", () => {
    renderPanel({
      upstream,
      pullDisabled: true,
      pendingPull: { upstreamRef: "refs/remotes/origin/main" },
    });

    const dialog = screen.getByRole("dialog", { name: "Pull has diverged" });
    expect(within(dialog).getByRole("button", { name: "Merge" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Rebase" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeEnabled();
  });

  it("renders a successful up-to-date pull outcome", () => {
    renderPanel({ upstream, pullOutcome: { kind: "UpToDate" } });

    expect(screen.getByRole("status")).toHaveTextContent("Already up to date.");
  });

  it("requires upstream discovery in both the client and panel contracts", () => {
    expectTypeOf<IsOptional<RepoClient, "getRemoteUpstreams">>().toEqualTypeOf<false>();
    expectTypeOf<
      IsOptional<Parameters<typeof RemotePanel>[0], "remoteUpstreams">
    >().toEqualTypeOf<false>();
  });

  it("adds a remote using the labelled URL form", () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    renderPanel({ onAddRemote });

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    fireEvent.change(screen.getByLabelText("Remote name"), { target: { value: "backup" } });
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    fireEvent.click(screen.getByText("Push URL (optional)"));
    fireEvent.change(screen.getByLabelText("Push URL"), { target: { value: "../push-backup.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));

    expect(onAddRemote).toHaveBeenCalledWith("backup", "../backup.git", "../push-backup.git");
  });

  it("shows example placeholder text on the add-remote URL fields", () => {
    renderPanel({});

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    expect(screen.getByLabelText("Fetch URL")).toHaveAttribute(
      "placeholder",
      "git@github.com:user/repo.git",
    );
  });

  it("auto-derives the remote name as origin when no remote is named origin yet", () => {
    renderPanel({ remotes: [] });

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    fireEvent.change(screen.getByLabelText("Fetch URL"), {
      target: { value: "git@github.com:user/repo.git" },
    });

    expect(screen.getByLabelText("Remote name")).toHaveValue("origin");
  });

  it("auto-derives the remote name from the URL slug when origin already exists", () => {
    renderPanel({});

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    fireEvent.change(screen.getByLabelText("Fetch URL"), {
      target: { value: "git@github.com:user/repo.git" },
    });

    expect(screen.getByLabelText("Remote name")).toHaveValue("repo");
  });

  // The two tests above paste the whole URL in one `change` event. Real typing fires one
  // `change` per keystroke, which is what broke the original `newName === ""` guard: the first
  // keystroke derived a one-character name, and every keystroke after that saw a non-empty name
  // and stopped deriving.
  it("keeps deriving the remote name while the URL is typed character by character", () => {
    renderPanel({});

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    const fetchUrl = screen.getByLabelText("Fetch URL");
    const url = "git@github.com:user/repo.git";
    for (let length = 1; length <= url.length; length += 1) {
      fireEvent.change(fetchUrl, { target: { value: url.slice(0, length) } });
    }

    expect(screen.getByLabelText("Remote name")).toHaveValue("repo");
  });

  it("does not overwrite a manually entered remote name when the URL changes", () => {
    renderPanel({});

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    fireEvent.change(screen.getByLabelText("Remote name"), { target: { value: "custom" } });
    fireEvent.change(screen.getByLabelText("Fetch URL"), {
      target: { value: "git@github.com:user/repo.git" },
    });

    expect(screen.getByLabelText("Remote name")).toHaveValue("custom");
  });

  it("keeps the Push URL field collapsed behind a disclosure by default", () => {
    renderPanel({});

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    expect(screen.queryByLabelText("Push URL")).not.toBeInTheDocument();

    fireEvent.click(screen.getByText("Push URL (optional)"));

    expect(screen.getByLabelText("Push URL")).toBeInTheDocument();
  });

  it("gives the Add remote submit button primary styling", () => {
    renderPanel({});

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    expect(screen.getByRole("button", { name: "Add remote" })).toHaveClass("primary");
  });

  // `useAppState`'s `addRemote` never rejects — it reports failure by *resolving* to the message
  // (see its comment there). A `mockRejectedValue` here would only exercise a shape the real
  // wiring can't produce, which is exactly how the panel ended up with a dead `catch` block and
  // a success path that wiped the user's input on a failed add.
  it("shows a failed add-remote's message next to the Fetch URL field and keeps the entered values", async () => {
    const onAddRemote = vi.fn().mockResolvedValue("invalid fetch URL");
    renderPanel({ onAddRemote });

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "not-a-url" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("invalid fetch URL");
    expect(screen.getByLabelText("Fetch URL")).toHaveValue("not-a-url");
    expect(screen.getByLabelText("Remote name")).toHaveValue("not-a-url");
  });

  it("clears the add-remote failure message once the Fetch URL is edited again", async () => {
    const onAddRemote = vi.fn().mockResolvedValue("invalid fetch URL");
    renderPanel({ onAddRemote });

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "not-a-url" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    await screen.findByRole("alert");

    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("clears the form and re-collapses the Push URL disclosure after a successful add", async () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    renderPanel({ onAddRemote });

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    fireEvent.click(screen.getByText("Push URL (optional)"));
    fireEvent.change(screen.getByLabelText("Push URL"), { target: { value: "../push-backup.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));

    await waitFor(() => expect(screen.getByLabelText("Fetch URL")).toHaveValue(""));
    expect(screen.getByLabelText("Remote name")).toHaveValue("");
    // A cleared-but-still-open disclosure on the next add is just an empty field taking up
    // space, so the disclosure resets with the rest of the form.
    expect(screen.queryByLabelText("Push URL")).not.toBeInTheDocument();
  });

  it("keeps the Add-remote form collapsed until its button is clicked", () => {
    renderPanel({});

    expect(screen.queryByLabelText("Fetch URL")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));

    expect(screen.getByLabelText("Fetch URL")).toBeInTheDocument();
  });

  it("collapses the Add-remote form again on Cancel without adding a remote", () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    renderPanel({ onAddRemote });

    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    fireEvent.change(screen.getByLabelText("Fetch URL"), { target: { value: "../backup.git" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(screen.queryByLabelText("Fetch URL")).not.toBeInTheDocument();
    expect(onAddRemote).not.toHaveBeenCalled();
  });

  it("shows a callout pointing at the Add-remote form when there are no remotes", () => {
    renderPanel({ remotes: [], remoteUpstreams: {} });

    expect(screen.getByText(/add a remote below to push and pull/i)).toBeInTheDocument();
  });

  it("sets the current branch upstream from the selected remote and branch", () => {
    const onSetUpstream = vi.fn();
    renderPanel({ onSetUpstream });

    fireEvent.change(screen.getByLabelText("Upstream remote"), { target: { value: "origin" } });
    fireEvent.change(screen.getByLabelText("Upstream branch"), { target: { value: "main" } });
    fireEvent.click(screen.getByRole("button", { name: "Set upstream" }));

    expect(onSetUpstream).toHaveBeenCalledWith("origin", "main");
  });

  it("does not update an existing remote's URLs when rename fails", async () => {
    const onRenameRemote = vi.fn().mockResolvedValue(false);
    const onUpdateRemoteUrls = vi.fn();
    renderPanel({ remotes: [origin, backup], onRenameRemote, onUpdateRemoteUrls });

    fireEvent.click(screen.getByRole("button", { name: "Edit origin" }));
    const editForm = screen.getByRole("form", { name: "Edit origin" });
    fireEvent.change(within(editForm).getByLabelText("Remote name"), {
      target: { value: "backup" },
    });
    fireEvent.change(within(editForm).getByLabelText("Fetch URL"), {
      target: { value: "../replacement.git" },
    });
    fireEvent.change(within(editForm).getByLabelText("Push URL"), {
      target: { value: "../replacement-push.git" },
    });
    fireEvent.click(within(editForm).getByRole("button", { name: "Save remote" }));

    await waitFor(() => expect(onRenameRemote).toHaveBeenCalledWith("origin", "backup"));
    expect(onUpdateRemoteUrls).not.toHaveBeenCalled();
    const backupItem = screen.getByText("backup", { selector: "strong" }).closest("li");
    expect(backupItem).not.toBeNull();
    expect(within(backupItem!).getByText("Fetch: ../backup.git")).toBeInTheDocument();
    expect(within(backupItem!).getByText("Push: ../push-backup.git")).toBeInTheDocument();
  });

  it("opens the Edit form's Push URL disclosure by default when the remote already has one", () => {
    renderPanel({});

    fireEvent.click(screen.getByRole("button", { name: "Edit origin" }));

    expect(within(screen.getByRole("form", { name: "Edit origin" })).getByLabelText("Push URL")).toBeInTheDocument();
  });

  it("keeps the Edit form's Push URL disclosure collapsed by default when the remote has none", () => {
    renderPanel({ remotes: [{ ...origin, pushUrl: null }] });

    fireEvent.click(screen.getByRole("button", { name: "Edit origin" }));

    expect(within(screen.getByRole("form", { name: "Edit origin" })).queryByLabelText("Push URL")).not.toBeInTheDocument();
  });

  it("shows the explicit all-branch removal route for a remote with upstreams", async () => {
    const onRemoveRemote = vi.fn().mockResolvedValue(undefined);
    const onClearUpstream = vi.fn().mockResolvedValue(undefined);
    const topicUpstream: UpstreamInfo = {
      localBranch: "topic",
      remoteName: "origin",
      remoteBranch: "topic",
    };
    renderPanel({
      upstream,
      remoteUpstreams: { origin: [upstream, topicUpstream] },
      onRemoveRemote,
      onClearUpstream,
    });

    fireEvent.click(screen.getByRole("button", { name: "Remove origin" }));

    expect(screen.getByText(/clear upstreams for main, topic/i)).toBeInTheDocument();
    expect(onRemoveRemote).not.toHaveBeenCalled();

    fireEvent.click(within(screen.getByRole("dialog", { name: "Remove remote confirmation" })).getByRole("button", { name: "Confirm remove" }));
    expect(onRemoveRemote).toHaveBeenCalledWith("origin", true);
  });

  it("focuses Cancel when the remove-remote confirmation opens", async () => {
    renderPanel({});

    fireEvent.click(screen.getByRole("button", { name: "Remove origin" }));
    const dialog = screen.getByRole("dialog", { name: "Remove remote confirmation" });
    await waitFor(() => {
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    });
  });

  it("removes a remote after explicit confirmation when it has no upstream", () => {
    const onRemoveRemote = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onRemoveRemote });

    fireEvent.click(screen.getByRole("button", { name: "Remove origin" }));
    expect(screen.getByText(/remove remote origin/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Confirm remove" }));

    expect(onRemoveRemote).toHaveBeenCalledWith("origin", false);
  });

  it("styles the Remove button with the danger variant", () => {
    renderPanel({});

    expect(screen.getByRole("button", { name: "Remove origin" })).toHaveClass("danger");
  });

  it("renders remote row actions as icon buttons without repeating the remote name as visible text", () => {
    renderPanel({});

    const fetchButton = screen.getByRole("button", { name: "Fetch origin" });
    expect(fetchButton).not.toHaveTextContent("Fetch origin");
    expect(fetchButton.querySelector("svg")).not.toBeNull();
  });
});
