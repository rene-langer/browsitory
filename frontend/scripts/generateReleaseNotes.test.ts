import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { isMainModule, parseChangelog } from "./generateReleaseNotes.mjs";

const SAMPLE = `# Changelog

## [Unreleased]

### Added
- work in progress, should not appear

## [0.6.0] - 2026-09-01

### Added
- Feature A
- Feature B

### Fixed
- Bug fix A

## [0.5.0] - 2026-08-26

### Changed
- Behavior change
`;

const TOPICS = `# Changelog

## [Unreleased]

### Something new
- Fixed: not released yet

## [0.7.0] - 2026-10-21

### Commit signing

Commits can now be signed.
This paragraph wraps.

- Commits are signed with gpg or SSH,
  using your existing agent.
- Fixed: a failed signature no longer
  makes a commit (#12).
- Changed: \`commit.gpgSign\` is read per repository.
- Windows: keeps its prefix because it is not a kind.

### Internal: typed params

- Added: a developer-only change.

### Internal

- Tests run faster.

### Worktrees

- Internal: a developer-only bullet in a user topic.
- Added: worktrees can be locked.

### Fixed

- Bug fix A

### Security

- Paths are validated.

### Fixed

- Bug fix B
`;

describe("parseChangelog", () => {
  it("skips the Unreleased section", () => {
    const entries = parseChangelog(SAMPLE);
    expect(entries.map((e) => e.version)).toEqual(["0.6.0", "0.5.0"]);
  });

  it("parses Keep a Changelog headings into groups in changelog order", () => {
    const entries = parseChangelog(SAMPLE);
    expect(entries[0]).toEqual({
      version: "0.6.0",
      date: "2026-09-01",
      groups: [
        { title: "Added", items: [{ text: "Feature A" }, { text: "Feature B" }] },
        { title: "Fixed", items: [{ text: "Bug fix A" }] },
      ],
    });
    expect(entries[1].groups).toEqual([{ title: "Changed", items: [{ text: "Behavior change" }] }]);
  });

  it("joins indented continuation lines onto their bullet", () => {
    const entries = parseChangelog(
      "## [1.0.0] - 2026-01-01\n\n### Fixed\n- First line of a long note\n  continues here\n  and here.\n- Next bullet\n",
    );
    expect(entries[0].groups[0].items).toEqual([
      { text: "First line of a long note continues here and here." },
      { text: "Next bullet" },
    ]);
  });

  it("turns each topic heading into a group, with the bullet prefix as its kind", () => {
    const [entry] = parseChangelog(TOPICS);
    expect(entry.version).toBe("0.7.0");
    expect(entry.groups[0]).toEqual({
      title: "Commit signing",
      intro: ["Commits can now be signed. This paragraph wraps."],
      items: [
        { text: "Commits are signed with gpg or SSH, using your existing agent." },
        { kind: "fixed", text: "A failed signature no longer makes a commit (#12)." },
        { kind: "changed", text: "`commit.gpgSign` is read per repository." },
        { text: "Windows: keeps its prefix because it is not a kind." },
      ],
    });
  });

  it("leaves out Internal sections and Internal bullets", () => {
    const [entry] = parseChangelog(TOPICS);
    expect(entry.groups.map((g) => g.title)).toEqual(["Commit signing", "Worktrees", "Fixed", "Security"]);
    expect(entry.groups[1].items).toEqual([{ kind: "added", text: "Worktrees can be locked." }]);
  });

  it("merges repeated headings within a release into one group", () => {
    const [entry] = parseChangelog(TOPICS);
    expect(entry.groups[2]).toEqual({ title: "Fixed", items: [{ text: "Bug fix A" }, { text: "Bug fix B" }] });
  });

  it("drops a group whose bullets are all internal", () => {
    const [entry] = parseChangelog("## [1.0.0] - 2026-01-01\n\n### Tooling\n- Internal: faster CI.\n");
    expect(entry.groups).toEqual([]);
  });

  it("returns an empty array for a changelog with only Unreleased", () => {
    const entries = parseChangelog("# Changelog\n\n## [Unreleased]\n\n### Added\n- x\n");
    expect(entries).toEqual([]);
  });
});

describe("the real CHANGELOG.md", () => {
  const changelog = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "..", "..", "CHANGELOG.md"), "utf-8");
  const entries = parseChangelog(changelog);

  it("yields release notes for every released version", () => {
    expect(entries.length).toBeGreaterThan(0);
    for (const entry of entries) {
      const items = entry.groups.flatMap((g) => g.items);
      expect(items.length, `release ${entry.version} has no release notes`).toBeGreaterThan(0);
    }
  });

  it("includes the topic sections of 0.3.0", () => {
    const entry = entries.find((e) => e.version === "0.3.0");
    expect(entry).toBeDefined();
    const titles = entry!.groups.map((g) => g.title);
    expect(titles).toContain("Commit signing");
    expect(titles).toContain("Undo and redo");
    const signing = entry!.groups.find((g) => g.title === "Commit signing")!;
    expect(signing.items.length).toBeGreaterThanOrEqual(2);
  });
});

describe("isMainModule", () => {
  it("recognizes an entry-point path when its file URL encodes spaces", () => {
    expect(isMainModule("file:///tmp/release%20notes.mjs", "/tmp/release notes.mjs")).toBe(true);
  });
});
