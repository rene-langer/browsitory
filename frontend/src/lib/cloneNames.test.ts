import { describe, expect, it } from "vitest";
import { deriveRepoName, joinPath, looksLikeToken, splitHttpsUserName } from "./cloneNames";

describe("deriveRepoName", () => {
  it.each([
    ["https://github.com/org/repo.git", "repo"],
    ["https://github.com/org/repo/", "repo"],
    ["git@github.com:org/repo.git", "repo"],
    ["git@host:repo.git", "repo"],
    ["ssh://git@host:22/org/repo", "repo"],
    ["/home/me/src/thing.git", "thing"],
    ["C:\\src\\thing", "thing"],
    ["  https://h/x.git  ", "x"],
    ["", ""],
  ])("%s -> %s", (url, name) => {
    expect(deriveRepoName(url)).toBe(name);
  });
});

describe("joinPath", () => {
  it("joins with the parent's separator and drops trailing separators", () => {
    expect(joinPath("/home/me/src/", "repo")).toBe("/home/me/src/repo");
    expect(joinPath("C:\\src\\", "repo")).toBe("C:\\src\\repo");
    expect(joinPath("/", "repo")).toBe("/repo");
  });
});

describe("splitHttpsUserName", () => {
  it.each([
    ["https://alice@bitbucket.org/ws/r.git", "https://bitbucket.org/ws/r.git", "alice"],
    ["https://ghp_abc@github.com/o/r.git", "https://github.com/o/r.git", "ghp_abc"],
    ["  HTTPS://al%40ice@host:8443/r?x=1  ", "HTTPS://host:8443/r?x=1", "al@ice"],
    ["https://alice@host", "https://host", "alice"],
  ])("%s -> %s and %s", (url, rest, username) => {
    expect(splitHttpsUserName(url)).toEqual({ url: rest, username });
  });

  it.each([
    "https://alice:secret@host/r.git",
    "https://alice:@host/r.git",
    "https://host/a@b",
    "https://host/r.git",
    "https://@host/r.git",
    "https://alice@",
    "http://alice@host/r.git",
    "ssh://alice@host/r.git",
    "alice@host:r.git",
    "",
  ])("leaves %s alone", (url) => {
    expect(splitHttpsUserName(url)).toBeNull();
  });
});

// A token pasted as the URL's user name must never be shown in the Username field (#1092).
// The fake tokens are split where a secret scanner would take them for real ones.
describe("looksLikeToken", () => {
  it.each([
    ["GitHub classic token", "ghp_0123456789abcdefABCDEF0123456789abcd"],
    ["GitHub OAuth token", "gho_0123456789abcdefABCDEF0123456789abcd"],
    ["GitHub user-to-server token", "ghu_0123456789abcdefABCDEF0123456789abcd"],
    ["GitHub server-to-server token", "ghs_0123456789abcdefABCDEF0123456789abcd"],
    ["GitHub refresh token", "ghr_0123456789abcdefABCDEF0123456789abcd"],
    ["GitHub fine-grained token", "github_pat_11ABCDEFG0123456789_abcdefghijklmnopqrstuvwxyz0123456789"],
    ["short GitHub prefix", "ghp_abc"],
    ["GitLab personal token", "glpat-" + "0123456789abcdefABCD"],
    ["GitLab deploy token", "gldt-" + "0123456789abcdefABCD"],
    ["GitLab OAuth token", "gloas-" + "0123456789abcdefABCD"],
    ["Bitbucket app password", "ATBB0123456789abcdefABCDEF0123456789"],
    ["Atlassian API token", "ATATT3xFfGF0123456789abcdefABCDEF"],
    ["Bitbucket access token", "ATCTT3xFfGF0123456789abcdefABCDEF"],
    ["40 hex characters (Gitea, old GitHub tokens)", "0123456789abcdef0123456789abcdef01234567"],
    ["52-character Azure DevOps token", "abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrst"],
    ["32 random characters", "Zq3v9XkT2mWb7LpR4sYd8NcF6hJg1aEu"],
  ])("%s", (_kind, name) => {
    expect(looksLikeToken(name)).toBe(true);
  });

  it.each([
    "alice",
    "Alice-Smith",
    "alice.smith@example.com",
    "x-token-auth",
    "x-access-token",
    "oauth2",
    "git",
    "ghost",
    "glenn-miller",
    "github-actions",
    "user1985",
    "a-rather-long-but-ordinary-user-name",
    "firstname.lastname.1234567890.company",
    "Zq3v9XkT2mWb7LpR4sYd8NcF6hJg1aE",
    "",
  ])("%s is an ordinary user name", (name) => {
    expect(looksLikeToken(name)).toBe(false);
  });
});
