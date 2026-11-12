import { describe, expect, it } from "vitest";
import { assembleResult, blockResult, hasConflictMarkers, sideTaken, takeSide, type BlockState } from "./conflictMerge";

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

  // #876: the take-side arrows. The second side taken goes after the first.
  it("takes a side, and the other one after it", () => {
    const unchosen: BlockState = { kind: "unchosen" };
    expect(takeSide(unchosen, "ours")).toEqual({ kind: "chosen", choice: "ours" });
    expect(takeSide(unchosen, "theirs")).toEqual({ kind: "chosen", choice: "theirs" });
    expect(takeSide({ kind: "chosen", choice: "ours" }, "theirs")).toEqual({ kind: "chosen", choice: "oursThenTheirs" });
    expect(takeSide({ kind: "chosen", choice: "theirs" }, "ours")).toEqual({ kind: "chosen", choice: "theirsThenOurs" });
    // A side already in the result stays as it is.
    expect(takeSide({ kind: "chosen", choice: "ours" }, "ours")).toEqual({ kind: "chosen", choice: "ours" });
    expect(takeSide({ kind: "chosen", choice: "oursThenTheirs" }, "theirs")).toEqual({
      kind: "chosen",
      choice: "oursThenTheirs",
    });
    // Over the base or a typed result, a side replaces it, as the Accept buttons do.
    expect(takeSide({ kind: "chosen", choice: "base" }, "theirs")).toEqual({ kind: "chosen", choice: "theirs" });
    expect(takeSide({ kind: "edited", text: "x\n" }, "ours")).toEqual({ kind: "chosen", choice: "ours" });
  });

  it("says whether a side is in the result", () => {
    expect(sideTaken({ kind: "unchosen" }, "ours")).toBe(false);
    expect(sideTaken({ kind: "chosen", choice: "ours" }, "ours")).toBe(true);
    expect(sideTaken({ kind: "chosen", choice: "ours" }, "theirs")).toBe(false);
    expect(sideTaken({ kind: "chosen", choice: "oursThenTheirs" }, "theirs")).toBe(true);
    expect(sideTaken({ kind: "chosen", choice: "theirsThenOurs" }, "ours")).toBe(true);
    expect(sideTaken({ kind: "chosen", choice: "base" }, "ours")).toBe(false);
    expect(sideTaken({ kind: "edited", text: "o\n" }, "ours")).toBe(false);
  });
});
