import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { DiffView } from "./DiffView";

vi.mock("../lib/highlight", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/highlight")>();
  return { ...actual, highlightLines: vi.fn().mockRejectedValue(new Error("boom")) };
});

const hunk: DiffHunk = {
  oldStart: 1, oldLines: 1, newStart: 1, newLines: 1,
  lines: [{ origin: "Add", content: "B\n" }],
};

describe("DiffView highlight failure", () => {
  it("renders plain text when highlighting fails", async () => {
    render(<DiffView hunks={[hunk]} path="f.ts" />);
    expect(await screen.findByText("B")).toBeInTheDocument();
  });
});
