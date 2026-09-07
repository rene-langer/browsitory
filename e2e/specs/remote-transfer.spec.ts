import { execFileSync } from "node:child_process";
import fs from "node:fs";
import { createServer } from "node:https";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-transfer-remote.git");
const REMOTE_SOURCE_PATH = path.join(os.tmpdir(), "browsitory-e2e-transfer-source");
const TRANSFER_SEED_FILE = "remote-transfer-seed.txt";
const BRANCH_PUSH_FILE = "branch-push.txt";
const CREDENTIAL_KEY_PATH = process.env.BROWSITORY_E2E_CREDENTIAL_KEY;
const CREDENTIAL_CERT_PATH = process.env.BROWSITORY_E2E_CREDENTIAL_CERT;

async function startCredentialChallengeServer(): Promise<{
  url: string;
  requests: () => number;
  close: () => Promise<void>;
}> {
  if (CREDENTIAL_KEY_PATH === undefined || CREDENTIAL_CERT_PATH === undefined) {
    throw new Error("credential fixture certificate was not prepared before the Tauri session");
  }
  let requests = 0;
  const server = createServer({ key: fs.readFileSync(CREDENTIAL_KEY_PATH), cert: fs.readFileSync(CREDENTIAL_CERT_PATH) }, (_request, response) => {
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
    requests: () => requests,
    close: () => new Promise((resolve, reject) => server.close((error) => {
      error === undefined ? resolve() : reject(error);
    })),
  };
}

// Remotes and branches now live in one unified tree (`BranchTree.tsx`, replacing the separate
// `RemotePanel` accordion), and every mutating remote action moved from a persistent
// button/aria-label to a right-click context menu on the remote's folder header. This dispatches
// a synthetic DOM `contextmenu` event (see rebase.spec.ts's comment on why, not
// WebdriverIO's `.click({ button: "right" })`), then clicks the named menu item — which, being a
// `ContextMenu`, closes itself immediately after the click, so no reference to the item survives
// past this call.
async function clickRemoteContextItem(remoteName: string, itemLabel: string) {
  const remoteHeader = await $(`button=${remoteName}`);
  await remoteHeader.waitForExist({ timeout: 10000 });
  await browser.execute((el) => {
    el.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
    );
  }, remoteHeader);
  const menuItem = await $(`button=${itemLabel}`);
  await menuItem.waitForExist({ timeout: 10000 });
  await menuItem.waitForEnabled({ timeout: 10000 });
  await menuItem.click();
}

// "Set upstream…" is a context-menu item on the *current* local branch's row (its text carries
// the " (current)" suffix, and the row itself is the interactive element — a `role="button"`
// `<li>`, not a nested `<button>` — see `ListRow.tsx`).
async function openSetUpstreamDialog() {
  const currentBranchButton = await $('//li[@role="button" and contains(., \' (current)\')]');
  await currentBranchButton.waitForExist({ timeout: 10000 });
  await browser.execute((el) => {
    el.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
    );
  }, currentBranchButton);
  await (await $("button=Set upstream…")).click();
  const dialog = await $("dialog[aria-label^='Set upstream for']");
  await dialog.waitForExist({ timeout: 10000 });
  return dialog;
}

describe("Browsitory remote transfer", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, TRANSFER_SEED_FILE), "transfer seed\n");
    execFileSync("git", ["add", "README.md", TRANSFER_SEED_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
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
    // Remotes now live inside the unified "Branches" tree; it defaults closed, expand it before
    // its Add button exists.
    await expandSidebarSection("Branches");

    // Adding a remote is reached via the tree's "Add" toolbar button (opens a context menu with
    // "New Branch…"/"Add Remote…"), not a standalone "Add remote" toggle.
    const addButton = await $('[aria-label="Add"]');
    await addButton.waitForExist({ timeout: 10000 });
    await addButton.click();
    await (await $("button=Add Remote…")).click();

    const remoteNameInput = await $("form[aria-label='Add remote'] input:nth-of-type(1)");
    await remoteNameInput.waitForExist({ timeout: 10000 });
    const addRemoteButton = await (await $("form[aria-label='Add remote']")).$("button=Add remote");
    await addRemoteButton.waitForEnabled({ timeout: 10000 });
    await remoteNameInput.setValue("transfer-origin");
    await (await $("[data-testid='add-remote-fetch-url']")).setValue(BARE_REMOTE_PATH);
    await addRemoteButton.click();

    const remoteFolderHeader = await $("button=transfer-origin");
    await remoteFolderHeader.waitForExist({ timeout: 10000 });
    await clickRemoteContextItem("transfer-origin", "Fetch");

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
    // The old "fetch button stays enabled after completion" check doesn't translate: "Fetch" is
    // now a context-menu item that closes (and unmounts) itself the instant it's clicked, so no
    // reference to it survives to re-check. The remote-transfer flow completing at all (asserted
    // above) already exercises the same underlying `onFetchRemote` call.
  });

  it("remediates a missing HTTPS credential without exposing the callback diagnostic", async () => {
    // Idempotent re-expand — see the previous test's comment.
    await expandSidebarSection("Branches");

    const challenge = await startCredentialChallengeServer();
    try {
      // Left open by the previous test's successful add (it doesn't auto-close on success); only
      // open it here if that isn't the case.
      if (!(await $("form[aria-label='Add remote']").isExisting())) {
        const addButton = await $('[aria-label="Add"]');
        await addButton.waitForExist({ timeout: 10000 });
        await addButton.click();
        await (await $("button=Add Remote…")).click();
      }
      const remoteNameInput = await $("form[aria-label='Add remote'] input:nth-of-type(1)");
      const addRemoteButton = await (await $("form[aria-label='Add remote']")).$("button=Add remote");
      await addRemoteButton.waitForEnabled({ timeout: 10000 });
      await remoteNameInput.setValue("credential-origin");
      await (await $("[data-testid='add-remote-fetch-url']")).setValue(challenge.url);
      await addRemoteButton.click();
      await (await $("button=credential-origin")).waitForExist({ timeout: 10000 });

      // This is the non-secret metadata the UI normally persists before saving a token. No
      // token is saved, so the loopback server invokes the real missing-credential callback.
      execFileSync("git", ["config", "--local", "browsitory.remote.credential-origin.auth-mode", "https-token"], { cwd: E2E_REPO_PATH });
      execFileSync("git", ["config", "--local", "browsitory.remote.credential-origin.username", "e2e-user"], { cwd: E2E_REPO_PATH });
      expect(
        execFileSync("git", ["remote", "get-url", "credential-origin"], {
          cwd: E2E_REPO_PATH,
          encoding: "utf8",
        }).trim(),
      ).toBe(challenge.url);

      await clickRemoteContextItem("credential-origin", "Fetch");
      await browser.waitUntil(() => challenge.requests() > 0, {
        timeout: 10000,
        timeoutMsg: "expected Fetch to reach the loopback HTTPS credential challenge",
      });
      const alert = await $("p[role='alert']");
      await alert.waitForExist({ timeout: 10000 });
      await expect(alert).toHaveText(expect.stringMatching(/Save an HTTPS token for this remote before retrying\.|operating-system credential store is unavailable/));
      expect(await alert.getText()).not.toContain(challenge.url);
    } finally {
      await challenge.close();
    }
  });

  it("fast-forwards a clean tracked upstream", async () => {
    // Idempotent re-expand — see the first test's comment.
    await expandSidebarSection("Branches");

    expect(execFileSync("git", ["status", "--porcelain"], { cwd: E2E_REPO_PATH, encoding: "utf8" }).trim()).toBe("");
    const upstreamDialog = await openSetUpstreamDialog();
    const upstreamRemote = await upstreamDialog.$("select");
    await upstreamRemote.selectByAttribute("value", "transfer-origin");
    const upstreamBranch = await upstreamDialog.$("input");
    await upstreamBranch.setValue("main");
    await (await upstreamDialog.$("button=Set upstream")).click();

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
    // "Branches" holds the remote-folder's push action; "Tags" holds the tag-creation form and
    // the push-tags controls used later in this test. Both default closed.
    await expandSidebarSection("Branches");
    await expandSidebarSection("Tags");

    try {
      execFileSync("git", ["config", "--local", "--unset-all", "browsitory.remote.transfer-origin.auth-mode"], {
        cwd: E2E_REPO_PATH,
        stdio: "ignore",
      });
    } catch {
      // Auth mode may be absent in a fresh fixture.
    }
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

    await clickRemoteContextItem("transfer-origin", "Push current branch here");
    await browser.waitUntil(
      () => execFileSync("git", ["rev-parse", `refs/heads/${currentBranch}`], { cwd: BARE_REMOTE_PATH, encoding: "utf8" }).trim() === localHead,
      { timeout: 10000, timeoutMsg: "expected Push to advance the remote branch" },
    );

    const createTagButton = await $("button=Create tag");
    await createTagButton.waitForEnabled({ timeout: 10000 });
    await (await $("form[aria-label='Create tag'] input")).setValue("e2e-transfer-tag");
    await createTagButton.click();
    await browser.waitUntil(
      () => {
        try {
          execFileSync("git", ["show-ref", "--verify", "refs/tags/e2e-transfer-tag"], {
            cwd: E2E_REPO_PATH,
            stdio: "ignore",
          });
          return true;
        } catch {
          return false;
        }
      },
      { timeout: 10000, timeoutMsg: "expected local tag creation to complete" },
    );
    const pushTags = await $("button=Push all tags");
    await pushTags.waitForEnabled({ timeout: 10000 });
    await $("section[aria-labelledby='push-tags-heading'] select").selectByAttribute("value", "transfer-origin");
    await browser.execute((el) => (el as HTMLElement).click(), pushTags);

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
    // Idempotent re-expand — see the first test's comment.
    await expandSidebarSection("Branches");

    await clickRemoteContextItem("transfer-origin", "Manage credentials");
    // The `aria-label` naming this dialog now lives on the `<dialog>` itself, not a nested
    // `<form>` (the form carries no attributes of its own).
    const credentialsForm = await $("dialog[aria-label='Credentials for transfer-origin']");
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
