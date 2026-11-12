import { describe, expect, it } from "vitest";
import type { ConflictSegment } from "../ipc/RepoClient";
import { conflictTokens, conflictVersions, segmentLines } from "./conflictHighlight";

describe("segmentLines", () => {
  it("splits a segment into lines without their line ends", () => {
    expect(segmentLines("a\nb\n")).toEqual(["a", "b"]);
    expect(segmentLines("a\r\nb")).toEqual(["a", "b"]);
    expect(segmentLines("")).toEqual([]);
    expect(segmentLines("\n")).toEqual([""]);
  });
});

describe("conflictVersions", () => {
  it("builds each side's version of the file and where each segment starts in it", () => {
    const segments: ConflictSegment[] = [
      { kind: "Clean", content: "one\n" },
      { kind: "Conflict", ours: "o1\no2\n", base: "", theirs: "t1\n" },
      { kind: "Clean", content: "three" },
    ];
    const versions = conflictVersions(segments);
    expect(versions.ours).toEqual({ text: "one\no1\no2\nthree", starts: [0, 1, 3] });
    expect(versions.theirs).toEqual({ text: "one\nt1\nthree", starts: [0, 1, 2] });
    expect(versions.base).toEqual({ text: "one\nthree", starts: [0, 1, 1] });
  });

  it("keeps a side without a final line end on its own line", () => {
    const segments: ConflictSegment[] = [
      { kind: "Conflict", ours: "a", base: "", theirs: "b\n" },
      { kind: "Clean", content: "c\n" },
    ];
    expect(conflictVersions(segments).ours).toEqual({ text: "a\nc", starts: [0, 1] });
  });

  // #847: a segment's lines were spread into push(), one argument per line, which throws past
  // the engine's argument limit (V8: about 120,000).
  it("builds a version from a segment of 200,000 lines", () => {
    const count = 200_000;
    const content = "x\n".repeat(count);
    const segments: ConflictSegment[] = [
      { kind: "Clean", content },
      { kind: "Conflict", ours: "o\n", base: "", theirs: content },
    ];
    const versions = conflictVersions(segments);
    expect(versions.ours.starts).toEqual([0, count]);
    expect(versions.ours.text.split("\n")).toHaveLength(count + 1);
    expect(versions.theirs.text.split("\n")).toHaveLength(2 * count);
    expect(versions.base.starts).toEqual([0, count]);
  });
});

describe("conflictTokens", () => {
  // #763: each side is tokenized as a whole file, so a comment opened in the unchanged lines
  // above a conflict stays a comment inside it.
  it("highlights the sides and the unchanged runs as continuous files", async () => {
    const segments: ConflictSegment[] = [
      { kind: "Clean", content: "/* opened above\n" },
      { kind: "Conflict", ours: "still a comment */ const a = 1;\n", base: "base */\n", theirs: "theirs too */\n" },
      { kind: "Clean", content: "let b = 2;\n" },
    ];
    const tokens = await conflictTokens(segments, "typescript");
    const text = (lines: { content: string }[][] | undefined) => lines?.map((l) => l.map((t) => t.content).join(""));
    expect(text(tokens.ours[0])).toEqual(["/* opened above"]);
    expect(text(tokens.ours[1])).toEqual(["still a comment */ const a = 1;"]);
    expect(text(tokens.theirs[1])).toEqual(["theirs too */"]);
    expect(text(tokens.base[1])).toEqual(["base */"]);
    expect(text(tokens.ours[2])).toEqual(["let b = 2;"]);
    expect(tokens.theirs[0]).toBeUndefined();
    expect(tokens.ours[1]![0][0].color).toBe("var(--shiki-token-comment)");
    expect(tokens.theirs[1]![0][0].color).toBe("var(--shiki-token-comment)");
    expect(tokens.ours[1]![0].some((t) => t.color === "var(--shiki-token-keyword)")).toBe(true);
  }, 20000);
});
