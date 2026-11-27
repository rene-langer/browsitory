import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readCommitDraft, writeCommitDraft } from "./commitDraftStorage";

// #1143: the commit message being written outlives the app, per repository, in web storage.
describe("commit draft storage", () => {
  beforeEach(() => localStorage.clear());
  afterEach(() => vi.restoreAllMocks());

  it("reads back the draft written for a repository, and only for that one", () => {
    const draft = {
      message: "feat: half-typed\n\nBody",
      lastSeeded: "",
      stashed: null,
      coAuthors: [{ name: "Ada", email: "ada@example.com" }],
      stashedCoAuthors: null,
      amendAuthor: null,
    };
    writeCommitDraft("/repos/a", draft);

    expect(readCommitDraft("/repos/a")).toEqual(draft);
    expect(readCommitDraft("/repos/b")).toBeUndefined();
  });

  it("keeps nothing for an empty draft, and drops the one kept before", () => {
    writeCommitDraft("/repos/a", { message: "wip", lastSeeded: "" });
    writeCommitDraft("/repos/a", { message: "", lastSeeded: "", stashed: null, coAuthors: [], stashedCoAuthors: null, amendAuthor: null });

    expect(readCommitDraft("/repos/a")).toBeUndefined();
    expect(localStorage.length).toBe(0);
  });

  it("keeps a draft whose message is empty while amend mode holds the one being written", () => {
    const draft = { message: "", lastSeeded: "", stashed: "feat: before amending", coAuthors: [], stashedCoAuthors: null, amendAuthor: null };
    writeCommitDraft("/repos/a", draft);
    expect(readCommitDraft("/repos/a")).toEqual(draft);
  });

  it("reads nothing from a damaged entry", () => {
    localStorage.setItem("commit-draft:/repos/a", "{not json");
    expect(readCommitDraft("/repos/a")).toBeUndefined();
    localStorage.setItem("commit-draft:/repos/a", JSON.stringify({ message: 42 }));
    expect(readCommitDraft("/repos/a")).toBeUndefined();
  });

  it("keeps working when web storage is blocked", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(() => writeCommitDraft("/repos/a", { message: "wip", lastSeeded: "" })).not.toThrow();
    expect(readCommitDraft("/repos/a")).toBeUndefined();
  });
});
