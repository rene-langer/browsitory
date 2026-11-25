import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { blameOf } from "../test/blame";
import { layoutInPane, ROW } from "../test/virtualLayout";
import { cascadedValue, rulesOf, subjectOf } from "../test/cssCascade";
import { BlameView } from "./BlameView";
import styles from "./BlameView.module.css";
import blameCss from "./BlameView.module.css?raw";

const lines = blameOf([
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
]);

describe("BlameView", () => {
  it("renders each line's number, short id, author, and content", () => {
    render(<BlameView blame={lines} onSelectRow={vi.fn()} />);

    expect(screen.getByText("aaa1111")).toBeInTheDocument();
    expect(screen.getByText("Rene")).toBeInTheDocument();
    expect(screen.getByText("line one")).toBeInTheDocument();
    expect(screen.getByText("bbb2222")).toBeInTheDocument();
    expect(screen.getByText("Someone")).toBeInTheDocument();
    expect(screen.getByText("line two")).toBeInTheDocument();
  });

  it("clicking a line's commit id calls onSelectRow with that commit", () => {
    const onSelectRow = vi.fn();
    render(<BlameView blame={lines} onSelectRow={onSelectRow} />);

    fireEvent.click(screen.getByRole("button", { name: "bbb2222" }));

    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "bbb222" });
  });
});

describe("BlameView layout (#986)", () => {
  // Noon UTC, so the date is the same day in every time zone the tests run in.
  const run = blameOf([
    { lineNumber: 1, content: "function a() {", commitId: "aaa111", shortId: "aaa1111", authorName: "Rene", timestamp: 1_699_963_200 },
    { lineNumber: 2, content: "    return 1;", commitId: "aaa111", shortId: "aaa1111", authorName: "Rene", timestamp: 1_699_963_200 },
    { lineNumber: 3, content: "}", commitId: "bbb222", shortId: "bbb2222", authorName: "Someone", timestamp: 1_599_998_400 },
  ]);

  it("shows the hash, author and date once per run of lines from the same commit", () => {
    render(<BlameView blame={run} onSelectRow={vi.fn()} />);

    expect(screen.getAllByText("aaa1111")).toHaveLength(1);
    expect(screen.getAllByText("Rene")).toHaveLength(1);
    expect(screen.getAllByText("2023-11-14")).toHaveLength(1);
    expect(screen.getAllByText("bbb2222")).toHaveLength(1);
    expect(screen.getByText("2020-09-13")).toBeInTheDocument();
  });

  it("cuts a long author name with an ellipsis: on an inline-block span, as a cell's max-width does nothing (#986)", () => {
    render(<BlameView blame={run} onSelectRow={vi.fn()} />);
    const name = screen.getByText("Rene");
    expect(name.tagName).toBe("SPAN");
    expect(name.className).toBe(styles.authorName);
    const rules = rulesOf(blameCss).filter(({ selector }) => subjectOf(selector) === ".authorName");
    expect(cascadedValue(rules, "display")).toBe("inline-block");
    expect(cascadedValue(rules, "max-width")).toBe("16ch");
    expect(cascadedValue(rules, "overflow")).toBe("hidden");
    expect(cascadedValue(rules, "text-overflow")).toBe("ellipsis");
  });

  it("keeps a line's indentation in its code cell", () => {
    const { container } = render(<BlameView blame={run} onSelectRow={vi.fn()} />);

    const code = container.querySelector('[data-line-number="2"] [data-blame-code]');
    expect(code?.textContent).toBe("    return 1;");
  });

  it("only the hash is a link: it selects the commit, a click on the code does not", () => {
    const onSelectRow = vi.fn();
    render(<BlameView blame={run} onSelectRow={onSelectRow} />);

    fireEvent.click(screen.getByText("}"));
    expect(onSelectRow).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "bbb2222" }));
    expect(onSelectRow).toHaveBeenCalledWith({ commitId: "bbb222" });
    expect(screen.getAllByRole("button")).toHaveLength(2);
  });

  it("colors the code with the diff's syntax highlighting", async () => {
    const { container } = render(<BlameView blame={run} path="a.ts" onSelectRow={vi.fn()} />);

    await waitFor(() => expect(container.querySelector('[style*="--shiki-token"]')).not.toBeNull(), { timeout: 15000 });
    const code = container.querySelector('[data-line-number="2"] [data-blame-code]');
    expect(code?.textContent).toBe("    return 1;");
  }, 20000);
});

// #1158: a long file's blame renders the lines in view, not one row per line of the file.
describe("BlameView windowing (#1158)", () => {
  const long = blameOf(
    Array.from({ length: 5000 }, (_, i) => ({
      content: `line ${i + 1}`,
      commitId: i < 2500 ? "aaa111" : "bbb222",
      shortId: i < 2500 ? "aaa1111" : "bbb2222",
      authorName: "Rene",
      timestamp: 1,
    })),
  );
  // The element whose padding stands in for the lines not rendered: the table's parent.
  const isList = (element: Element) => element.firstElementChild?.tagName === "TABLE";

  it("renders only the lines in view of a long file, keeping the whole file's height", () => {
    const { pane, restore } = layoutInPane(260, { rowTag: "TR", isList });
    try {
      render(<BlameView blame={long} onSelectRow={vi.fn()} />, { container: pane });

      const rows = pane.querySelectorAll("tr[data-line-number]");
      expect(rows.length).toBeLessThan(200);
      expect(screen.getByText("line 1")).toBeInTheDocument();
      expect(screen.queryByText("line 2500")).toBeNull();
      const list = pane.querySelector("table")!.parentElement!;
      const padding = parseFloat(list.style.paddingTop || "0") + parseFloat(list.style.paddingBottom || "0");
      expect(padding + rows.length * ROW).toBe(5000 * ROW);
    } finally {
      restore();
    }
  });

  it("renders the lines further down once scrolled there, with their own numbers and commit", () => {
    const { pane, scrollTo, restore } = layoutInPane(260, { rowTag: "TR", isList });
    try {
      render(<BlameView blame={long} onSelectRow={vi.fn()} />, { container: pane });
      scrollTo(ROW * 2495);

      expect(screen.getByText("line 2501")).toBeInTheDocument();
      expect(pane.querySelector('[data-line-number="2501"] [data-blame-code]')?.textContent).toBe("line 2501");
      expect(screen.queryByText("line 1")).toBeNull();
      // Line 2501 starts the second commit's run: its id shows there.
      expect(screen.getByRole("button", { name: "bbb2222" })).toBeInTheDocument();
    } finally {
      restore();
    }
  });

  it("marks a run's start with a shadow, not a border, so every row has one height", () => {
    const rules = rulesOf(blameCss).filter(({ selector }) => selector.includes(".runStart"));
    expect(rules.length).toBeGreaterThan(0);
    expect(cascadedValue(rules, "border-top")).toBeNull();
    expect(cascadedValue(rules, "box-shadow")).toBe("inset 0 1px 0 var(--color-border)");
  });
});
