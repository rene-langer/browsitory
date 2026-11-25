import { describe, expect, it } from "vitest";
import { validateRemoteUrls } from "./validateRemoteUrls";

describe("validateRemoteUrls", () => {
  // A remote whose saved URL holds user info is listed with it hidden as `***` (#1081); sent
  // back unchanged, the backend keeps the saved URL, and refuses any other.
  it.each([
    ["https://***@bitbucket.example.test/ws/r.git", null],
    ["https://example.test/r.git", "http://***@example.test:8080/r.git"],
  ])("leaves hidden user info (%s, %s) to the backend", (fetchUrl, pushUrl) => {
    expect(() => validateRemoteUrls(fetchUrl, pushUrl)).not.toThrow();
  });

  it.each([
    "https://alice@example.test/r.git",
    "https://***:secret@example.test/r.git",
    "https://alice:***@example.test/r.git",
    "https://****@example.test/r.git",
  ])("refuses other user info: %s", (url) => {
    expect(() => validateRemoteUrls(url, null)).toThrow("Remote URLs must not contain embedded credentials");
    expect(() => validateRemoteUrls("https://example.test/r.git", url)).toThrow(
      "Remote URLs must not contain embedded credentials",
    );
  });
});
