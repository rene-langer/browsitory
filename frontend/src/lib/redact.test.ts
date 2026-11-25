import { describe, expect, it } from "vitest";
import { redactCredentials } from "./redact";

// The same cases as `repo_service::oplog`'s `redact_removes_user_information_and_authorization_values`.
describe("redactCredentials (#1056)", () => {
  it("removes the user information of every URL", () => {
    expect(redactCredentials("https://user:pass@host/x and http://tok@h2:8080/y")).toBe(
      "https://***@host/x and http://***@h2:8080/y",
    );
  });

  it("removes everything up to the host when the password holds an @", () => {
    expect(redactCredentials("'https://u:p@ss@host/x'")).toBe("'https://***@host/x'");
  });

  it("keeps a URL without user information, a scp-like address and an @ past the host", () => {
    expect(redactCredentials("https://host/a@b git@github.com:o/r.git")).toBe("https://host/a@b git@github.com:o/r.git");
  });

  it("removes the value of every Authorization header line", () => {
    expect(redactCredentials("authorization: Bearer abc\nAUTHORIZATION:x\r\nnext")).toBe(
      "authorization: ***\nAUTHORIZATION: ***\r\nnext",
    );
  });

  it("ends an authority at a quote, a bracket, a comma or whitespace", () => {
    expect(redactCredentials(`{"message":"https://tok@host"} (https://a@b) [https://c@d],x\thttps://e@f`)).toBe(
      `{"message":"https://***@host"} (https://***@b) [https://***@d],x\thttps://***@f`,
    );
  });

  // #1125: the same cases as `redact_removes_a_password_holding_characters_that_end_a_url_elsewhere`.
  it.each([
    ["https://alice:tok@host.example.test/r", "https://***@host.example.test/r"],
    [
      "unable to access 'https://alice:pa,ss@host.example.test/r/'",
      "unable to access 'https://***@host.example.test/r/'",
    ],
    ["https://alice:p(a)ss@host.example.test/r", "https://***@host.example.test/r"],
    ["https://alice:pa'ss@host.example.test/r", "https://***@host.example.test/r"],
    ["https://alice:pa[ss@host.example.test/r", "https://***@host.example.test/r"],
    ["https://alice:pa]ss@host.example.test/r", "https://***@host.example.test/r"],
    ["https://alice:pa/ss@host.example.test/r", "https://***@host.example.test/r"],
    ["https://alice:pa?ss@host.example.test/r", "https://***@host.example.test/r"],
  ])("removes a password holding characters that end a URL elsewhere: %s", (text, redacted) => {
    expect(redactCredentials(text)).toBe(redacted);
  });

  // #1125: the same cases as `redact_keeps_an_at_sign_after_the_host`.
  it.each([
    "(https://host.example.test/r) and git@host.example.test:o/r.git",
    "https://host.example.test:8443/r?mail=a@b.example",
    "https://[::1]/r@x",
    "https://host.example.test/a@b, https://h2.example.test",
  ])("keeps an @ after the host: %s", (text) => {
    expect(redactCredentials(text)).toBe(text);
  });

  it("finds a second URL right after a first one without user information", () => {
    expect(redactCredentials("https://host.example.test/x,https://u:p@h2.example.test/y")).toBe(
      "https://host.example.test/x,https://***@h2.example.test/y",
    );
  });

  it("keeps text without credentials", () => {
    expect(redactCredentials("plain text")).toBe("plain text");
    expect(redactCredentials("")).toBe("");
  });
});
