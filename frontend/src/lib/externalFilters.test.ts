import { describe, expect, it } from "vitest";
import type { StatusEntry } from "../ipc/RepoClient";
import { leftOut, leftOutNotice, stashedMessage } from "./externalFilters";

const entry = (path: string, staged = false, extra: Partial<StatusEntry> = {}): StatusEntry => ({
  path,
  oldPath: null,
  staged,
  kind: "Modified",
  ...extra,
});
const secret = entry("secret.env", false, { externalFilter: "crypt" });
const big = entry("big.bin", false, { externalFilter: "lfs" });
const notes = entry("notes.txt", false, { workingTreeEncoding: "UTF-16" });

// #1107: the bulk actions leave out files Browsitory can't filter or convert, and say so.
describe("files left out of bulk actions (#1107)", () => {
  it("picks the unstaged entries Browsitory can't filter or convert", () => {
    expect(leftOut([entry("a.txt"), secret, entry("secret.env", true), notes])).toEqual([secret, notes]);
  });

  it("names one file and what Browsitory can't do with it", () => {
    expect(leftOutNotice([secret])).toBe(
      "secret.env is left out: Browsitory can't run its 'crypt' filter. Use git on the command line for it.",
    );
    expect(leftOutNotice([notes])).toBe(
      "notes.txt is left out: Browsitory can't convert its 'UTF-16' encoding. Use git on the command line for it.",
    );
  });

  it("names every file of several", () => {
    expect(leftOutNotice([secret, big])).toBe(
      "2 files are left out because Browsitory can't run their filters: secret.env ('crypt' filter), " +
        "big.bin ('lfs' filter). Use git on the command line for them.",
    );
    expect(leftOutNotice([secret, notes])).toBe(
      "2 files are left out because Browsitory can't run their filters or convert their encodings: " +
        "secret.env ('crypt' filter), notes.txt (UTF-16 encoding). Use git on the command line for them.",
    );
    expect(leftOutNotice([notes, entry("other.txt", false, { workingTreeEncoding: "UTF-16LE" })])).toBe(
      "2 files are left out because Browsitory can't convert their encodings: notes.txt (UTF-16 encoding), " +
        "other.txt (UTF-16LE encoding). Use git on the command line for them.",
    );
  });

  it("says what a stash of everything left out", () => {
    expect(stashedMessage([entry("a.txt"), secret])).toBe(
      "Changes stashed. secret.env is left out: Browsitory can't run its 'crypt' filter. " +
        "Use git on the command line for it.",
    );
    expect(stashedMessage([entry("a.txt")])).toBe("Changes stashed");
  });
});
