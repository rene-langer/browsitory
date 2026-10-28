import { describe, expect, it } from "vitest";
import { coAuthorTrailers, formatPerson, parsePerson, sameEmail } from "./people";

describe("people", () => {
  it("formats and parses Name <email>", () => {
    expect(formatPerson({ name: "Ada Lovelace", email: "ada@x.org" })).toBe("Ada Lovelace <ada@x.org>");
    expect(parsePerson("  Ada Lovelace  <ada@x.org> ")).toEqual({ name: "Ada Lovelace", email: "ada@x.org" });
    expect(parsePerson("Ada <ada@x.org>")).toEqual({ name: "Ada", email: "ada@x.org" });
  });

  it("refuses anything else", () => {
    for (const text of ["Ada", "<ada@x.org>", "Ada <>", "Ada <a b@x.org>", "A<da <a@x.org>", "Ada <a@x.org> extra", "Ada\nEvil <a@x.org>", ""]) {
      expect(parsePerson(text)).toBeNull();
    }
  });

  it("turns people into Co-authored-by trailers and matches emails case-insensitively", () => {
    expect(coAuthorTrailers([{ name: "Ada", email: "ada@x.org" }])).toEqual([
      { key: "Co-authored-by", value: "Ada <ada@x.org>" },
    ]);
    expect(sameEmail({ name: "A", email: "ADA@x.org" }, { name: "B", email: "ada@X.org" })).toBe(true);
  });
});
