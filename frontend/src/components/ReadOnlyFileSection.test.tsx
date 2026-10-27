import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
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
});
