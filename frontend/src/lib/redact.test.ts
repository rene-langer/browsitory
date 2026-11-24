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

  it("keeps text without credentials", () => {
    expect(redactCredentials("plain text")).toBe("plain text");
    expect(redactCredentials("")).toBe("");
  });
});
