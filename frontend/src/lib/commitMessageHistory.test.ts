import { afterEach, describe, expect, it, vi } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import {
  RECORDED_MESSAGE_LIMIT,
  messageHistoryEntries,
  readCommitMessages,
  recentBranchCommits,
  recordCommitMessage,
} from "./commitMessageHistory";

const commit = (id: string, summary: string, parentIds: string[] = []): GraphCommit => ({
  id,
  shortId: id.slice(0, 7),
  summary,
  authorName: "A",
  authorEmail: "a@example.com",
  timestamp: 0,
  parentIds,
  branchRefs: [],
  remoteBranchRefs: [],
});

describe("recorded commit messages", () => {
  afterEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it("keeps messages per repo, newest first, without duplicates", () => {
    recordCommitMessage("/a", "First");
    recordCommitMessage("/a", "Second\n\nBody");
    recordCommitMessage("/b", "Other repo");
    recordCommitMessage("/a", "First\n");
    expect(readCommitMessages("/a")).toEqual(["First", "Second\n\nBody"]);
    expect(readCommitMessages("/b")).toEqual(["Other repo"]);
  });

  it("ignores blank messages and keeps at most the limit", () => {
    recordCommitMessage("/a", "   ");
    expect(readCommitMessages("/a")).toEqual([]);
    for (let i = 0; i < RECORDED_MESSAGE_LIMIT + 5; i += 1) recordCommitMessage("/a", `m${i}`);
    const stored = readCommitMessages("/a");
    expect(stored).toHaveLength(RECORDED_MESSAGE_LIMIT);
    expect(stored[0]).toBe(`m${RECORDED_MESSAGE_LIMIT + 4}`);
  });

  it("survives broken or blocked storage", () => {
    localStorage.setItem("commit-messages:/a", "{not json");
    expect(readCommitMessages("/a")).toEqual([]);
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(readCommitMessages("/a")).toEqual([]);
    expect(() => recordCommitMessage("/a", "x")).not.toThrow();
  });
});

describe("recentBranchCommits", () => {
  it("follows first parents from HEAD while they are loaded", () => {
    const commits = [
      commit("m", "Merge", ["c", "side"]),
      commit("side", "Side work", ["b"]),
      commit("c", "Third", ["b"]),
      commit("b", "Second", ["a"]),
    ];
    expect(recentBranchCommits(commits, "m").map((c) => c.id)).toEqual(["m", "c", "b"]);
    expect(recentBranchCommits(commits, "m", 2).map((c) => c.id)).toEqual(["m", "c"]);
    expect(recentBranchCommits(commits, null)).toEqual([]);
    expect(recentBranchCommits(commits, "missing")).toEqual([]);
  });
});

describe("messageHistoryEntries", () => {
  it("lists recorded messages first and each summary once", async () => {
    const load = vi.fn((id: string) => Promise.resolve(`full ${id}`));
    const entries = messageHistoryEntries(
      ["Fix it\n\nBody", "Fix it\n\nOther body", "Add tests"],
      [commit("c", "Add tests"), commit("b", "Older")],
      load,
    );
    expect(entries.map((e) => [e.summary, e.source])).toEqual([
      ["Fix it", "recorded"],
      ["Add tests", "recorded"],
      ["Older", "branch"],
    ]);
    expect(entries[2].shortId).toBe("b");
    await expect(entries[0].load()).resolves.toBe("Fix it\n\nBody");
    await expect(entries[2].load()).resolves.toBe("full b");
    expect(load).toHaveBeenCalledWith("b");
  });
});
