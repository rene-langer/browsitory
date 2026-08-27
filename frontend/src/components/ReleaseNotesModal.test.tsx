import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ReleaseNotesModal, type ReleaseNotesEntry } from "./ReleaseNotesModal";

const ENTRIES: ReleaseNotesEntry[] = [
  {
    version: "0.6.0",
    date: "2026-09-01",
    sections: { added: ["Feature A"], fixed: ["Bug fix A"] },
  },
  {
    version: "0.5.0",
    date: "2026-08-26",
    sections: { changed: ["Behavior change"] },
  },
];

describe("ReleaseNotesModal", () => {
  it("renders each version, its date, and its bullets", () => {
    render(<ReleaseNotesModal entries={ENTRIES} onClose={vi.fn()} />);

    expect(screen.getByText("0.6.0", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("2026-09-01")).toBeInTheDocument();
    expect(screen.getByText("Feature A")).toBeInTheDocument();
    expect(screen.getByText("Bug fix A")).toBeInTheDocument();
    expect(screen.getByText("0.5.0", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("Behavior change")).toBeInTheDocument();
  });

  it("omits section headings with no entries", () => {
    render(<ReleaseNotesModal entries={ENTRIES} onClose={vi.fn()} />);

    // 0.6.0 has no "Changed" or "Removed" bullets — those headings shouldn't render for it.
    expect(screen.queryByText("Removed")).not.toBeInTheDocument();
  });

  it("shows a fallback message when there are no entries", () => {
    render(<ReleaseNotesModal entries={[]} onClose={vi.fn()} />);

    expect(screen.getByText("No release notes yet.")).toBeInTheDocument();
  });

  it("calls onClose when the close button is clicked", () => {
    const onClose = vi.fn();
    render(<ReleaseNotesModal entries={ENTRIES} onClose={onClose} />);

    screen.getByRole("button", { name: "Close" }).click();

    expect(onClose).toHaveBeenCalledOnce();
  });
});
