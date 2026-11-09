import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FileContent, RepoClient, TreeEntry, TreeListing } from "../ipc/RepoClient";
import { CommitTreeBrowser, FILTER_DEBOUNCE_MS } from "./CommitTreeBrowser";
import { fakeRepoClient } from "../test/fakeRepoClient";

const COMMIT = "0123456789abcdef0123456789abcdef01234567";

const entry = (path: string, kind: TreeEntry["kind"], size: number | null = null): TreeEntry => ({
  path,
  name: path.split("/").pop()!,
  kind,
  size,
});

const FOLDERS: Record<string, TreeListing> = {
  "": {
    entries: [entry("src", "Folder"), entry("README.txt", "File", 12), entry("logo.png", "File", 8), entry("data.bin", "File", 3)],
    truncated: false,
  },
  src: { entries: [entry("src/main.txt", "File", 20)], truncated: false },
};

const CONTENTS: Record<string, FileContent> = {
  "README.txt": { kind: "Text", size: 12, text: "line one\nline two\n" },
  "src/main.txt": { kind: "Text", size: 20, text: "main\n" },
  "logo.png": { kind: "Image", size: 8, base64: "iVBORw0KGgo=", mime: "image/png" },
  "data.bin": { kind: "Binary", size: 3 },
};

function fakeClient(overrides: Partial<RepoClient> = {}) {
  const client = {
    getCommitTree: vi.fn(async (_repo: string, _commit: string, dir: string, filter: string | null) => {
      if (filter !== null) {
        return { entries: [entry("src/main.txt", "File", 20)], truncated: filter === "a" };
      }
      return FOLDERS[dir] ?? { entries: [], truncated: false };
    }),
    getFileAtCommit: vi.fn(async (_repo: string, _commit: string, path: string) => CONTENTS[path]!),
    saveFileVersion: vi.fn(async (): Promise<string | null> => "/home/me/README.txt"),
    getWorkingTreeCompareDiff: vi.fn(async () => []),
    getBlame: vi.fn(async () => []),
  };
  return fakeRepoClient({ ...client, ...overrides }) as typeof client & RepoClient;
}

function renderBrowser(props: Partial<Parameters<typeof CommitTreeBrowser>[0]> = {}) {
  const client = (props.client as ReturnType<typeof fakeClient> | undefined) ?? fakeClient();
  const onRestoreFileVersion = vi.fn();
  const onOpenFileHistory = vi.fn();
  render(
    <CommitTreeBrowser
      repoPath="/repo"
      client={client}
      commitId={COMMIT}
      onSelectRow={vi.fn()}
      onOpenFileHistory={onOpenFileHistory}
      onRestoreFileVersion={onRestoreFileVersion}
      {...props}
    />,
  );
  return { client, onRestoreFileVersion, onOpenFileHistory };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("CommitTreeBrowser", () => {
  it("lists the root and loads a folder only when it is opened", async () => {
    const { client } = renderBrowser();
    const tree = await screen.findByRole("tree", { name: "Files at 0123456" });
    expect(within(tree).getByRole("button", { name: /README\.txt/ })).toBeInTheDocument();
    expect(client.getCommitTree).toHaveBeenCalledWith("/repo", COMMIT, "", null);
    expect(client.getCommitTree).not.toHaveBeenCalledWith("/repo", COMMIT, "src", null);

    fireEvent.click(within(tree).getByRole("button", { name: "src" }));
    expect(await screen.findByRole("button", { name: /main\.txt/ })).toBeInTheDocument();
    expect(client.getCommitTree).toHaveBeenCalledWith("/repo", COMMIT, "src", null);
    expect(screen.getByRole("treeitem", { name: /^src/ })).toHaveAttribute("aria-expanded", "true");

    // Closing and reopening doesn't load it again.
    fireEvent.click(within(tree).getByRole("button", { name: "src" }));
    fireEvent.click(within(tree).getByRole("button", { name: "src" }));
    expect(client.getCommitTree.mock.calls.filter((call) => call[2] === "src")).toHaveLength(1);
  });

  it("searches the whole tree after typing stops, and says when matches were cut off", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { client } = renderBrowser();
    await screen.findByRole("tree");
    fireEvent.change(screen.getByRole("searchbox", { name: "Filter files" }), { target: { value: "ma" } });
    expect(client.getCommitTree).not.toHaveBeenCalledWith("/repo", COMMIT, "", "ma");
    await act(async () => {
      vi.advanceTimersByTime(FILTER_DEBOUNCE_MS);
    });
    await waitFor(() => expect(client.getCommitTree).toHaveBeenCalledWith("/repo", COMMIT, "", "ma"));
    expect(await screen.findByRole("button", { name: /src\/main\.txt/ })).toBeInTheDocument();
    expect(screen.queryByText(/Showing the first/)).not.toBeInTheDocument();

    fireEvent.change(screen.getByRole("searchbox", { name: "Filter files" }), { target: { value: "a" } });
    await act(async () => {
      vi.advanceTimersByTime(FILTER_DEBOUNCE_MS);
    });
    expect(await screen.findByText(/Showing the first 1 matches/)).toBeInTheDocument();
  });

  it("shows a text file's lines read-only", async () => {
    const { client } = renderBrowser();
    fireEvent.click(await screen.findByRole("button", { name: /README\.txt/ }));
    const content = await screen.findByRole("region", { name: "Content of README.txt" });
    expect(client.getFileAtCommit).toHaveBeenCalledWith("/repo", COMMIT, "README.txt");
    expect(within(content).getByText("line one")).toBeInTheDocument();
    expect(within(content).getByText("line two")).toBeInTheDocument();
    expect(within(content).queryByRole("textbox")).not.toBeInTheDocument();
  });

  it("shows an image, and a notice instead of a binary or too large file", async () => {
    const client = fakeClient();
    renderBrowser({ client });
    fireEvent.click(await screen.findByRole("button", { name: /logo\.png/ }));
    expect(await screen.findByRole("img", { name: "logo.png at 0123456" })).toHaveAttribute(
      "src",
      "data:image/png;base64,iVBORw0KGgo=",
    );

    fireEvent.click(screen.getByRole("button", { name: /data\.bin/ }));
    expect(await screen.findByText(/Binary file \(3 bytes\), not shown/)).toBeInTheDocument();

    client.getFileAtCommit.mockResolvedValueOnce({ kind: "TooLarge", size: 5 * 1024 * 1024 });
    fireEvent.click(screen.getByRole("button", { name: /README\.txt/ }));
    expect(await screen.findByText(/too large to show here \(5 MB\)/)).toBeInTheDocument();
  });

  it("restores a version after confirming, and not while the repository is busy", async () => {
    const { onRestoreFileVersion } = renderBrowser();
    fireEvent.click(await screen.findByRole("button", { name: /README\.txt/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Restore…" }));
    const dialog = screen.getByRole("dialog", { name: "Confirm restore" });
    expect(dialog).toHaveTextContent("Replace README.txt in the working tree with its version from 0123456?");
    fireEvent.click(within(dialog).getByRole("button", { name: "Restore" }));
    expect(onRestoreFileVersion).toHaveBeenCalledWith(COMMIT, "README.txt");
  });

  it("disables Restore with the reason while repository actions are blocked", async () => {
    const { onRestoreFileVersion } = renderBrowser({ restoreBlockedReason: "A merge is in progress." });
    fireEvent.click(await screen.findByRole("button", { name: /README\.txt/ }));
    const restore = await screen.findByRole("button", { name: "Restore…" });
    expect(restore).toBeDisabled();
    expect(restore).toHaveAttribute("title", "A merge is in progress.");
    fireEvent.click(restore);
    expect(onRestoreFileVersion).not.toHaveBeenCalled();
  });

  it("saves a version and says where; a cancelled dialog says nothing", async () => {
    const { client } = renderBrowser();
    fireEvent.click(await screen.findByRole("button", { name: /README\.txt/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Save as…" }));
    expect(await screen.findByText("Saved to /home/me/README.txt")).toBeInTheDocument();
    expect(client.saveFileVersion).toHaveBeenCalledWith("/repo", COMMIT, "README.txt");

    client.saveFileVersion.mockResolvedValueOnce(null);
    fireEvent.click(screen.getByRole("button", { name: "Save as…" }));
    await waitFor(() => expect(client.saveFileVersion).toHaveBeenCalledTimes(2));
    expect(screen.queryByText(/Saved to/)).not.toBeInTheDocument();
  });

  it("compares the file with the working tree and opens its history", async () => {
    const { client, onOpenFileHistory } = renderBrowser();
    fireEvent.click(await screen.findByRole("button", { name: /README\.txt/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Compare with working tree" }));
    await waitFor(() =>
      expect(client.getWorkingTreeCompareDiff).toHaveBeenCalledWith("/repo", COMMIT, "README.txt", null, {
        ignoreWhitespace: false,
        contextLines: 3,
        algorithm: "Default",
      }),
    );
    expect(screen.getByRole("button", { name: "Compare with working tree" })).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(screen.getByRole("button", { name: "History" }));
    expect(onOpenFileHistory).toHaveBeenCalledWith("README.txt");
  });

  it("opens a folder's history from its row and from a file's header (#447)", async () => {
    const onOpenFolderHistory = vi.fn();
    renderBrowser({ onOpenFolderHistory });
    const tree = await screen.findByRole("tree");

    fireEvent.click(within(tree).getByRole("button", { name: "History of folder src" }));
    expect(onOpenFolderHistory).toHaveBeenCalledWith("src");
    // The folder stays closed: the history button is not the row's toggle.
    expect(screen.getByRole("treeitem", { name: /^src/ })).toHaveAttribute("aria-expanded", "false");

    fireEvent.click(within(tree).getByRole("button", { name: "src" }));
    fireEvent.click(await screen.findByRole("button", { name: /main\.txt/ }));
    await screen.findByText("main");
    const header = screen.getByRole("region", { name: `src/main.txt at 0123456` });
    fireEvent.click(within(header).getByRole("button", { name: "History of folder src" }));
    expect(onOpenFolderHistory).toHaveBeenCalledTimes(2);
  });

  it("opens the history of lines selected in a text file at this commit (#447)", async () => {
    const onOpenLineHistory = vi.fn();
    renderBrowser({ onOpenLineHistory });
    fireEvent.click(await screen.findByRole("button", { name: /README\.txt/ }));
    await screen.findByText("line two");
    expect(screen.getByRole("button", { name: "History of selected lines of README.txt" })).toBeDisabled();

    const range = document.createRange();
    range.selectNodeContents(document.querySelector('[data-line-number="2"]')!);
    window.getSelection()!.removeAllRanges();
    window.getSelection()!.addRange(range);
    act(() => {
      document.dispatchEvent(new Event("selectionchange"));
    });

    fireEvent.click(screen.getByRole("button", { name: "History of line 2 of README.txt" }));
    expect(onOpenLineHistory).toHaveBeenCalledWith("README.txt", COMMIT, 2, 2);
  });

  it("offers Retry when a folder fails to load", async () => {
    const getCommitTree = vi.fn().mockRejectedValueOnce(new Error("boom")).mockResolvedValue(FOLDERS[""]);
    renderBrowser({ client: fakeClient({ getCommitTree }) });
    fireEvent.click(await screen.findByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("button", { name: /README\.txt/ })).toBeInTheDocument();
  });
});
