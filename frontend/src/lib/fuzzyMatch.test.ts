import { describe, expect, it } from "vitest";
import { matchItem, mergeRanges } from "./fuzzyMatch";

const item = (label: string, keywords: string[] = []) => ({ label, keywords });

describe("matchItem (#644)", () => {
  it("matches everything with no ranges for an empty query", () => {
    expect(matchItem(item("Fetch origin"), "  ")).toEqual({ score: 0, ranges: [] });
  });

  it("matches the words of the query in any order", () => {
    expect(matchItem(item("Merge dark-mode into main"), "merge dark")).not.toBeNull();
    expect(matchItem(item("Merge dark-mode into main"), "dark merge")).not.toBeNull();
    expect(matchItem(item("Switch to main"), "sw main")).not.toBeNull();
  });

  it("needs every word to match", () => {
    expect(matchItem(item("Switch to main"), "sw zebra")).toBeNull();
  });

  it("matches letters in order across a word (fuzzy)", () => {
    expect(matchItem(item("Checkout branch"), "chkout")).not.toBeNull();
    expect(matchItem(item("Fetch origin"), "ftch")).not.toBeNull();
  });

  it("doesn't match scattered letters that don't start at a word", () => {
    expect(matchItem(item("Switch to main"), "checkout")).toBeNull();
    expect(matchItem(item("Fetch origin"), "hg")).toBeNull();
  });

  it("matches keywords, without a highlight in the label", () => {
    expect(matchItem(item("Fetch origin", ["remote"]), "remote")).toEqual({
      score: expect.any(Number) as number,
      ranges: [],
    });
  });

  it("returns the label ranges to highlight", () => {
    expect(matchItem(item("Switch to main"), "sw main")?.ranges).toEqual([
      [0, 2],
      [10, 14],
    ]);
    expect(matchItem(item("Fetch origin"), "ftch")?.ranges).toEqual([
      [0, 1],
      [2, 5],
    ]);
  });

  it("ranks the whole label over a prefix over word starts over fuzzy over keywords", () => {
    const score = (label: string, query: string, keywords: string[] = []) =>
      matchItem(item(label, keywords), query)?.score ?? -1;
    expect(score("Go to branch…", "go to branch")).toBeGreaterThan(score("Go to Branches", "go to branch"));
    expect(score("Fetch origin", "fetch")).toBeGreaterThan(score("Force fetch", "fetch"));
    expect(score("Force fetch", "fetch")).toBeGreaterThan(score("Prefetch", "fetch"));
    expect(score("Prefetch", "fetch")).toBeGreaterThan(score("Fast tech", "ftch"));
    expect(score("Fast tech", "ftch")).toBeGreaterThan(score("Pull", "ftch", ["ftch"]));
  });

  it("prefers a tight fuzzy match over a spread-out one", () => {
    const tight = matchItem(item("Stash all"), "sta")!.score;
    const loose = matchItem(item("Switch to a branch"), "sta")!.score;
    expect(tight).toBeGreaterThan(loose);
  });
});

describe("mergeRanges", () => {
  it("sorts and merges overlapping or touching ranges", () => {
    expect(
      mergeRanges([
        [5, 7],
        [0, 2],
        [2, 3],
        [6, 9],
      ]),
    ).toEqual([
      [0, 3],
      [5, 9],
    ]);
  });
});
