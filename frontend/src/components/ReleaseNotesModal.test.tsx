import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ReleaseNotesModal, type ReleaseNotesEntry, type ReleaseNotesItem } from "./ReleaseNotesModal";

const ENTRIES: ReleaseNotesEntry[] = [
  {
    version: "0.6.0",
    date: "2026-09-01",
    groups: [
      {
        title: "Commit signing",
        blocks: [
          { paragraph: "Commits can now be signed." },
          {
            items: [
              { text: "Commits are signed with gpg or SSH." },
              { kind: "fixed", text: "a failed signature no longer makes a commit." },
            ],
          },
          { paragraph: "A closing paragraph." },
        ],
      },
      { title: "Added", blocks: [{ items: [{ text: "Feature A" }] }] },
    ],
  },
  {
    version: "0.5.0",
    date: "2026-08-26",
    groups: [{ title: "Changed", blocks: [{ items: [{ text: "Behavior change" }] }] }],
  },
];

function renderItems(items: ReleaseNotesItem[]) {
  return render(
    <ReleaseNotesModal
      entries={[{ version: "1.0.0", date: "2026-01-01", groups: [{ title: "Topic", blocks: [{ items }] }] }]}
      onClose={vi.fn()}
    />,
  );
}

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

  it("shows each group under its own heading, with its paragraphs in order", () => {
    render(<ReleaseNotesModal entries={ENTRIES} onClose={vi.fn()} />);

    expect(screen.getByRole("heading", { level: 4, name: "Commit signing" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 4, name: "Added" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 4, name: "Changed" })).toBeInTheDocument();
    const intro = screen.getByText("Commits can now be signed.");
    const list = screen.getByText("Commits are signed with gpg or SSH.").closest("ul")!;
    const outro = screen.getByText("A closing paragraph.");
    expect(intro.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(list.compareDocumentPosition(outro) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("labels each kind of bullet, readable as one sentence", () => {
    renderItems([
      { kind: "added", text: "one" },
      { kind: "changed", text: "two" },
      { kind: "fixed", text: "three" },
      { kind: "removed", text: "four" },
      { kind: "deprecated", text: "five" },
      { kind: "security", text: "six" },
      { text: "plain" },
    ]);

    const items = screen.getAllByRole("listitem");
    expect(items.map((li) => li.textContent)).toEqual([
      "Added: one",
      "Changed: two",
      "Fixed: three",
      "Removed: four",
      "Deprecated: five",
      "Security: six",
      "plain",
    ]);
  });

  it("gives the kind label a colon that only screen readers get", () => {
    renderItems([{ kind: "fixed", text: "three" }]);

    const label = within(screen.getByRole("listitem")).getByText("Fixed", { exact: false });
    expect(label).toHaveTextContent(/^Fixed:$/);
    const colon = within(label).getByText(":");
    expect(colon).not.toBe(label);
    expect(colon.className).toMatch(/srOnly/);
  });

  it("leaves out releases with nothing for users", () => {
    render(
      <ReleaseNotesModal
        entries={[{ version: "0.7.0", date: "2026-10-01", groups: [] }, ...ENTRIES]}
        onClose={vi.fn()}
      />,
    );

    expect(screen.queryByText("0.7.0", { exact: false })).not.toBeInTheDocument();
    expect(screen.getByText("0.6.0", { exact: false })).toBeInTheDocument();
  });

  it("shows the fallback when no release has notes for users", () => {
    render(<ReleaseNotesModal entries={[{ version: "0.7.0", date: "2026-10-01", groups: [] }]} onClose={vi.fn()} />);

    expect(screen.getByText("No release notes yet.")).toBeInTheDocument();
  });

  it("renders sub-items as a nested list", () => {
    renderItems([{ text: "Parent", children: [{ kind: "fixed", text: "child" }] }]);

    const child = screen.getByText("child").closest("li")!;
    expect(child).toHaveTextContent("Fixed: child");
    expect(child.parentElement!.closest("li")).toHaveTextContent(/^Parent/);
  });

  it("renders inline code and bold text from the changelog", () => {
    renderItems([{ text: "Press **Ctrl+Z** when `commit.gpgSign` is set." }]);

    expect(screen.getByText("Ctrl+Z").tagName).toBe("STRONG");
    expect(screen.getByText("commit.gpgSign").tagName).toBe("CODE");
    expect(screen.getByRole("listitem")).toHaveTextContent("Press Ctrl+Z when commit.gpgSign is set.");
  });

  it("handles stars inside bold, code inside bold, and unmatched markers", () => {
    renderItems([{ text: "**a*b** and **press `s` now** and **open and `x" }]);

    expect(screen.getByText("a*b").tagName).toBe("STRONG");
    const code = screen.getByText("s");
    expect(code.tagName).toBe("CODE");
    expect(code.parentElement!.tagName).toBe("STRONG");
    expect(screen.getByRole("listitem")).toHaveTextContent("a*b and press s now and **open and `x");
  });

  it("shows HTML-like text as text", () => {
    const { container } = renderItems([{ text: "<b>not bold</b> <img src=x onerror=alert(1)>" }]);

    expect(screen.getByRole("listitem")).toHaveTextContent("<b>not bold</b> <img src=x onerror=alert(1)>");
    expect(container.ownerDocument.querySelector("li b, li img")).toBeNull();
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

  it("links to the user guide when the app can open it (#320)", () => {
    const onOpenUserGuide = vi.fn();
    const { rerender } = render(<ReleaseNotesModal entries={ENTRIES} onClose={vi.fn()} />);
    expect(screen.queryByRole("button", { name: "User guide" })).not.toBeInTheDocument();

    rerender(<ReleaseNotesModal entries={ENTRIES} onClose={vi.fn()} onOpenUserGuide={onOpenUserGuide} />);
    screen.getByRole("button", { name: "User guide" }).click();

    expect(onOpenUserGuide).toHaveBeenCalledOnce();
  });

  it("calls onClose when the close button is clicked", () => {
    const onClose = vi.fn();
    render(<ReleaseNotesModal entries={ENTRIES} onClose={onClose} />);

    screen.getByRole("button", { name: "Close" }).click();

    expect(onClose).toHaveBeenCalledOnce();
  });
});
