import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { BlameLine } from "../ipc/RepoClient";
import { BlameView } from "./BlameView";

const lines: BlameLine[] = [
  {
    lineNumber: 1,
    content: "line one",
    commitId: "aaa111",
    shortId: "aaa1111",
    authorName: "Rene",
    timestamp: 1,
  },
  {
    lineNumber: 2,
    content: "line two",
    commitId: "bbb222",
    shortId: "bbb2222",
    authorName: "Someone",
    timestamp: 2,
  },
];

describe("BlameView", () => {
  it("renders each line's number, short id, author, and content", () => {
    render(<BlameView lines={lines} onSelectRow={vi.fn()} />);

    expect(screen.getByText("aaa1111")).toBeInTheDocument();
    expect(screen.getByText("Rene")).toBeInTheDocument();
    expect(screen.getByText("line one")).toBeInTheDocument();
    expect(screen.getByText("bbb2222")).toBeInTheDocument();
    expect(screen.getByText("Someone")).toBeInTheDocument();
    expect(screen.getByText("line two")).toBeInTheDocument();
  });

  it("clicking a line calls onSelectRow with that line's commit id", () => {
    const onSelectRow = vi.fn();
    render(<BlameView lines={lines} onSelectRow={onSelectRow} />);

    fireEvent.click(screen.getByText("line two").closest("tr")!);

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "bbb222" });
  });
});

describe("BlameView layout (#986)", () => {
  // Noon UTC, so the date is the same day in every time zone the tests run in.
  const run: BlameLine[] = [
    { lineNumber: 1, content: "function a() {", commitId: "aaa111", shortId: "aaa1111", authorName: "Rene", timestamp: 1_699_963_200 },
    { lineNumber: 2, content: "    return 1;", commitId: "aaa111", shortId: "aaa1111", authorName: "Rene", timestamp: 1_699_963_200 },
    { lineNumber: 3, content: "}", commitId: "bbb222", shortId: "bbb2222", authorName: "Someone", timestamp: 1_599_998_400 },
  ];

  it("shows the hash, author and date once per run of lines from the same commit", () => {
    render(<BlameView lines={run} onSelectRow={vi.fn()} />);

    expect(screen.getAllByText("aaa1111")).toHaveLength(1);
    expect(screen.getAllByText("Rene")).toHaveLength(1);
    expect(screen.getAllByText("2023-11-14")).toHaveLength(1);
    expect(screen.getAllByText("bbb2222")).toHaveLength(1);
    expect(screen.getByText("2020-09-13")).toBeInTheDocument();
  });

  it("keeps a line's indentation in its code cell", () => {
    const { container } = render(<BlameView lines={run} onSelectRow={vi.fn()} />);

    const code = container.querySelector('[data-line-number="2"] [data-blame-code]');
    expect(code?.textContent).toBe("    return 1;");
  });

  it("only the hash is a link: it selects the commit, a click on the code does not", () => {
    const onSelectRow = vi.fn();
    render(<BlameView lines={run} onSelectRow={onSelectRow} />);

    fireEvent.click(screen.getByText("}"));
    expect(onSelectRow).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "bbb2222" }));
    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "bbb222" });
    expect(screen.getAllByRole("button")).toHaveLength(2);
  });

  it("colors the code with the diff's syntax highlighting", async () => {
    const { container } = render(<BlameView lines={run} path="a.ts" onSelectRow={vi.fn()} />);

    await waitFor(() => expect(container.querySelector('[style*="--shiki-token"]')).not.toBeNull(), { timeout: 15000 });
    const code = container.querySelector('[data-line-number="2"] [data-blame-code]');
    expect(code?.textContent).toBe("    return 1;");
  }, 20000);
});
