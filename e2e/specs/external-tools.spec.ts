import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, setValueInPage, waitUntilGoneInPage, waitUntilInPage } from "../support/dom";
import { FAKE_DIFF_TOOL_NAME, FAKE_TOOL, fakeToolLaunches } from "../support/fakeTool";
import { gitIn } from "../support/git";

// External tools (#128): "Open in editor" runs the editor command from Preferences, and "Open in
// diff tool" the git diff tool, with the paths a real tool would get. Both are one fake script
// that records its arguments (`support/fakeTool.ts`); `wdio.conf.ts` gives this spec's session a
// HOME whose git config names it as `diff.tool`.
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const CONFIG_FILE = path.join(os.tmpdir(), "browsitory-e2e-config", "config.toml");
const FILE = "tools-e2e.txt";
const git = gitIn(E2E_REPO_PATH);

async function pickFileAction(label: string) {
  await clickInPage(`button[aria-label="Actions for ${FILE}"]`);
  await clickInPage("[role='menu'] [role='menuitem']", label);
  await waitUntilGoneInPage("[role='menu']");
}

async function waitForLaunches(count: number): Promise<string[][]> {
  await browser.waitUntil(() => fakeToolLaunches().length >= count, {
    timeoutMsg: `the fake tool was not launched ${count} time(s): ${JSON.stringify(fakeToolLaunches())}`,
  });
  return fakeToolLaunches();
}

describe("Browsitory external tools", () => {
  before(function () {
    // The fake tool is a shell script; the Windows launch paths are smoke-tested by hand (#128).
    if (process.platform === "win32") this.skip();
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "one\ntwo\nthree\n");
    git("add", FILE);
    git("commit", "-m", "e2e: external tools base");
    // Line 1 changes, so the first hunk starts at line 1 whatever the context-line setting.
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "ONE\ntwo\nthree\n");
  });

  after(() => {
    if (process.platform === "win32") return;
    // config.toml is shared by every spec file: drop the editor command again.
    const text = fs.readFileSync(CONFIG_FILE, "utf8");
    fs.writeFileSync(CONFIG_FILE, text.replace(/^editor = .*\n/m, ""));
    git("checkout", "--", FILE);
  });

  it("shows the git diff tool and saves the editor command in Preferences", async () => {
    await clickInPage("header button[aria-label='Preferences']");
    await clickInPage("dialog[aria-label='Preferences'] button[role='tab']", "External tools");
    // The diff tool comes from the session's own HOME (wdio.conf.ts), never the developer's.
    await waitUntilInPage("dialog[aria-label='Preferences'] dd", FAKE_DIFF_TOOL_NAME, { contains: true });

    await setValueInPage("input[placeholder='System default app']", `${FAKE_TOOL} editor {file} {line}`);
    await clickInPage("dialog[aria-label='Preferences'] button[type='submit']", "Save");
    await browser.waitUntil(() => fs.readFileSync(CONFIG_FILE, "utf8").includes(`editor = "${FAKE_TOOL} editor {file} {line}"`), {
      timeoutMsg: "the editor command did not reach config.toml",
    });
    await clickInPage("button", "Close");
  });

  it("opens a changed file in the editor at its first hunk's line", async () => {
    const before = fakeToolLaunches().length;
    // The line comes from the file's loaded diff.
    await browser.waitUntil(() => browser.execute(() => document.body.textContent?.includes("ONE") ?? false), {
      timeoutMsg: "the file's diff never showed",
    });
    await pickFileAction("Open in editor");

    const launch = (await waitForLaunches(before + 1))[before];
    expect(launch).toEqual(["arg: editor", `arg: ${path.join(E2E_REPO_PATH, FILE)}`, "arg: 1"]);
  });

  it("opens a changed file in the diff tool: the staged version against the working tree file", async () => {
    const before = fakeToolLaunches().length;
    await pickFileAction("Open in diff tool");

    const launch = (await waitForLaunches(before + 1))[before];
    const [command, local, remote, ...rest] = launch;
    expect(command).toBe("arg: diff");
    // LOCAL is a temporary copy of the index version; REMOTE the working tree file itself.
    expect(local).toMatch(/^arg: .*[\\/]browsitory-tool-[^\\/]+[\\/]tools-e2e_LOCAL\.txt$/);
    expect(remote).toBe(`arg: ${path.join(E2E_REPO_PATH, FILE)}`);
    expect(rest).toEqual(["LOCAL contents:", "one", "two", "three", `REMOTE: ${path.join(E2E_REPO_PATH, FILE)}`]);
  });
});
