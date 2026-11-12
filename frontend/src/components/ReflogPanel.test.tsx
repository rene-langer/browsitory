import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ReflogEntry } from "../ipc/RepoClient";
import { formatAbsoluteDate, formatShortDate } from "../lib/formatDate";
import { layoutInPane, ROW } from "../test/virtualLayout";
import { ReflogPanel } from "./ReflogPanel";

const entry: ReflogEntry = {
  reference: "refs/heads/main",
  oldId: "1111111111111111111111111111111111111111",
  newId: "2222222222222222222222222222222222222222",
  committerName: "Ada Lovelace",
  committerEmail: "ada@example.com",
  timestamp: 1_710_000_000,
  message: "commit: add recovery",
  summary: "add recovery",
};

function renderPanel(
  overrides: Partial<Parameters<typeof ReflogPanel>[0]> = {},
) {
  localStorage.removeItem("sidebar-reflog");
  const result = render(
    <ReflogPanel
      references={["HEAD", "refs/heads/main", "refs/remotes/origin/main"]}
      selectedReference="refs/heads/main"
      entries={[entry]}
      onSelectReference={vi.fn().mockResolvedValue(undefined)}
      onRestore={vi.fn().mockResolvedValue(undefined)}
      operationDisabled={false}
      {...overrides}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Reflog" }));
  return result;
}

describe("ReflogPanel", () => {
  it("prompts for a reference when none is selected", () => {
    renderPanel({ selectedReference: null, entries: [] });

    expect(screen.getByText(/Select a reference above/)).toBeInTheDocument();
  });

  it("shows an empty state when the selected reference has no reflog entries", () => {
    renderPanel({ entries: [] });

    expect(screen.getByText("No reflog entries for refs/heads/main.")).toBeInTheDocument();
  });

  it("lists HEAD and local branches with each selected entry's recovery details", () => {
    renderPanel();

    const selector = screen.getByRole("combobox", { name: "Reflog reference" });
    expect(within(selector).getByRole("option", { name: "HEAD" })).toBeInTheDocument();
    expect(within(selector).getByRole("option", { name: "refs/heads/main" })).toBeInTheDocument();
    expect(within(selector).queryByRole("option", { name: "refs/remotes/origin/main" })).not.toBeInTheDocument();
    // #775: two lines, with the full IDs, email, time and summary in the tooltip.
    expect(screen.getByText(entry.message)).toBeInTheDocument();
    expect(screen.getByText(`1111111 → 2222222 · Ada Lovelace · ${formatShortDate(entry.timestamp)}`)).toBeInTheDocument();
    const details = screen.getByTitle(/^Old ID: /).getAttribute("title")!;
    expect(details).toContain(`Old ID: ${entry.oldId}`);
    expect(details).toContain(`New ID: ${entry.newId}`);
    expect(details).toContain("Ada Lovelace <ada@example.com>");
    expect(details).toContain(formatAbsoluteDate(entry.timestamp));
    expect(details).toContain(entry.summary!);
    expect(screen.getByRole("button", { name: `Restore refs/heads/main to ${entry.newId}` })).toBeEnabled();
  });

  it("requires a named dialog confirmation that identifies the target before restoring", () => {
    const onRestore = vi.fn().mockResolvedValue(undefined);
    renderPanel({ onRestore });

    fireEvent.click(screen.getByRole("button", { name: `Restore refs/heads/main to ${entry.newId}` }));

    expect(onRestore).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog", { name: "Restore refs/heads/main" });
    expect(within(dialog).getByText("Restore refs/heads/main to 2222222222222222222222222222222222222222?")).toBeInTheDocument();
    // #287: Restore moves only the ref.
    expect(within(dialog).getByText("Your files and staged changes stay as they are.")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(onRestore).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: `Restore refs/heads/main to ${entry.newId}` }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Restore refs/heads/main" })).getByRole("button", { name: "Restore reflog entry" }));
    expect(onRestore).toHaveBeenCalledWith("refs/heads/main", entry.newId);
  });

  it("disables restore controls while a repository mutation is pending", () => {
    renderPanel({ operationDisabled: true });

    expect(screen.getByRole("button", { name: `Restore refs/heads/main to ${entry.newId}` })).toBeDisabled();
  });

  it("keeps repeated local-ref transitions distinct with labeled targets and unique restore controls", () => {
    const olderEntry: ReflogEntry = {
      ...entry,
      oldId: "3333333333333333333333333333333333333333",
      newId: "4444444444444444444444444444444444444444",
      timestamp: entry.timestamp - 60,
      message: "commit: older recovery",
      summary: "older recovery",
    };
    renderPanel({ entries: [entry, olderEntry] });

    expect(screen.getByTitle(new RegExp(`^Old ID: ${entry.oldId}\\s`))).toBeInTheDocument();
    expect(screen.getByTitle(new RegExp(`^Old ID: ${olderEntry.oldId}\\s`))).toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Restore refs/heads/main to ${entry.newId}` })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: `Restore refs/heads/main to ${olderEntry.newId}` })).toBeInTheDocument();
  });
});

// #775: two fixed lines per entry, so the list can be windowed like the Stashes rows.
describe("ReflogPanel — rows", () => {
  it("shows the message, then the short IDs, committer and date, with the full details as a tooltip", () => {
    renderPanel();

    const row = screen.getByText(entry.message).closest("li")!;
    expect(
      within(row).getByText(`1111111 → 2222222 · Ada Lovelace · ${formatShortDate(entry.timestamp)}`),
    ).toBeInTheDocument();
    const details = within(row).getByTitle(/^Old ID: /);
    expect(details).toHaveAttribute(
      "title",
      [
        `Old ID: ${entry.oldId}`,
        `New ID: ${entry.newId}`,
        "Ada Lovelace <ada@example.com>",
        formatAbsoluteDate(entry.timestamp),
        entry.message,
        entry.summary,
      ].join("\n"),
    );
    expect(within(row).getByRole("button", { name: `Restore refs/heads/main to ${entry.newId}` })).toHaveTextContent(
      "Restore refs/heads/main",
    );
  });

  it("leaves the summary out of the tooltip when the entry has none", () => {
    renderPanel({ entries: [{ ...entry, summary: null }] });

    expect(screen.getByTitle(/^Old ID: /).getAttribute("title")).toBe(
      [
        `Old ID: ${entry.oldId}`,
        `New ID: ${entry.newId}`,
        "Ada Lovelace <ada@example.com>",
        formatAbsoluteDate(entry.timestamp),
        entry.message,
      ].join("\n"),
    );
  });
});

// #775: only the rows in view render, as in the commit graph (#685).
describe("ReflogPanel — windowed rows", () => {
  const many: ReflogEntry[] = Array.from({ length: 1000 }, (_, n) => ({
    ...entry,
    newId: `${n}`.padStart(40, "a"),
    message: `commit: step ${n}`,
  }));

  function renderInPane() {
    localStorage.removeItem("sidebar-reflog");
    const layout = layoutInPane();
    const onRestore = vi.fn().mockResolvedValue(undefined);
    render(
      <ReflogPanel
        references={["HEAD", "refs/heads/main"]}
        selectedReference="refs/heads/main"
        entries={many}
        onSelectReference={vi.fn().mockResolvedValue(undefined)}
        onRestore={onRestore}
        operationDisabled={false}
      />,
      { container: layout.pane },
    );
    fireEvent.click(screen.getByRole("button", { name: "Reflog" }));
    const rows = () => [...layout.pane.querySelectorAll<HTMLElement>("ul > li")];
    return { ...layout, rows, onRestore };
  }

  it("renders the rows in view plus a margin, and pads for the rest", () => {
    const { rows, restore } = renderInPane();
    try {
      const rendered = rows();
      // Rows 0–9 are in view, 20 more below them.
      expect(rendered).toHaveLength(30);
      expect(rendered[0]).toHaveTextContent("commit: step 0");
      expect(rendered[1]).toHaveAttribute("aria-posinset", "2");
      expect(rendered[1]).toHaveAttribute("aria-setsize", "1000");
      const list = rendered[0].parentElement!;
      expect(list.style.paddingTop).toBe("0px");
      expect(list.style.paddingBottom).toBe(`${(1000 - 30) * ROW}px`);
    } finally {
      restore();
    }
  });

  it("restores an entry from the scrolled-to part", () => {
    const { rows, scrollTo, onRestore, restore } = renderInPane();
    try {
      scrollTo(ROW * 500);
      const rendered = rows();
      expect(rendered[0]).toHaveAttribute("data-row-index", "480");
      expect(rendered[0]).toHaveTextContent("commit: step 480");
      expect(rendered.at(-1)).toHaveAttribute("data-row-index", "529");
      expect(rendered[0].parentElement!.style.paddingTop).toBe(`${480 * ROW}px`);

      fireEvent.click(screen.getByRole("button", { name: `Restore refs/heads/main to ${many[505].newId}` }));
      fireEvent.click(
        within(screen.getByRole("dialog", { name: "Restore refs/heads/main" })).getByRole("button", {
          name: "Restore reflog entry",
        }),
      );
      expect(onRestore).toHaveBeenCalledWith("refs/heads/main", many[505].newId);
    } finally {
      restore();
    }
  });
});
