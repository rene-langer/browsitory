import { describe, expect, it } from "vitest";
import { assembleResult, blockResult, hasConflictMarkers } from "./conflictMerge";

const block = { kind: "Conflict" as const, ours: "o\n", base: "b\n", theirs: "t\n" };

describe("conflictMerge", () => {
  it("gives each choice's text", () => {
    expect(blockResult(block, { kind: "unchosen" })).toBeNull();
    expect(blockResult(block, { kind: "chosen", choice: "ours" })).toBe("o\n");
    expect(blockResult(block, { kind: "chosen", choice: "theirs" })).toBe("t\n");
    expect(blockResult(block, { kind: "chosen", choice: "oursThenTheirs" })).toBe("o\nt\n");
    expect(blockResult(block, { kind: "chosen", choice: "theirsThenOurs" })).toBe("t\no\n");
    expect(blockResult(block, { kind: "chosen", choice: "base" })).toBe("b\n");
    expect(blockResult(block, { kind: "edited", text: "x" })).toBe("x");
  });

  it("writes markers around an unchosen block, ending each side with a newline", () => {
    const noNewline = { kind: "Conflict" as const, ours: "o", base: "", theirs: "t" };
    expect(assembleResult([{ kind: "Clean", content: "a\n" }, noNewline], [{ kind: "unchosen" }, { kind: "unchosen" }])).toBe(
      "a\n<<<<<<< ours\no\n||||||| base\n=======\nt\n>>>>>>> theirs\n",
    );
  });

  it("finds start and end markers, but not a heading underline or longer runs", () => {
    expect(hasConflictMarkers("a\n<<<<<<< ours\n")).toBe(true);
    expect(hasConflictMarkers(">>>>>>>\nb")).toBe(true);
    expect(hasConflictMarkers(">>>>>>>\r\nb")).toBe(true);
    expect(hasConflictMarkers("Title\n=======\n")).toBe(false);
    expect(hasConflictMarkers("<<<<<<<< eight\n")).toBe(false);
    expect(hasConflictMarkers("x <<<<<<< ours\n")).toBe(false);
    expect(hasConflictMarkers("<<<<<<<x\n")).toBe(false);
  });
});
