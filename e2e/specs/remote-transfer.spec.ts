import { execFileSync } from "node:child_process";
import fs from "node:fs";
import { createServer } from "node:https";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-transfer-remote.git");
const REMOTE_SOURCE_PATH = path.join(os.tmpdir(), "browsitory-e2e-transfer-source");
const TRANSFER_SEED_FILE = "remote-transfer-seed.txt";
const BRANCH_PUSH_FILE = "branch-push.txt";

async function startCredentialChallengeServer(): Promise<{
  url: string;
  certificatePath: string;
  requests: () => number;
  close: () => Promise<void>;
}> {
  const certificateDir = fs.mkdtempSync(path.join(os.tmpdir(), "browsitory-e2e-credential-cert-"));
  const keyPath = path.join(certificateDir, "key.pem");
  const certificatePath = path.join(certificateDir, "cert.pem");
  execFileSync("openssl", [
    "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", keyPath, "-out", certificatePath,
    "-days", "1", "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost",
  ], { stdio: "ignore" });
  let requests = 0;
  const server = createServer({ key: fs.readFileSync(keyPath), cert: fs.readFileSync(certificatePath) }, (_request, response) => {
    requests += 1;
    response.writeHead(401, { "WWW-Authenticate": 'Basic realm="Browsitory E2E"' });
    response.end();
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (address === null || typeof address === "string") throw new Error("credential challenge server has no TCP address");

  return {
    url: `https://localhost:${address.port}/credential.git`,
    certificatePath,
    requests: () => requests,
    close: () => new Promise((resolve, reject) => server.close((error) => {
      fs.rmSync(certificateDir, { recursive: true, force: true });
      error === undefined ? resolve() : reject(error);
    })),
  };
}

describe("Browsitory remote transfer", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, TRANSFER_SEED_FILE), "transfer seed\n");
    execFileSync("git", ["add", TRANSFER_SEED_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: seed transfer base"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    fs.rmSync(REMOTE_SOURCE_PATH, { recursive: true, force: true });
    execFileSync("git", ["init", "--bare", BARE_REMOTE_PATH], { stdio: "inherit" });
    const localBranch = execFileSync("git", ["branch", "--show-current"], { cwd: E2E_REPO_PATH, encoding: "utf8" }).trim();
    const initialRefspecs = localBranch === "main" ? ["HEAD:main"] : ["HEAD:main", `HEAD:${localBranch}`];
    execFileSync("git", ["push", BARE_REMOTE_PATH, ...initialRefspecs], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["clone", "--branch", "main", BARE_REMOTE_PATH, REMOTE_SOURCE_PATH], { stdio: "inherit" });
    execFileSync("git", ["config", "user.name", "Test User"], { cwd: REMOTE_SOURCE_PATH, stdio: "inherit" });
    execFileSync("git", ["config", "user.email", "test@example.com"], { cwd: REMOTE_SOURCE_PATH, stdio: "inherit" });
    fs.writeFileSync(path.join(REMOTE_SOURCE_PATH, "remote-change.txt"), "remote change\n");
    execFileSync("git", ["add", "remote-change.txt"], { cwd: REMOTE_SOURCE_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: remote change"], { cwd: REMOTE_SOURCE_PATH, stdio: "inherit" });
    execFileSync("git", ["push", "origin", "main"], { cwd: REMOTE_SOURCE_PATH, stdio: "inherit" });
  });

  after(() => {
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    fs.rmSync(REMOTE_SOURCE_PATH, { recursive: true, force: true });
  });

  it("fetches a configured remote", async () => {
    const remoteNameInput = await $("form[aria-label='Add remote'] input:nth-of-type(1)");
    await remoteNameInput.waitForExist({ timeout: 10000 });
    await remoteNameInput.setValue("transfer-origin");
    await (await $("[data-testid='add-remote-fetch-url']")).setValue(BARE_REMOTE_PATH);
    await (await $("button=Add remote")).click();

    const fetchButton = await $("button=Fetch transfer-origin");
    await fetchButton.waitForExist({ timeout: 10000 });
    await fetchButton.click();

    const remoteHead = execFileSync("git", ["rev-parse", "refs/heads/main"], {
      cwd: REMOTE_SOURCE_PATH,
      encoding: "utf8",
    }).trim();
    await browser.waitUntil(() => {
      try {
        return execFileSync("git", ["rev-parse", "refs/remotes/transfer-origin/main"], {
          cwd: E2E_REPO_PATH,
          encoding: "utf8",
        }).trim() === remoteHead;
      } catch {
        return false;
      }
    }, {
      timeout: 10000,
      timeoutMsg: "expected Fetch to update the transfer-origin tracking ref",
    });
    await expect(fetchButton).toBeEnabled();
  });

  it("remediates a missing HTTPS credential without exposing the callback diagnostic", async () => {
    const challenge = await startCredentialChallengeServer();
    try {
      const remoteNameInput = await $("form[aria-label='Add remote'] input:nth-of-type(1)");
      await remoteNameInput.setValue("credential-origin");
      await (await $("[data-testid='add-remote-fetch-url']")).setValue(challenge.url);
      await (await $("button=Add remote")).click();
      await (await $("button=Fetch credential-origin")).waitForExist({ timeout: 10000 });

      // This is the non-secret metadata the UI normally persists before saving a token. No
      // token is saved, so the loopback server invokes the real missing-credential callback.
      execFileSync("git", ["config", "--local", "browsitory.remote.credential-origin.auth-mode", "https-token"], { cwd: E2E_REPO_PATH });
      execFileSync("git", ["config", "--local", "browsitory.remote.credential-origin.username", "e2e-user"], { cwd: E2E_REPO_PATH });
      execFileSync("git", ["config", "--local", "http.sslCAInfo", challenge.certificatePath], { cwd: E2E_REPO_PATH });

      await (await $("button=Fetch credential-origin")).click();
      await browser.waitUntil(() => challenge.requests() > 0, {
        timeout: 10000,
        timeoutMsg: "expected Fetch to reach the loopback HTTPS credential challenge",
      });
      const alert = await $("p[role='alert']");
      await alert.waitForExist({ timeout: 10000 });
      await expect(alert).toHaveText("Save an HTTPS token for this remote before retrying.");
      expect(await alert.getText()).not.toContain(challenge.url);
    } finally {
      await challenge.close();
    }
  });

  it("fast-forwards a clean tracked upstream", async () => {
    const upstreamRemote = await $("form[aria-label='Set upstream'] select");
    await upstreamRemote.selectByAttribute("value", "transfer-origin");
    const upstreamBranch = await $("form[aria-label='Set upstream'] input");
    await upstreamBranch.setValue("main");
    await (await $("button=Set upstream")).click();

    const pullButton = await $("button=Pull");
    await pullButton.waitForEnabled({ timeout: 10000 });
    await pullButton.click();

    const remoteHead = execFileSync("git", ["rev-parse", "refs/heads/main"], {
      cwd: REMOTE_SOURCE_PATH,
      encoding: "utf8",
    }).trim();
    await browser.waitUntil(
      () => execFileSync("git", ["rev-parse", "HEAD"], { cwd: E2E_REPO_PATH, encoding: "utf8" }).trim() === remoteHead,
      { timeout: 10000, timeoutMsg: "expected Pull to fast-forward the local branch" },
    );
  });

  it("pushes the current branch and a local tag", async () => {
    const currentBranch = execFileSync("git", ["branch", "--show-current"], {
      cwd: E2E_REPO_PATH,
      encoding: "utf8",
    }).trim();
    const remoteHeadBeforePush = execFileSync("git", ["rev-parse", `refs/heads/${currentBranch}`], {
      cwd: BARE_REMOTE_PATH,
      encoding: "utf8",
    }).trim();
    fs.writeFileSync(path.join(E2E_REPO_PATH, BRANCH_PUSH_FILE), "branch push\n");
    execFileSync("git", ["add", BRANCH_PUSH_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: branch push change"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    const localHead = execFileSync("git", ["rev-parse", "HEAD"], { cwd: E2E_REPO_PATH, encoding: "utf8" }).trim();
    expect(localHead).not.toBe(remoteHeadBeforePush);

    const pushBranch = await $("button=Push branch to transfer-origin");
    await pushBranch.waitForEnabled({ timeout: 10000 });
    await pushBranch.click();
    await browser.waitUntil(
      () => execFileSync("git", ["rev-parse", `refs/heads/${currentBranch}`], { cwd: BARE_REMOTE_PATH, encoding: "utf8" }).trim() === localHead,
      { timeout: 10000, timeoutMsg: "expected Push to advance the remote branch" },
    );

    await (await $("form[aria-label='Create tag'] input")).setValue("e2e-transfer-tag");
    await (await $("button=Create tag")).click();
    const pushTags = await $("button=Push all tags");
    await pushTags.waitForEnabled({ timeout: 10000 });
    await pushTags.click();

    await browser.waitUntil(
      () => {
        try {
          execFileSync("git", ["show-ref", "--verify", "refs/tags/e2e-transfer-tag"], { cwd: BARE_REMOTE_PATH, stdio: "ignore" });
          return true;
        } catch {
          return false;
        }
      },
      { timeout: 10000, timeoutMsg: "expected tag push to complete" },
    );
  });

  it("selects SSH-agent authentication without rendering an HTTPS token field", async () => {
    await (await $("button=Credentials for transfer-origin")).click();
    const credentialsForm = await $("form[aria-label='Credentials for transfer-origin']");
    await credentialsForm.waitForExist({ timeout: 10000 });
    await credentialsForm.$("select").selectByAttribute("value", "SshAgent");

    expect(await credentialsForm.$("input[type='password']").isExisting()).toBe(false);

    await (await $("button=Use SSH agent")).click();
    await browser.waitUntil(
      () => {
        try {
          return execFileSync(
            "git",
            ["config", "--get", "browsitory.remote.transfer-origin.auth-mode"],
            { cwd: E2E_REPO_PATH, encoding: "utf8" },
          ).trim() === "ssh-agent";
        } catch {
          return false;
        }
      },
      { timeout: 10000, timeoutMsg: "expected SSH-agent mode to be stored without an HTTPS credential" },
    );
    expect(
      execFileSync("git", ["config", "--get-regexp", "^browsitory\\.remote\\.transfer-origin\\."], {
        cwd: E2E_REPO_PATH,
        encoding: "utf8",
      }).trim(),
    ).toBe("browsitory.remote.transfer-origin.auth-mode ssh-agent");
  });
});
