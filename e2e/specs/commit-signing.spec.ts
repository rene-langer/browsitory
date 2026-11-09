import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { gitIn } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "signing-e2e.txt";
const SUMMARY = "e2e: signed commit";

const git = gitIn(E2E_REPO_PATH);

describe("Browsitory commit signing", () => {
  let keyDir = "";

  before(function () {
    try {
      execFileSync("ssh-keygen", ["-?"], { stdio: "ignore" });
    } catch (error) {
      // ssh-keygen -? exits non-zero but exists; only ENOENT means it's missing.
      if ((error as NodeJS.ErrnoException).code === "ENOENT") this.skip();
    }
    keyDir = fs.mkdtempSync(path.join(os.tmpdir(), "browsitory-e2e-ssh-"));
    const key = path.join(keyDir, "id_e2e");
    execFileSync("ssh-keygen", ["-q", "-t", "ed25519", "-N", "", "-C", "e2e@example.com", "-f", key]);
    const pub = fs.readFileSync(`${key}.pub`, "utf8").trim();
    const email = git("config", "user.email");
    fs.writeFileSync(path.join(keyDir, "allowed_signers"), `${email} ${pub}\n`);
    git("config", "commit.gpgSign", "true");
    git("config", "gpg.format", "ssh");
    git("config", "user.signingKey", key);
    git("config", "gpg.ssh.allowedSignersFile", path.join(keyDir, "allowed_signers"));
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "signed\n");
  });

  after(() => {
    for (const key of ["commit.gpgSign", "gpg.format", "user.signingKey", "gpg.ssh.allowedSignersFile"]) {
      try {
        git("config", "--unset", key);
      } catch {
        // already unset
      }
    }
    if (keyDir) fs.rmSync(keyDir, { recursive: true, force: true });
  });

  it("commits a signed commit and shows a good signature", async () => {
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist();
    const stageButton = await $(`button[aria-label="Stage ${FILE}"]`);
    await stageButton.waitForExist();
    // Same opacity-0 workaround as first-flow.spec.ts.
    await browser.execute((el) => (el as HTMLElement).scrollIntoView({ block: "center" }), stageButton);
    await browser.execute((el) => (el as HTMLElement).click(), stageButton);
    await browser.execute(
      (el, value) => {
        const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
        setter?.call(el, value);
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      commitMessageInput,
      SUMMARY,
    );
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Commit"));

    await browser.waitUntil(() => git("log", "-1", "--format=%s") === SUMMARY, {
      timeoutMsg: "commit did not land",
    });
    if (!git("cat-file", "commit", "HEAD").includes("-----BEGIN SSH SIGNATURE-----")) {
      throw new Error("HEAD commit is not ssh-signed");
    }

    const row = await $(`li*=${SUMMARY}`);
    await row.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), row);
    await browser.waitUntil(
      async () => (await browser.execute(() => document.body.textContent ?? "")).includes("Good signature"),
      { timeoutMsg: "expected 'Good signature' in the commit header" },
    );
  });
});
