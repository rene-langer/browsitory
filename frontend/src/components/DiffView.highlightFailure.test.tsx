import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DiffHunk, RepoClient } from "../ipc/RepoClient";
import { installGlobalErrorLogging } from "../lib/logger";
import { DiffView } from "./DiffView";

// Make the highlighter itself fail, so the real useHunkTokens -> highlightLines path rejects.
vi.mock("shiki/core", async (importOriginal) => {
  const actual = await importOriginal<typeof import("shiki/core")>();
  return { ...actual, createHighlighterCore: vi.fn().mockRejectedValue(new Error("boom")) };
});

const hunk: DiffHunk = {
  oldStart: 1, oldLines: 1, newStart: 1, newLines: 1,
  lines: [{ origin: "Add", content: "B\n" }],
};

describe("DiffView highlight failure", () => {
  let uninstall: (() => void) | undefined;
  afterEach(() => uninstall?.());

  it("renders plain text and logs when highlighting fails", async () => {
    const logFrontendError = vi.fn().mockResolvedValue(undefined);
    uninstall = installGlobalErrorLogging({ logFrontendError } as unknown as RepoClient);

    const { container } = render(<DiffView hunks={[hunk]} path="f.ts" />);

    expect(await screen.findByText("B")).toBeInTheDocument();
    await waitFor(() => expect(logFrontendError).toHaveBeenCalledWith("Syntax highlighting failed", expect.any(Error)));
    expect(container.querySelector('[style*="--shiki-token"]')).toBeNull();
    expect(screen.getByText("B")).toBeInTheDocument();
  });
});
