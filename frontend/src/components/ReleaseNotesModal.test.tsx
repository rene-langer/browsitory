import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ReleaseNotesModal, type ReleaseNotesEntry } from "./ReleaseNotesModal";

const ENTRIES: ReleaseNotesEntry[] = [
  {
    version: "0.6.0",
    date: "2026-09-01",
    groups: [
      {
        title: "Commit signing",
        intro: ["Commits can now be signed."],
        items: [
          { text: "Commits are signed with gpg or SSH." },
          { kind: "fixed", text: "A failed signature no longer makes a commit." },
        ],
      },
      { title: "Added", items: [{ text: "Feature A" }] },
    ],
  },
  {
    version: "0.5.0",
    date: "2026-08-26",
    groups: [{ title: "Changed", items: [{ text: "Behavior change" }] }],
  },
];

describe("ReleaseNotesModal", () => {
  it("renders each version, its date, and its bullets", () => {
    render(<ReleaseNotesModal entries={ENTRIES} onClose={vi.fn()} />);

    expect(screen.getByText("0.6.0", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("2026-09-01")).toBeInTheDocument();
    expect(screen.getByText("Feature A")).toBeInTheDocument();
    expect(screen.getByText("Commits are signed with gpg or SSH.")).toBeInTheDocument();
    expect(screen.getByText("0.5.0", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("Behavior change")).toBeInTheDocument();
  });

  it("shows each group under its own heading, with its intro", () => {
    render(<ReleaseNotesModal entries={ENTRIES} onClose={vi.fn()} />);

    expect(screen.getByRole("heading", { level: 4, name: "Commit signing" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 4, name: "Added" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 4, name: "Changed" })).toBeInTheDocument();
    expect(screen.getByText("Commits can now be signed.")).toBeInTheDocument();
  });

  it("labels a bullet with its kind", () => {
    render(<ReleaseNotesModal entries={ENTRIES} onClose={vi.fn()} />);

    const item = screen.getByText("A failed signature no longer makes a commit.").closest("li")!;
    expect(within(item).getByText("Fixed")).toBeInTheDocument();
    const plain = screen.getByText("Commits are signed with gpg or SSH.").closest("li")!;
    expect(plain).toHaveTextContent(/^Commits are signed with gpg or SSH\.$/);
  });

  it("renders inline code and bold text from the changelog", () => {
    render(
      <ReleaseNotesModal
        entries={[
          {
            version: "1.0.0",
            date: "2026-01-01",
            groups: [{ title: "Undo", items: [{ text: "Press **Ctrl+Z** when `commit.gpgSign` is set." }] }],
          },
        ]}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText("Ctrl+Z").tagName).toBe("STRONG");
    expect(screen.getByText("commit.gpgSign").tagName).toBe("CODE");
    expect(screen.getByText("Ctrl+Z").closest("li")).toHaveTextContent("Press Ctrl+Z when commit.gpgSign is set.");
  });

  it("names the dialog itself", () => {
    render(<ReleaseNotesModal entries={ENTRIES} onClose={vi.fn()} />);

    expect(screen.getByRole("dialog", { name: "Release notes" })).toBeInTheDocument();
  });

  it("shows a fallback message when there are no entries", () => {
    render(<ReleaseNotesModal entries={[]} onClose={vi.fn()} />);

    expect(screen.getByText("No release notes yet.")).toBeInTheDocument();
  });

  it("offers the third-party licences when the app can show them", () => {
    const onOpenThirdPartyLicences = vi.fn();
    const { rerender } = render(<ReleaseNotesModal entries={ENTRIES} onClose={vi.fn()} />);
    expect(screen.queryByRole("button", { name: "Third-party licences" })).not.toBeInTheDocument();

    rerender(
      <ReleaseNotesModal entries={ENTRIES} onClose={vi.fn()} onOpenThirdPartyLicences={onOpenThirdPartyLicences} />,
    );
    screen.getByRole("button", { name: "Third-party licences" }).click();

    expect(onOpenThirdPartyLicences).toHaveBeenCalledOnce();
  });

  it("calls onClose when the close button is clicked", () => {
    const onClose = vi.fn();
    render(<ReleaseNotesModal entries={ENTRIES} onClose={onClose} />);

    screen.getByRole("button", { name: "Close" }).click();

    expect(onClose).toHaveBeenCalledOnce();
  });
});
