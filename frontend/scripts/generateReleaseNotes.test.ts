import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it, vi } from "vitest";
import { isMainModule, parseChangelog } from "./generateReleaseNotes.mjs";

type Item = { kind?: string; text: string; children?: Item[] };
type Block = { paragraph: string } | { items: Item[] };
type Group = { title: string; blocks: Block[] };
type Entry = { version: string; date: string; groups: Group[] };

const parse = (markdown: string, onWarning = vi.fn()): Entry[] => parseChangelog(markdown, { onWarning });
const itemsOf = (group: Group): Item[] => group.blocks.flatMap((b) => ("items" in b ? b.items : []));

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
  Its continuation goes too.
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
    expect(parse(SAMPLE).map((e) => e.version)).toEqual(["0.6.0", "0.5.0"]);
  });

  it("parses Keep a Changelog headings into groups in changelog order", () => {
    const entries = parse(SAMPLE);
    expect(entries[0]).toEqual({
      version: "0.6.0",
      date: "2026-09-01",
      groups: [
        { title: "Added", blocks: [{ items: [{ text: "Feature A" }, { text: "Feature B" }] }] },
        { title: "Fixed", blocks: [{ items: [{ text: "Bug fix A" }] }] },
      ],
    });
    expect(entries[1].groups).toEqual([{ title: "Changed", blocks: [{ items: [{ text: "Behavior change" }] }] }]);
  });

  it("joins indented continuation lines onto their bullet", () => {
    const [entry] = parse(
      "## [1.0.0] - 2026-01-01\n\n### Fixed\n- First line of a long note\n  continues here\n  and here.\n- Next bullet\n",
    );
    expect(itemsOf(entry.groups[0])).toEqual([
      { text: "First line of a long note continues here and here." },
      { text: "Next bullet" },
    ]);
  });

  it("reads CRLF line endings like LF", () => {
    const [entry] = parse("## [1.0.0] - 2026-01-01\r\n\r\n### Fixed\r\n- Fixed: one\r\n  two\r\n- three\r\n");
    expect(entry.groups).toEqual([
      { title: "Fixed", blocks: [{ items: [{ kind: "fixed", text: "one two" }, { text: "three" }] }] },
    ]);
  });

  it("turns each topic heading into a group, with the bullet prefix as its kind", () => {
    const [entry] = parse(TOPICS);
    expect(entry.version).toBe("0.7.0");
    expect(entry.groups[0]).toEqual({
      title: "Commit signing",
      blocks: [
        { paragraph: "Commits can now be signed. This paragraph wraps." },
        {
          items: [
            { text: "Commits are signed with gpg or SSH, using your existing agent." },
            { kind: "fixed", text: "a failed signature no longer makes a commit (#12)." },
            { kind: "changed", text: "`commit.gpgSign` is read per repository." },
            { text: "Windows: keeps its prefix because it is not a kind." },
          ],
        },
      ],
    });
  });

  it("recognises all six kinds and leaves the text as written", () => {
    const [entry] = parse(
      [
        "## [1.0.0] - 2026-01-01",
        "### Topic",
        "- Added: macOS support",
        "- Changed: gpg is found on PATH",
        "- Fixed: a",
        "- Removed: b",
        "- Deprecated: c",
        "- Security: d",
      ].join("\n"),
    );
    expect(itemsOf(entry.groups[0])).toEqual([
      { kind: "added", text: "macOS support" },
      { kind: "changed", text: "gpg is found on PATH" },
      { kind: "fixed", text: "a" },
      { kind: "removed", text: "b" },
      { kind: "deprecated", text: "c" },
      { kind: "security", text: "d" },
    ]);
  });

  it("leaves out Internal sections and Internal bullets with their continuation lines", () => {
    const [entry] = parse(TOPICS);
    expect(entry.groups.map((g) => g.title)).toEqual(["Commit signing", "Worktrees", "Fixed", "Security"]);
    expect(itemsOf(entry.groups[1])).toEqual([{ kind: "added", text: "worktrees can be locked." }]);
  });

  it("merges repeated headings within a release into one group", () => {
    const [entry] = parse(TOPICS);
    expect(itemsOf(entry.groups[2])).toEqual([{ text: "Bug fix A" }, { text: "Bug fix B" }]);
  });

  it("drops a group whose bullets are all internal", () => {
    const [entry] = parse("## [1.0.0] - 2026-01-01\n\n### Tooling\n- Internal: faster CI.\n### Topic\n- kept\n");
    expect(entry.groups.map((g) => g.title)).toEqual(["Topic"]);
  });

  it("turns indented bullets into sub-items of the bullet above", () => {
    const [entry] = parse(
      [
        "## [1.0.0] - 2026-01-01",
        "### Topic",
        "- Parent",
        "  continues.",
        "  - Fixed: first sub",
        "    wraps.",
        "  - second sub",
        "- Next",
      ].join("\n"),
    );
    expect(itemsOf(entry.groups[0])).toEqual([
      {
        text: "Parent continues.",
        children: [{ kind: "fixed", text: "first sub wraps." }, { text: "second sub" }],
      },
      { text: "Next" },
    ]);
  });

  it("joins unindented text right after a bullet onto it, and keeps a paragraph after the list", () => {
    const [entry] = parse(
      ["## [1.0.0] - 2026-01-01", "### Topic", "- A bullet", "lazily continued.", "", "A closing paragraph", "on two lines."].join(
        "\n",
      ),
    );
    expect(entry.groups[0].blocks).toEqual([
      { items: [{ text: "A bullet lazily continued." }] },
      { paragraph: "A closing paragraph on two lines." },
    ]);
  });

  it("ignores everything inside code fences", () => {
    const [entry] = parse(
      ["## [1.0.0] - 2026-01-01", "### Topic", "- Kept", "```", "### Not a heading", "- not a bullet", "## not a release", "```", "- Also kept"].join(
        "\n",
      ),
    );
    expect(entry.groups).toEqual([{ title: "Topic", blocks: [{ items: [{ text: "Kept" }, { text: "Also kept" }] }] }]);
  });

  it("accepts an en dash or em dash between version and date", () => {
    const entries = parse("## [1.1.0] – 2026-02-01\n### A\n- a\n## [1.0.0] — 2026-01-01\n### B\n- b\n");
    expect(entries.map((e) => [e.version, e.date])).toEqual([
      ["1.1.0", "2026-02-01"],
      ["1.0.0", "2026-01-01"],
    ]);
  });

  it("fails on a release heading it can't read", () => {
    expect(() => parse("## [1.0.0]\n### A\n- a\n")).toThrow(/line 1.*## \[1\.0\.0\]/);
    expect(() => parse("# Changelog\n\n## Notes\n")).toThrow(/line 3/);
  });

  it("keeps a release with nothing for users, with no groups, and warns about it", () => {
    const onWarning = vi.fn();
    const entries = parse(
      "## [1.1.0] - 2026-02-01\n### Internal\n- CI only\n## [1.0.0] - 2026-01-01\n### A\n- a\n",
      onWarning,
    );
    expect(entries.map((e) => [e.version, e.groups.length])).toEqual([
      ["1.1.0", 0],
      ["1.0.0", 1],
    ]);
    expect(onWarning).toHaveBeenCalledOnce();
    expect(onWarning.mock.calls[0][0]).toMatch(/1\.1\.0/);
  });

  it("treats any Unreleased heading as unreleased, even with a date", () => {
    const entries = parse("## [Unreleased] - 2026-01-01\n### A\n- not yet\n## [1.0.0] - 2026-01-01\n### B\n- b\n");
    expect(entries.map((e) => e.version)).toEqual(["1.0.0"]);
  });

  it("attaches a wrapped line at the sub-bullet's indent to the sub-item", () => {
    const [entry] = parse(["## [1.0.0] - 2026-01-01", "### Topic", "- Parent", "  - Fixed: sub", "  wraps."].join("\n"));
    expect(itemsOf(entry.groups[0])).toEqual([{ text: "Parent", children: [{ kind: "fixed", text: "sub wraps." }] }]);
  });

  it("starts a list at an indented bullet after a paragraph", () => {
    const [entry] = parse(["## [1.0.0] - 2026-01-01", "### Topic", "Intro", "  - sub"].join("\n"));
    expect(entry.groups[0].blocks).toEqual([{ paragraph: "Intro" }, { items: [{ text: "sub" }] }]);
  });

  it("fails on a code fence that is never closed", () => {
    expect(() => parse("## [1.0.0] - 2026-01-01\n### A\n- a\n```\n## [0.9.0] - 2025-01-01\n")).toThrow(
      /line 4.*code fence/,
    );
  });

  it("does not glue text after a code fence onto the bullet before it", () => {
    const [entry] = parse(["## [1.0.0] - 2026-01-01", "### Topic", "- Bullet", "```", "code", "```", "After."].join("\n"));
    expect(entry.groups[0].blocks).toEqual([{ items: [{ text: "Bullet" }] }, { paragraph: "After." }]);
  });

  it("returns an empty array for a changelog with only Unreleased", () => {
    expect(parse("# Changelog\n\n## [Unreleased]\n\n### Added\n- x\n")).toEqual([]);
  });
});

describe("the real CHANGELOG.md", () => {
  const changelog = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "..", "..", "CHANGELOG.md"), "utf-8");
  const entries = parse(changelog);

  // Released versions that have at least one non-Internal `###` section.
  const userFacingVersions = changelog
    .split(/^## /m)
    .filter((section) => /^\[\d/.test(section))
    .filter((section) => /^###\s+(?!Internal\b)/im.test(section))
    .map((section) => section.match(/^\[([^\]]+)\]/)![1]);

  it("yields release notes for every released version with user-facing sections", () => {
    expect(userFacingVersions.length).toBeGreaterThan(0);
    for (const version of userFacingVersions) {
      const entry = entries.find((e) => e.version === version);
      expect(entry, `release ${version} has no release notes`).toBeDefined();
      expect(entry!.groups.flatMap(itemsOf).length, `release ${version} has no bullets`).toBeGreaterThan(0);
    }
  });

  it("reads the topic sections of 0.3.0", () => {
    const entry = entries.find((e) => e.version === "0.3.0");
    expect(entry).toBeDefined();
    expect(entry!.groups.length).toBeGreaterThanOrEqual(8);
    expect(entry!.groups.some((g) => itemsOf(g).length > 0)).toBe(true);
  });
});

describe("isMainModule", () => {
  it("recognizes an entry-point path when its file URL encodes spaces", () => {
    expect(isMainModule("file:///tmp/release%20notes.mjs", "/tmp/release notes.mjs")).toBe(true);
  });
});
