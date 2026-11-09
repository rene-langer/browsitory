import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DEFAULT_DIFF_SETTINGS, WHOLE_FILE_CONTEXT_LINES, type DiffSource, type RepoClient } from "../ipc/RepoClient";
import { DiffSettingsContext } from "../state/useDiffSettings";
import type { ExternalToolsApi } from "../state/useExternalTools";
import { ReadOnlyFileSection } from "./ReadOnlyFileSection";

function renderSection(overrides: Partial<Parameters<typeof ReadOnlyFileSection>[0]> = {}) {
  const props: Parameters<typeof ReadOnlyFileSection>[0] = {
    repoPath: "/repo",
    client: { getBlame: vi.fn(async () => []) } as unknown as RepoClient,
    path: "a.txt",
    diffKey: "k1",
    fetchDiff: vi.fn(async () => []),
    blameAt: "abc",
    collapsed: false,
    onToggleCollapse: vi.fn(),
    onSelectRow: vi.fn(),
    ...overrides,
  };
  return { ...render(<ReadOnlyFileSection {...props} />), props };
}

describe("ReadOnlyFileSection", () => {
  it("fetches through fetchDiff with the diff settings and refetches when diffKey changes", async () => {
    const { props, rerender } = renderSection();
    await waitFor(() => expect(props.fetchDiff).toHaveBeenCalledWith({ ignoreWhitespace: false, contextLines: 3, algorithm: "Default" }));

    rerender(<ReadOnlyFileSection {...props} diffKey="k2" />);
    await waitFor(() => expect(props.fetchDiff).toHaveBeenCalledTimes(2));
  });

  it("Show whole file refetches the commit or compare diff with whole-file context", async () => {
    const hunk = { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add" as const, content: "x\n" }] };
    const { props } = renderSection({ fetchDiff: vi.fn(async () => [hunk]) });
    fireEvent.click(await screen.findByRole("button", { name: "Show whole file" }));
    await waitFor(() =>
      expect(props.fetchDiff).toHaveBeenLastCalledWith({
        ignoreWhitespace: false,
        contextLines: WHOLE_FILE_CONTEXT_LINES,
        algorithm: "Default",
      }),
    );
    expect(screen.getByRole("button", { name: "Show whole file" })).toHaveAttribute("aria-pressed", "true");
  });

  it("a commit's hunk header reverts or applies that hunk with the context it was shown with (#448)", async () => {
    const hunk = { oldStart: 4, oldLines: 1, newStart: 5, newLines: 1, lines: [{ origin: "Add" as const, content: "x\n" }] };
    const patch = {
      blockedReason: null,
      onRevertFile: vi.fn(),
      onApplyFile: vi.fn(),
      onSavePatch: vi.fn(),
      onRevertHunk: vi.fn(),
      onApplyHunk: vi.fn(),
    };
    renderSection({ fetchDiff: vi.fn(async () => [hunk]), patch });
    fireEvent.click(await screen.findByRole("button", { name: "Revert hunk" }));
    expect(patch.onRevertHunk).toHaveBeenCalledWith({ oldStart: 4, newStart: 5, contextLines: 3, algorithm: "Default" });
    fireEvent.click(screen.getByRole("button", { name: "Apply hunk" }));
    expect(patch.onApplyHunk).toHaveBeenCalledWith({ oldStart: 4, newStart: 5, contextLines: 3, algorithm: "Default" });

    // The file menu opens without external tools, and holds the file's actions.
    fireEvent.click(screen.getByRole("button", { name: "Actions for a.txt" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Revert this file" }));
    expect(patch.onRevertFile).toHaveBeenCalled();
    expect(screen.queryByRole("menuitem", { name: "Open in editor" })).toBeNull();
  });

  it("a commit's hunk action carries the algorithm the diff was fetched with (#469)", async () => {
    const hunk = { oldStart: 4, oldLines: 1, newStart: 5, newLines: 1, lines: [{ origin: "Add" as const, content: "x\n" }] };
    const patch = {
      blockedReason: null,
      onRevertFile: vi.fn(),
      onApplyFile: vi.fn(),
      onSavePatch: vi.fn(),
      onRevertHunk: vi.fn(),
      onApplyHunk: vi.fn(),
    };
    const fetchDiff = vi.fn(async () => [hunk]);
    const settings = { ...DEFAULT_DIFF_SETTINGS, algorithm: "Patience" as const };
    render(
      <DiffSettingsContext.Provider value={{ settings, update: vi.fn(), error: null, dismissError: vi.fn() }}>
        <ReadOnlyFileSection
          repoPath="/repo"
          client={{ getBlame: vi.fn(async () => []) } as unknown as RepoClient}
          path="a.txt"
          diffKey="k1"
          fetchDiff={fetchDiff}
          blameAt="abc"
          collapsed={false}
          onToggleCollapse={vi.fn()}
          onSelectRow={vi.fn()}
          patch={patch}
        />
      </DiffSettingsContext.Provider>,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Revert hunk" }));
    expect(fetchDiff).toHaveBeenCalledWith({ ignoreWhitespace: false, contextLines: 3, algorithm: "Patience" });
    expect(patch.onRevertHunk).toHaveBeenCalledWith({ oldStart: 4, newStart: 5, contextLines: 3, algorithm: "Patience" });
  });

  it("disables a commit's hunk actions with the reason while repository actions are blocked", async () => {
    const hunk = { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add" as const, content: "x\n" }] };
    const patch = {
      blockedReason: "A rebase is in progress.",
      onRevertFile: vi.fn(),
      onApplyFile: vi.fn(),
      onSavePatch: vi.fn(),
      onRevertHunk: vi.fn(),
      onApplyHunk: vi.fn(),
    };
    renderSection({ fetchDiff: vi.fn(async () => [hunk]), patch });
    const revert = await screen.findByRole("button", { name: "Revert hunk" });
    expect(revert).toBeDisabled();
    expect(revert).toHaveAttribute("title", "A rebase is in progress.");
  });

  it("does not fetch while collapsed", () => {
    const { props } = renderSection({ collapsed: true });
    expect(props.fetchDiff).not.toHaveBeenCalled();
  });

  it("blames at blameAt", async () => {
    const getBlame = vi.fn(async () => []);
    renderSection({ client: { getBlame } as unknown as RepoClient, blameAt: "HEAD" });
    fireEvent.click(screen.getByRole("button", { name: "Blame" }));
    await waitFor(() => expect(getBlame).toHaveBeenCalledWith("/repo", "HEAD", "a.txt"));
  });

  it("shows the rename and a History button when given", () => {
    const onOpenFileHistory = vi.fn();
    renderSection({ path: "new.txt", oldPath: "old.txt", onOpenFileHistory });
    expect(screen.getByText("Renamed from old.txt")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "History of new.txt" }));
    expect(onOpenFileHistory).toHaveBeenCalledWith("new.txt");
  });

  describe("line and folder history (#447)", () => {
    const hunk = {
      oldStart: 4,
      oldLines: 1,
      newStart: 4,
      newLines: 3,
      lines: [
        { origin: "Context" as const, content: "four\n" },
        { origin: "Remove" as const, content: "old five\n" },
        { origin: "Add" as const, content: "new five\n" },
        { origin: "Add" as const, content: "six\n" },
      ],
    };

    function select(from: Node, to: Node) {
      const range = document.createRange();
      range.setStart(from, 0);
      range.setEnd(to, to.childNodes.length);
      const selection = window.getSelection()!;
      selection.removeAllRanges();
      selection.addRange(range);
      act(() => {
        document.dispatchEvent(new Event("selectionchange"));
      });
    }

    function lineElement(line: number): HTMLElement {
      return document.querySelector(`[data-line-number="${line}"]`)!;
    }

    it("opens the history of the diff lines selected, at blameAt", async () => {
      const onOpenLineHistory = vi.fn();
      renderSection({ path: "src/a.txt", fetchDiff: vi.fn(async () => [hunk]), onOpenLineHistory });
      await screen.findByText("six");
      const button = screen.getByRole("button", { name: "History of selected lines of src/a.txt" });
      expect(button).toBeDisabled();

      select(lineElement(5), lineElement(6));

      fireEvent.click(screen.getByRole("button", { name: "History of lines 5–6 of src/a.txt" }));
      expect(onOpenLineHistory).toHaveBeenCalledWith("src/a.txt", "abc", 5, 6);
    });

    it("counts only new-side lines: a selection of removed lines alone selects nothing", async () => {
      renderSection({ fetchDiff: vi.fn(async () => [hunk]), onOpenLineHistory: vi.fn() });
      await screen.findByText("six");
      const removed = document.querySelector(".diff-line-remove")!;

      select(removed, removed);

      expect(screen.getByRole("button", { name: "History of selected lines of a.txt" })).toBeDisabled();
    });

    it("opens the history of lines selected in the blame view", async () => {
      const onOpenLineHistory = vi.fn();
      const blame = [1, 2, 3].map((n) => ({
        lineNumber: n,
        content: `line ${n}`,
        commitId: "c",
        shortId: "c",
        authorName: "A",
        timestamp: 0,
      }));
      renderSection({ client: { getBlame: vi.fn(async () => blame) } as unknown as RepoClient, onOpenLineHistory });
      fireEvent.click(screen.getByRole("button", { name: "Blame" }));
      await screen.findByText("line 3");

      select(lineElement(2), lineElement(2));

      fireEvent.click(screen.getByRole("button", { name: "History of line 2 of a.txt" }));
      expect(onOpenLineHistory).toHaveBeenCalledWith("a.txt", "abc", 2, 2);
    });

    it("has no line history button without the callback", async () => {
      renderSection({ fetchDiff: vi.fn(async () => [hunk]) });
      await screen.findByText("six");
      expect(screen.queryByRole("button", { name: /selected lines/ })).toBeNull();
    });

    it("opens the history of the file's folder, and has none at the root", () => {
      const onOpenFolderHistory = vi.fn();
      const { unmount } = renderSection({ path: "src/ui/a.txt", onOpenFolderHistory });
      fireEvent.click(screen.getByRole("button", { name: "History of folder src/ui" }));
      expect(onOpenFolderHistory).toHaveBeenCalledWith("src/ui");
      unmount();

      renderSection({ path: "a.txt", onOpenFolderHistory });
      expect(screen.queryByRole("button", { name: /History of folder/ })).toBeNull();
    });
  });

  it("opens the file menu with the Menu key from a header button and returns focus to '…' on Escape", () => {
    const externalTools = { openInEditor: vi.fn(), reveal: vi.fn(), openDiffTool: vi.fn() } as unknown as ExternalToolsApi;
    renderSection({ externalTools, toolSource: { kind: "Commit", commitId: "abc" } as unknown as DiffSource });
    const blame = screen.getByRole("button", { name: "Blame" });
    blame.focus();
    const prevented = !fireEvent.keyDown(blame, { key: "F10", shiftKey: true });
    expect(prevented).toBe(true);
    expect(screen.getByRole("menuitem", { name: "Open in editor" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Open in editor" }), { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("button", { name: "Actions for a.txt" })).toHaveFocus();
  });

  it.each([
    ["Escape", (item: HTMLElement) => fireEvent.keyDown(item, { key: "Escape" })],
    ["Tab", (item: HTMLElement) => fireEvent.keyDown(item, { key: "Tab" })],
    ["item activation", (item: HTMLElement) => fireEvent.click(item)],
  ])("a header right-click menu returns focus to '…' on %s", (_label, close) => {
    const externalTools = { openInEditor: vi.fn(), reveal: vi.fn(), openDiffTool: vi.fn() } as unknown as ExternalToolsApi;
    renderSection({ externalTools, toolSource: { kind: "Commit", commitId: "abc" } as unknown as DiffSource });
    fireEvent.contextMenu(screen.getByText("a.txt"), { clientX: 0, clientY: 0 });
    close(screen.getByRole("menuitem", { name: "Open in editor" }));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("button", { name: "Actions for a.txt" })).toHaveFocus();
  });

  it("has no menu key handling without external tools", () => {
    renderSection();
    fireEvent.keyDown(screen.getByRole("button", { name: "Blame" }), { key: "ContextMenu" });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("a second click on '…' closes its menu", () => {
    const externalTools = { openInEditor: vi.fn(), reveal: vi.fn(), openDiffTool: vi.fn() } as unknown as ExternalToolsApi;
    renderSection({ externalTools, toolSource: { kind: "Commit", commitId: "abc" } as unknown as DiffSource });
    const actions = screen.getByRole("button", { name: "Actions for a.txt" });
    fireEvent.click(actions, { detail: 1 });
    expect(actions).toHaveAttribute("aria-expanded", "true");

    fireEvent.mouseDown(actions);
    fireEvent.click(actions, { detail: 1 });

    expect(screen.queryByRole("menu")).toBeNull();
    expect(actions).toHaveAttribute("aria-expanded", "false");
    expect(actions).toHaveFocus();
  });

  it("a menu opened by clicking '…' returns focus to it, even if the click did not focus it", () => {
    const externalTools = { openInEditor: vi.fn(), reveal: vi.fn(), openDiffTool: vi.fn() } as unknown as ExternalToolsApi;
    renderSection({ externalTools, toolSource: { kind: "Commit", commitId: "abc" } as unknown as DiffSource });
    screen.getByRole("button", { name: "Blame" }).focus();
    const actions = screen.getByRole("button", { name: "Actions for a.txt" });
    fireEvent.click(actions, { detail: 1 });
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "Open in editor" }), { key: "Escape" });
    expect(actions).toHaveFocus();
  });
});
