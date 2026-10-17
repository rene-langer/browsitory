import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CloneError, type CloneOutcome, type RepoClient, type TransferProgress } from "../ipc/RepoClient";
import { CloneDialog } from "./CloneDialog";

type Listener = (progress: TransferProgress) => void;

function fakeClient(overrides: Partial<RepoClient> = {}) {
  const listeners: Listener[] = [];
  const client = {
    supportsCloneAndInit: true,
    pickRepoFolder: vi.fn(async () => "/home/me/src"),
    cloneRepo: vi.fn(async (): Promise<CloneOutcome> => ({ path: "/home/me/src/repo", submoduleFailures: [], credentialSaveFailed: false })),
    cancelClone: vi.fn(async () => {}),
    subscribeTransferProgress: (listener: Listener) => {
      listeners.push(listener);
      return () => listeners.splice(listeners.indexOf(listener), 1);
    },
    ...overrides,
  } as unknown as RepoClient;
  return { client, emit: (p: TransferProgress) => listeners.forEach((l) => l(p)) };
}

function fill(url: string, parent = "/home/me/src") {
  fireEvent.change(screen.getByLabelText("Repository URL"), { target: { value: url } });
  fireEvent.change(screen.getByLabelText("Parent folder"), { target: { value: parent } });
}

describe("CloneDialog", () => {
  it("derives the folder name from the URL until the user edits it", () => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    expect(screen.getByLabelText("Folder name")).toHaveValue("repo");
    fireEvent.change(screen.getByLabelText("Folder name"), { target: { value: "mine" } });
    fill("https://github.com/org/other.git");
    expect(screen.getByLabelText("Folder name")).toHaveValue("mine");
    expect(screen.getByText("/home/me/src/mine")).toBeInTheDocument();
  });

  it("shows the HTTPS fields only for https URLs", () => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("git@github.com:org/repo.git");
    expect(screen.queryByRole("button", { name: /HTTPS credentials/ })).not.toBeInTheDocument();
    fill("http://github.com/org/repo.git");
    expect(screen.queryByRole("button", { name: /HTTPS credentials/ })).not.toBeInTheDocument();
    fill("https://github.com/org/repo.git");
    expect(screen.getByRole("button", { name: /HTTPS credentials/ })).toBeInTheDocument();
  });

  it("disables Clone until URL, parent and name are filled", () => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Clone" })).toBeDisabled();
    fill("https://github.com/org/repo.git");
    expect(screen.getByRole("button", { name: "Clone" })).toBeEnabled();
  });

  it.each(["~/src", "src", "./src"])("refuses the relative parent folder %s with a hint", (parent) => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git", parent);
    expect(screen.getByRole("button", { name: "Clone" })).toBeDisabled();
    expect(screen.getByText("Use an absolute folder path, or Browse…")).toBeInTheDocument();
  });

  it.each(["/home/me", "C:\\Users\\me", "C:/Users/me", "\\\\server\\share"])("accepts the absolute parent folder %s", (parent) => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git", parent);
    expect(screen.getByRole("button", { name: "Clone" })).toBeEnabled();
    expect(screen.queryByText("Use an absolute folder path, or Browse…")).not.toBeInTheDocument();
  });

  it("points an http:// URL at https when it needs a token", async () => {
    const { client } = fakeClient({
      cloneRepo: vi.fn(async () => {
        throw new CloneError("MissingCredential", "x");
      }),
    });
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("http://example.test/org/repo.git");
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    expect(await screen.findByText("Use an https:// URL to sign in with a token.")).toBeInTheDocument();
  });

  it("fills the parent folder from Browse", async () => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Browse…" }));
    await waitFor(() => expect(screen.getByLabelText("Parent folder")).toHaveValue("/home/me/src"));
  });

  it("sends the request and reports the outcome", async () => {
    const onCloned = vi.fn();
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={onCloned} onCancel={vi.fn()} />);
    fill(" https://github.com/org/repo.git ");
    fireEvent.change(screen.getByLabelText("Branch (optional)"), { target: { value: "  " } });
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    await waitFor(() => expect(onCloned).toHaveBeenCalled());
    expect(client.cloneRepo).toHaveBeenCalledWith(expect.any(String), {
      url: "https://github.com/org/repo.git",
      dest: "/home/me/src/repo",
      branch: null,
      recurseSubmodules: true,
      https: null,
    });
    expect(onCloned).toHaveBeenCalledWith(expect.objectContaining({ path: "/home/me/src/repo" }), "repo");
  });

  it("sends HTTPS credentials when entered", async () => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    fireEvent.click(screen.getByRole("button", { name: /HTTPS credentials/ }));
    fireEvent.change(screen.getByLabelText("Username"), { target: { value: "alice" } });
    fireEvent.change(screen.getByLabelText("Token"), { target: { value: "tok" } });
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    await waitFor(() => expect(client.cloneRepo).toHaveBeenCalled());
    expect(vi.mocked(client.cloneRepo).mock.calls[0][1].https).toEqual({ username: "alice", token: "tok" });
  });

  it("shows progress for its own operation only and cancels it", async () => {
    let finish: (o: CloneOutcome) => void = () => {};
    const { client, emit } = fakeClient({
      cloneRepo: vi.fn(() => new Promise<CloneOutcome>((resolve) => (finish = resolve))),
    });
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    const opId = vi.mocked(client.cloneRepo).mock.calls[0][0];
    const base = { operation: "Clone", errorKind: null, receivedBytes: 0, message: null } as const;
    act(() => emit({ ...base, operationId: "other", phase: "Receiving", current: 9, total: 9 }));
    act(() => emit({ ...base, operationId: opId, phase: "Receiving", current: 3, total: 10 }));
    expect(screen.getByText("Receiving objects 3/10")).toBeInTheDocument();
    expect(screen.getByLabelText("Repository URL")).toBeDisabled();
    act(() => emit({ ...base, operationId: opId, phase: "Updating", current: 1, total: 2 }));
    expect(screen.getByText("Checking out…")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel clone" }));
    expect(client.cancelClone).toHaveBeenCalledWith(opId);
    await act(async () => finish({ path: "/x", submoduleFailures: [], credentialSaveFailed: false }));
  });

  it.each([
    ["DestNotEmpty", "/home/me/src/repo already exists and isn't empty."],
    ["BranchNotFound", "Branch dev not found on the remote."],
    ["InvalidUrl", "Unsupported URL. Use https://, ssh://, git@host:path or a local path."],
    ["SshAgentFailure", "SSH agent unavailable or has no matching key."],
    ["TransferFailed", "backend said no"],
  ] as const)("shows %s inline and stays open", async (kind, text) => {
    const { client } = fakeClient({
      cloneRepo: vi.fn(async () => {
        throw new CloneError(kind, "backend said no");
      }),
    });
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    fireEvent.change(screen.getByLabelText("Branch (optional)"), { target: { value: "dev" } });
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    expect(await screen.findByText(text)).toBeInTheDocument();
    expect(screen.getByLabelText("Repository URL")).toBeEnabled();
  });

  it.each([
    ["MissingCredential", "This repository needs a username and token."],
    ["CredentialBearingUrl", "Remove the password or token from the URL and put it in the token field."],
  ] as const)("%s expands the HTTPS section", async (kind, text) => {
    const { client } = fakeClient({
      cloneRepo: vi.fn(async () => {
        throw new CloneError(kind, "x");
      }),
    });
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    expect(await screen.findByText(text)).toBeInTheDocument();
    expect(screen.getByLabelText("Token")).toBeInTheDocument();
  });

  it("returns to the form without an error after a cancelled clone", async () => {
    const { client } = fakeClient({
      cloneRepo: vi.fn(async () => {
        throw new CloneError("Cancelled", "Clone cancelled.");
      }),
    });
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    await waitFor(() => expect(screen.getByLabelText("Repository URL")).toBeEnabled());
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("reports running state to its host", async () => {
    const onRunningChange = vi.fn();
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} onRunningChange={onRunningChange} />);
    fill("https://github.com/org/repo.git");
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    expect(onRunningChange).toHaveBeenCalledWith(true);
    await waitFor(() => expect(onRunningChange).toHaveBeenLastCalledWith(false));
  });
});
