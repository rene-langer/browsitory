import { getConfig, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

// setupTests.ts builds the DOM dump in Testing Library's errors only when the message is read
// (#217). These pin down that a failure still shows the rendered DOM, in both `message` and
// `stack`, the two fields Vitest reports.
describe("Testing Library element errors", () => {
  it("show the rendered DOM when a failed getBy* is caught and read in the test", () => {
    render(<p>rendered-for-sync-failure</p>);

    let caught: unknown;
    try {
      screen.getByRole("button", { name: "missing" });
    } catch (error) {
      caught = error;
    }

    expect(caught).toBeInstanceOf(Error);
    const error = caught as Error;
    expect(error.name).toBe("TestingLibraryElementError");
    expect(error.message).toContain('Unable to find an accessible element with the role "button" and name "missing"');
    expect(error.message).toContain("rendered-for-sync-failure");
    expect(error.stack).toContain("rendered-for-sync-failure");
  });

  it("show the rendered DOM when a findBy* times out", async () => {
    render(<p>rendered-for-timeout</p>);

    const error: Error = await screen.findByText("missing", {}, { timeout: 100 }).then(
      () => {
        throw new Error("findByText should not have found anything");
      },
      (rejection: Error) => rejection,
    );

    expect(error.name).toBe("TestingLibraryElementError");
    expect(error.message).toContain("Unable to find an element with the text: missing");
    expect(error.message).toContain("rendered-for-timeout");
    expect(error.stack).toContain("rendered-for-timeout");
  });

  it("keep a message that was assigned, as waitFor does on a timeout", () => {
    const error = getConfig().getElementError("original", document.body);
    error.message = "replaced";
    expect(error.message).toBe("replaced");
  });
});
