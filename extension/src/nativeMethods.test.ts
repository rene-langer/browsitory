import * as fs from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";
import { NATIVE_METHODS } from "./sidecarBridge";

// The frontend's transport parity test (#306, `frontend/src/ipc/transportParity.test.ts`) reads
// this list to check that every method the VSCode webview calls is answered by the sidecar or by
// the extension host. Regenerate with `UPDATE_TRANSPORT_METHODS=1 pnpm test -- --run`.
const GENERATED = path.resolve(
  __dirname,
  "../../frontend/src/ipc/generated/extension-native-methods.json",
);

describe("NATIVE_METHODS", () => {
  it("matches the checked-in list the frontend parity test reads", () => {
    const expected = JSON.stringify([...NATIVE_METHODS].sort(), null, 2) + "\n";
    if (process.env["UPDATE_TRANSPORT_METHODS"] === "1") {
      fs.writeFileSync(GENERATED, expected);
      return;
    }
    const actual = fs.existsSync(GENERATED) ? fs.readFileSync(GENERATED, "utf8") : "";
    expect(
      actual.replace(/\r\n/g, "\n"),
      `${GENERATED} is out of date; rerun with UPDATE_TRANSPORT_METHODS=1`,
    ).toBe(expected);
  });
});
