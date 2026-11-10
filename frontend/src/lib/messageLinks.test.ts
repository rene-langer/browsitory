import { describe, expect, it } from "vitest";
import type { ForgeRepository } from "../ipc/RepoClient";
import { issueForgeRepository, parseMessageLinks, type MessageSegment } from "./messageLinks";

const github: ForgeRepository = { provider: "GitHub", host: "github.com", owner: "acme", name: "app", remoteName: "origin" };
const bitbucket: ForgeRepository = {
  provider: "Bitbucket",
  host: "bitbucket.org",
  owner: "team",
  name: "service",
  remoteName: "upstream",
};

/** The links `parseMessageLinks` finds, as `[text, url]`. */
function links(message: string, forge: ForgeRepository | null = github): [string, string][] {
  return parseMessageLinks(message, forge)
    .filter((segment): segment is Extract<MessageSegment, { kind: "link" }> => segment.kind === "link")
    .map((segment) => [segment.text, segment.url]);
}

describe("parseMessageLinks", () => {
  it("keeps the whole message, whitespace included, across its segments", () => {
    const message = "Fix #1\n\n  See https://example.com/a (and acme/lib#2).\n";
    const segments = parseMessageLinks(message, github);
    expect(segments.map((segment) => segment.text).join("")).toBe(message);
    expect(segments).toEqual([
      { kind: "text", text: "Fix " },
      { kind: "link", text: "#1", url: "https://github.com/acme/app/issues/1" },
      { kind: "text", text: "\n\n  See " },
      { kind: "link", text: "https://example.com/a", url: "https://example.com/a" },
      { kind: "text", text: " (and " },
      { kind: "link", text: "acme/lib#2", url: "https://github.com/acme/lib/issues/2" },
      { kind: "text", text: ").\n" },
    ]);
  });

  it("is one text segment when there is nothing to link", () => {
    expect(parseMessageLinks("Plain message", github)).toEqual([{ kind: "text", text: "Plain message" }]);
    expect(parseMessageLinks("", github)).toEqual([]);
  });

  it("leaves trailing punctuation and closing brackets out of a URL", () => {
    expect(links("See https://example.com/a.")).toEqual([["https://example.com/a", "https://example.com/a"]]);
    expect(links("https://example.com/a, then")).toEqual([["https://example.com/a", "https://example.com/a"]]);
    expect(links("(https://example.com/a)")).toEqual([["https://example.com/a", "https://example.com/a"]]);
    expect(links("(see https://example.com/a).")).toEqual([["https://example.com/a", "https://example.com/a"]]);
    expect(links("[https://example.com/a]")).toEqual([["https://example.com/a", "https://example.com/a"]]);
    expect(links('"https://example.com/a?b=1&c=2"!')).toEqual([
      ["https://example.com/a?b=1&c=2", "https://example.com/a?b=1&c=2"],
    ]);
  });

  it("keeps brackets a URL opens itself, and its fragment", () => {
    expect(links("https://example.com/wiki/Foo_(bar)")).toEqual([
      ["https://example.com/wiki/Foo_(bar)", "https://example.com/wiki/Foo_(bar)"],
    ]);
    expect(links("http://example.com/page#section")).toEqual([
      ["http://example.com/page#section", "http://example.com/page#section"],
    ]);
  });

  it("links http and https URLs only, with or without a forge", () => {
    expect(links("https://example.com and HTTP://example.org", null)).toEqual([
      ["https://example.com", "https://example.com"],
      ["HTTP://example.org", "HTTP://example.org"],
    ]);
    expect(links("javascript:alert(1) ftp://example.com file:///etc/passwd mailto:a@example.com")).toEqual([]);
    expect(links("javascript://example.com/%0Aalert(1)")).toEqual([]);
    expect(links("https:// nothing after the scheme")).toEqual([]);
  });

  it("links #N at the start of a line and after punctuation, not inside a word", () => {
    expect(links("#12 first\n#3 second")).toEqual([
      ["#12", "https://github.com/acme/app/issues/12"],
      ["#3", "https://github.com/acme/app/issues/3"],
    ]);
    expect(links("(#4), [#5]")).toEqual([
      ["#4", "https://github.com/acme/app/issues/4"],
      ["#5", "https://github.com/acme/app/issues/5"],
    ]);
    expect(links("abc#1 a_#2 1#3 ##4 #5a #0 #")).toEqual([]);
  });

  it("links owner/repo#N to that repository on the forge's host", () => {
    expect(links("Refs other-org/other.repo#7.")).toEqual([
      ["other-org/other.repo#7", "https://github.com/other-org/other.repo/issues/7"],
    ]);
    expect(links("a/b/c#1 x.y/z#2")).toEqual([]);
  });

  it("builds Bitbucket issue URLs", () => {
    expect(links("Fixes #9 and team/other#3", bitbucket)).toEqual([
      ["#9", "https://bitbucket.org/team/service/issues/9"],
      ["team/other#3", "https://bitbucket.org/team/other/issues/3"],
    ]);
  });

  it("leaves issue references as text without a forge remote", () => {
    expect(parseMessageLinks("Fixes #9 and acme/app#3", null)).toEqual([{ kind: "text", text: "Fixes #9 and acme/app#3" }]);
  });
});

describe("issueForgeRepository", () => {
  const fork: ForgeRepository = { ...github, owner: "me", remoteName: "fork" };

  it("prefers the tracked remote, then origin, then the first forge remote", () => {
    expect(issueForgeRepository([fork, github, bitbucket], "upstream")).toBe(bitbucket);
    expect(issueForgeRepository([fork, github, bitbucket], null)).toBe(github);
    expect(issueForgeRepository([fork, github, bitbucket], "not-a-forge")).toBe(github);
    expect(issueForgeRepository([fork, bitbucket], null)).toBe(fork);
    expect(issueForgeRepository([], "origin")).toBeNull();
  });
});
