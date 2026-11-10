import { describe, expect, it } from "vitest";
import guide from "../../../docs/guide/search.md?raw";
import { SEARCH_KEYS } from "../ipc/RepoClient";
import { SEARCH_SYNTAX, SEARCH_SYNTAX_FOOTER } from "./searchSyntax";

// The user guide repeats the in-app search help (#320); this keeps the two from drifting apart.
const plain = (text: string) => text.replace(/`/g, "");

function guideTableRows(): { syntax: string; meaning: string }[] {
  return guide
    .split("\n")
    .filter((line) => line.startsWith("| ") && !line.startsWith("| Syntax |"))
    .map((line) => {
      const [syntax, meaning] = line.split("|").slice(1, 3).map((cell) => plain(cell.trim()));
      return { syntax, meaning };
    });
}

describe("docs/guide/search.md", () => {
  it("has exactly the search syntax table the app shows", () => {
    expect(guideTableRows()).toEqual(SEARCH_SYNTAX);
  });

  it("states the footer the app shows", () => {
    expect(plain(guide)).toContain(SEARCH_SYNTAX_FOOTER);
  });
});

// #497: the keys come from the Rust parser (`git_core::search::KEYS`, generated into
// `ipc/generated/wire-constants.ts`), so the help can't offer a key the backend rejects or leave
// one out.
describe("SEARCH_SYNTAX", () => {
  it("shows exactly the keys the backend's search parser accepts", () => {
    const shown = SEARCH_SYNTAX.flatMap(({ syntax }) => /^([a-z]+):/.exec(syntax)?.[1] ?? []);
    expect([...new Set(shown)].sort()).toEqual([...SEARCH_KEYS].sort());
  });
});
