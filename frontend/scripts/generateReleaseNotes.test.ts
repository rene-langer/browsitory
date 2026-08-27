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

describe("parseChangelog", () => {
  it("skips the Unreleased section", () => {
    const entries = parseChangelog(SAMPLE);
    expect(entries.map((e) => e.version)).toEqual(["0.6.0", "0.5.0"]);
  });

  it("parses sections and bullets for a version", () => {
    const entries = parseChangelog(SAMPLE);
    expect(entries[0]).toEqual({
      version: "0.6.0",
      date: "2026-09-01",
      sections: {
        added: ["Feature A", "Feature B"],
        fixed: ["Bug fix A"],
      },
    });
  });

  it("omits sections with no bullets", () => {
    const entries = parseChangelog(SAMPLE);
    expect(entries[1].sections).toEqual({ changed: ["Behavior change"] });
  });

  it("returns an empty array for a changelog with only Unreleased", () => {
    const entries = parseChangelog("# Changelog\n\n## [Unreleased]\n\n### Added\n- x\n");
    expect(entries).toEqual([]);
  });
});

describe("isMainModule", () => {
  it("recognizes an entry-point path when its file URL encodes spaces", () => {
    expect(isMainModule("file:///tmp/release%20notes.mjs", "/tmp/release notes.mjs")).toBe(true);
  });
});
