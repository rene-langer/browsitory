import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const SOURCE = path.join(os.tmpdir(), "browsitory-e2e-clone-source.git");
const PARENT = path.join(os.tmpdir(), "browsitory-e2e-clones");
const NAME = "e2e-cloned";

async function setInput(label: string, value: string) {
  const input = await $(`aria/${label}`);
  await input.waitForExist({ timeout: 10000 });
  await input.setValue(value);
}

describe("Browsitory clone", () => {
  before(() => {
    fs.rmSync(SOURCE, { recursive: true, force: true });
    fs.rmSync(PARENT, { recursive: true, force: true });
    fs.mkdirSync(PARENT, { recursive: true });
    execFileSync("git", ["clone", "--bare", E2E_REPO_PATH, SOURCE], { stdio: "inherit" });
  });

  it("clones a repository from the picker and opens it in a new tab", async () => {
    await $('section[aria-label="Branches"] button[aria-expanded]').waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), await $('button[aria-label="Open another repository"]'));
    const cloneButton = await $("button=Clone…");
    await cloneButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), cloneButton);

    await setInput("Repository URL", `file://${SOURCE.replace(/\\/g, "/")}`);
    await setInput("Parent folder", PARENT);
    const nameInput = await $("aria/Folder name");
    await nameInput.clearValue();
    await nameInput.setValue(NAME);
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Clone"));

    const tab = await $(`[role="tab"]*=${NAME}`);
    await tab.waitForExist({ timeout: 30000 });
    expect(fs.existsSync(path.join(PARENT, NAME, ".git"))).toBe(true);
    const seedCommit = await $("li*=e2e: add submodule");
    await seedCommit.waitForExist({ timeout: 15000 });
  });
});
