import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { WHOLE_FILE_CONTEXT_LINES, type DiffSource, type RepoClient } from "../ipc/RepoClient";
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
    await waitFor(() => expect(props.fetchDiff).toHaveBeenCalledWith({ ignoreWhitespace: false, contextLines: 3 }));

    rerender(<ReadOnlyFileSection {...props} diffKey="k2" />);
    await waitFor(() => expect(props.fetchDiff).toHaveBeenCalledTimes(2));
  });

  it("Show whole file refetches the commit or compare diff with whole-file context", async () => {
    const hunk = { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add" as const, content: "x\n" }] };
    const { props } = renderSection({ fetchDiff: vi.fn(async () => [hunk]) });
    fireEvent.click(await screen.findByRole("button", { name: "Show whole file" }));
    await waitFor(() =>
      expect(props.fetchDiff).toHaveBeenLastCalledWith({ ignoreWhitespace: false, contextLines: WHOLE_FILE_CONTEXT_LINES }),
    );
    expect(screen.getByRole("button", { name: "Show whole file" })).toHaveAttribute("aria-pressed", "true");
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
