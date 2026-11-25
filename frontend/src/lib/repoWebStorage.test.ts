import { beforeEach, describe, expect, it } from "vitest";
import { forgetRepoWebStorage, forgetRepoWebStorageExcept } from "./repoWebStorage";
import { NOTHING, SOMETHING, rememberRepo, remembered } from "../test/rememberRepo";

describe("repository web storage (#1096)", () => {
  beforeEach(() => localStorage.clear());

  it("forgets everything one repository keeps, and only that", () => {
    rememberRepo("/repo/a");
    rememberRepo("/repo/a/b");
    rememberRepo("/repo/ab");
    localStorage.setItem("sidebar-width", "300");

    forgetRepoWebStorage("/repo/a");

    expect(remembered("/repo/a")).toEqual(NOTHING);
    expect(remembered("/repo/a/b")).toEqual(SOMETHING);
    expect(remembered("/repo/ab")).toEqual(SOMETHING);
    expect(localStorage.getItem("sidebar-width")).toBe("300");
  });

  it("forgets the repositories not kept, and leaves the rest of web storage alone", () => {
    rememberRepo("/repo/kept");
    rememberRepo("/repo/gone");
    rememberRepo("C:\\work\\gone");
    rememberRepo("C:\\work\\kept");
    localStorage.setItem("sidebar-width", "300");
    localStorage.setItem("forge-account.GitHub", "rene");
    localStorage.setItem("branchtree.folder.feature", "closed");

    forgetRepoWebStorageExcept(["/repo/kept", "C:\\work\\kept"]);

    expect(remembered("/repo/kept")).toEqual(SOMETHING);
    expect(remembered("C:\\work\\kept")).toEqual(SOMETHING);
    expect(remembered("/repo/gone")).toEqual(NOTHING);
    expect(remembered("C:\\work\\gone")).toEqual(NOTHING);
    expect(localStorage.length).toBe(3 + 2 * 5);
    expect(localStorage.getItem("sidebar-width")).toBe("300");
    expect(localStorage.getItem("forge-account.GitHub")).toBe("rene");
    expect(localStorage.getItem("branchtree.folder.feature")).toBe("closed");
  });

  it("forgets every repository when none is kept", () => {
    rememberRepo("/repo/a");
    rememberRepo("/repo/b");

    forgetRepoWebStorageExcept([]);

    expect(remembered("/repo/a")).toEqual(NOTHING);
    expect(remembered("/repo/b")).toEqual(NOTHING);
    expect(localStorage.length).toBe(0);
  });
});
