import fs from "node:fs";
import { createServer } from "node:https";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage } from "../support/dom";
import { ADD_REMOTE_NAME_INPUT, expandSidebarSection, sidebarRemoteFolder } from "../support/sidebar";
import { git, gitWith } from "../support/git";

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
  const remoteHeader = await $(sidebarRemoteFolder(remoteName));
  await remoteHeader.waitForExist();
  await browser.execute((el) => {
    el.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
    );
  }, remoteHeader);
  // The context menu's item, not the sync bar's button of the same name (Fetch).
  const menuItem = await $("[role='menu']").$(`button=${itemLabel}`);
  await menuItem.waitForExist();
  await menuItem.waitForEnabled();
  await menuItem.click();
}

// "Set upstream…" is a context-menu item on the *current* local branch's row (its text carries
// the " (current)" suffix, and the row itself is the interactive element — a `role="button"`
// `<li>`, not a nested `<button>` — see `ListRow.tsx`).
async function openSetUpstreamDialog() {
  const currentBranchButton = await $('//li[@role="button" and contains(., \' (current)\')]');
  await currentBranchButton.waitForExist();
  await browser.execute((el) => {
    el.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
    );
  }, currentBranchButton);
  await (await $("button=Set upstream…")).click();
  const dialog = await $("dialog[aria-label^='Set upstream for']");
  await dialog.waitForExist();
  return dialog;
}

describe("Browsitory remote transfer", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, TRANSFER_SEED_FILE), "transfer seed\n");
    git(E2E_REPO_PATH, "add", "README.md", TRANSFER_SEED_FILE);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: seed transfer base");
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    fs.rmSync(REMOTE_SOURCE_PATH, { recursive: true, force: true });
    git(os.tmpdir(), "init", "--bare", BARE_REMOTE_PATH);
    const localBranch = git(E2E_REPO_PATH, "branch", "--show-current");
    const initialRefspecs = localBranch === "main" ? ["HEAD:main"] : ["HEAD:main", `HEAD:${localBranch}`];
    git(E2E_REPO_PATH, "push", BARE_REMOTE_PATH, ...initialRefspecs);
    git(os.tmpdir(), "clone", "--branch", "main", BARE_REMOTE_PATH, REMOTE_SOURCE_PATH);
    git(REMOTE_SOURCE_PATH, "config", "user.name", "Test User");
    git(REMOTE_SOURCE_PATH, "config", "user.email", "test@example.com");
    fs.writeFileSync(path.join(REMOTE_SOURCE_PATH, "remote-change.txt"), "remote change\n");
    git(REMOTE_SOURCE_PATH, "add", "remote-change.txt");
    git(REMOTE_SOURCE_PATH, "commit", "-m", "e2e: remote change");
    git(REMOTE_SOURCE_PATH, "push", "origin", "main");
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
    // "New branch…"/"Add remote…"), not a standalone "Add remote" toggle.
    const addButton = await $('[aria-label="Add"]');
    await addButton.waitForExist();
    await addButton.click();
    await (await $("button=Add remote…")).click();

    const remoteNameInput = await $(ADD_REMOTE_NAME_INPUT);
    await remoteNameInput.waitForExist();
    const addRemoteButton = await (await $("form[aria-label='Add remote']")).$("button=Add remote");
    await addRemoteButton.waitForEnabled();
    await remoteNameInput.setValue("transfer-origin");
    await (await $("[data-testid='add-remote-fetch-url']")).setValue(BARE_REMOTE_PATH);
    await addRemoteButton.click();

    const remoteFolderHeader = await $(sidebarRemoteFolder("transfer-origin"));
    await remoteFolderHeader.waitForExist();
    await clickRemoteContextItem("transfer-origin", "Fetch");

    const remoteHead = git(REMOTE_SOURCE_PATH, "rev-parse", "refs/heads/main");
    await browser.waitUntil(() => {
      try {
        return git(E2E_REPO_PATH, "rev-parse", "refs/remotes/transfer-origin/main") === remoteHead;
      } catch {
        return false;
      }
    }, {
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
        await addButton.waitForExist();
        await addButton.click();
        await (await $("button=Add remote…")).click();
      }
      const remoteNameInput = await $(ADD_REMOTE_NAME_INPUT);
      const addRemoteButton = await (await $("form[aria-label='Add remote']")).$("button=Add remote");
      await addRemoteButton.waitForEnabled();
      await remoteNameInput.setValue("credential-origin");
      await (await $("[data-testid='add-remote-fetch-url']")).setValue(challenge.url);
      await addRemoteButton.click();
      await (await $(sidebarRemoteFolder("credential-origin"))).waitForExist();

      // This is the non-secret metadata the UI normally persists before saving a token. No
      // token is saved, so the loopback server invokes the real missing-credential callback.
      git(E2E_REPO_PATH, "config", "--local", "browsitory.remote.credential-origin.auth-mode", "https-token");
      git(E2E_REPO_PATH, "config", "--local", "browsitory.remote.credential-origin.username", "e2e-user");
      expect(
        git(E2E_REPO_PATH, "remote", "get-url", "credential-origin"),
      ).toBe(challenge.url);

      await clickRemoteContextItem("credential-origin", "Fetch");
      await browser.waitUntil(() => challenge.requests() > 0, {
        timeoutMsg: "expected Fetch to reach the loopback HTTPS credential challenge",
      });
      const alert = await $("p[role='alert']");
      await alert.waitForExist();
      await expect(alert).toHaveText(expect.stringMatching(/Save an HTTPS token for this remote before retrying\.|operating-system credential store is unavailable/));
      expect(await alert.getText()).not.toContain(challenge.url);
    } finally {
      await challenge.close();
    }
  });

  it("fast-forwards a clean tracked upstream", async () => {
    // Idempotent re-expand — see the first test's comment.
    await expandSidebarSection("Branches");

    expect(git(E2E_REPO_PATH, "status", "--porcelain")).toBe("");
    const upstreamDialog = await openSetUpstreamDialog();
    const upstreamRemote = await upstreamDialog.$("select");
    await upstreamRemote.selectByAttribute("value", "transfer-origin");
    const upstreamBranch = await upstreamDialog.$("input");
    await upstreamBranch.setValue("main");
    await (await upstreamDialog.$("button=Set upstream")).click();

    // The repository toolbar's Pull (#637): the sidebar no longer has one of its own.
    const pullButton = await $("div[aria-label='Repository']").$("button=Pull");
    await pullButton.waitForEnabled();
    await pullButton.click();

    const remoteHead = git(REMOTE_SOURCE_PATH, "rev-parse", "refs/heads/main");
    await browser.waitUntil(
      () => git(E2E_REPO_PATH, "rev-parse", "HEAD") === remoteHead,
      { timeoutMsg: "expected Pull to fast-forward the local branch" },
    );
  });

  it("pushes the current branch and a tag created at an older commit", async () => {
    // "Branches" holds the remote-folder's push action; "Tags" holds the push-tags controls used
    // later in this test. Both default closed.
    await expandSidebarSection("Branches");
    await expandSidebarSection("Tags");

    try {
      gitWith(E2E_REPO_PATH, ["config", "--local", "--unset-all", "browsitory.remote.transfer-origin.auth-mode"], {
        stdio: "ignore",
      });
    } catch {
      // Auth mode may be absent in a fresh fixture.
    }
    const currentBranch = git(E2E_REPO_PATH, "branch", "--show-current");
    // Pin git's default push.default so the host's config cannot change the flow below.
    gitWith(E2E_REPO_PATH, ["config", "--local", "push.default", "simple"], { stdio: "ignore" });
    const configured = (key: string) => {
      try {
        return git(E2E_REPO_PATH, "config", key);
      } catch {
        return null;
      }
    };
    // The earlier fast-forward test made `transfer-origin/main` the upstream. When the current
    // branch has another name (e.g. `master`), `simple` must not push onto `main`: Push opens
    // "Push to…" prefilled with the remote and the local name instead.
    const upstreamBranch =
      configured(`branch.${currentBranch}.remote`) === "transfer-origin"
        ? (configured(`branch.${currentBranch}.merge`) ?? "").replace(/^refs\/heads\//, "")
        : null;
    const needsChoice = upstreamBranch !== null && upstreamBranch !== currentBranch;
    const bareTip = (branch: string) => {
      try {
        return git(BARE_REMOTE_PATH, "rev-parse", "--verify", "--quiet", `refs/heads/${branch}`);
      } catch {
        return null;
      }
    };
    const remoteHeadBeforePush = bareTip(currentBranch);
    const upstreamBeforePush = upstreamBranch === null ? null : bareTip(upstreamBranch);
    fs.writeFileSync(path.join(E2E_REPO_PATH, BRANCH_PUSH_FILE), "branch push\n");
    git(E2E_REPO_PATH, "add", BRANCH_PUSH_FILE);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: branch push change");
    const localHead = git(E2E_REPO_PATH, "rev-parse", "HEAD");
    expect(localHead).not.toBe(remoteHeadBeforePush);

    await clickRemoteContextItem("transfer-origin", "Push current branch here");
    if (needsChoice) {
      const dialog = await $(`dialog[aria-label="Push ${currentBranch}"]`);
      await dialog.waitForExist();
      await expect(await dialog.$("select")).toHaveValue("transfer-origin");
      await expect(await dialog.$("input:not([type='checkbox'])")).toHaveValue(currentBranch);
      await expect(await dialog.$("input[type='checkbox']")).not.toBeSelected();
      await (await dialog.$("button=Push")).click();
    }
    await browser.waitUntil(() => bareTip(currentBranch) === localHead, {
      timeoutMsg: "expected Push to advance the remote branch",
    });
    // The differently named upstream branch is never the silent target.
    if (needsChoice) expect(bareTip(upstreamBranch)).toBe(upstreamBeforePush);

    // #438: tag the commit before HEAD from its row in the graph, without checking it out.
    const parent = git(E2E_REPO_PATH, "rev-parse", "HEAD~1");
    const parentRow = await $(`li.commit-row[data-commit-id="${parent}"]`);
    await parentRow.waitForExist();
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
    }, parentRow);
    await clickInPage("[role='menu'] button", "Create tag…");
    const tagDialog = await $("dialog[aria-label='Create tag']");
    await tagDialog.waitForExist();
    await expect(tagDialog).toHaveText(expect.stringContaining(parent.slice(0, 7)));
    await (await tagDialog.$("//label[contains(., 'Tag name')]/following-sibling::input")).setValue("e2e-transfer-tag");
    await clickInPage("dialog[aria-label='Create tag'] button[type='submit']", "Create tag");
    await browser.waitUntil(
      () => {
        try {
          return (
            git(E2E_REPO_PATH, "rev-parse", "refs/tags/e2e-transfer-tag^{commit}") === parent
          );
        } catch {
          return false;
        }
      },
      { timeoutMsg: "expected the tag to be created at the older commit" },
    );
    expect(git(E2E_REPO_PATH, "rev-parse", "HEAD")).toBe(localHead);
    const pushTags = await $("button=Push all tags");
    await pushTags.waitForEnabled();
    await $("section[aria-labelledby='push-tags-heading'] select").selectByAttribute("value", "transfer-origin");
    await browser.execute((el) => (el as HTMLElement).click(), pushTags);

    await browser.waitUntil(
      () => {
        try {
          gitWith(BARE_REMOTE_PATH, ["show-ref", "--verify", "refs/tags/e2e-transfer-tag"], { stdio: "ignore" });
          return true;
        } catch {
          return false;
        }
      },
      { timeoutMsg: "expected tag push to complete" },
    );
  });

  it("deletes a tag on the remote and, when asked, locally too", async () => {
    // #439. Follows the push above, which left `e2e-transfer-tag` on `transfer-origin`.
    await expandSidebarSection("Tags");
    const hasTag = (cwd: string) => {
      try {
        gitWith(cwd, ["show-ref", "--verify", "refs/tags/e2e-transfer-tag"], { stdio: "ignore" });
        return true;
      } catch {
        return false;
      }
    };
    expect(hasTag(BARE_REMOTE_PATH)).toBe(true);

    // The Tags panel row's "…" menu (shown on hover/focus, clickable in the page either way).
    const actions = await $('button[aria-label="Actions for e2e-transfer-tag"]');
    await actions.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), actions);
    await clickInPage("[role='menu'] button", "Delete from remote…");
    const dialog = await $("dialog[aria-label='Delete tag e2e-transfer-tag from remote']");
    await dialog.waitForExist();
    // A picker only appears with several remotes; aim at this spec's own remote either way.
    const picker = await dialog.$("select");
    if (await picker.isExisting()) await picker.selectByAttribute("value", "transfer-origin");
    await expect(dialog).toHaveText(expect.stringContaining("Delete tag e2e-transfer-tag on transfer-origin? This can't be undone."));
    await (await dialog.$("//label[contains(., 'Also delete the local tag')]/input")).click();
    await (await dialog.$("button=Delete from remote")).click();

    await browser.waitUntil(() => !hasTag(BARE_REMOTE_PATH), {
      timeoutMsg: "expected the tag to be deleted on the remote",
    });
    await browser.waitUntil(() => !hasTag(E2E_REPO_PATH), {
      timeoutMsg: "expected the local tag to be deleted after the remote delete",
    });
  });

  it("selects SSH-agent authentication without rendering an HTTPS token field", async () => {
    // Idempotent re-expand — see the first test's comment.
    await expandSidebarSection("Branches");

    await clickRemoteContextItem("transfer-origin", "Manage credentials");
    // The `aria-label` naming this dialog now lives on the `<dialog>` itself, not a nested
    // `<form>` (the form carries no attributes of its own).
    const credentialsForm = await $("dialog[aria-label='Credentials for transfer-origin']");
    await credentialsForm.waitForExist();
    await credentialsForm.$("select").selectByAttribute("value", "SshAgent");

    expect(await credentialsForm.$("input[type='password']").isExisting()).toBe(false);

    await (await $("button=Use SSH agent")).click();
    await browser.waitUntil(
      () => {
        try {
          return git(E2E_REPO_PATH, "config", "--get", "browsitory.remote.transfer-origin.auth-mode") === "ssh-agent";
        } catch {
          return false;
        }
      },
      { timeoutMsg: "expected SSH-agent mode to be stored without an HTTPS credential" },
    );
    expect(
      git(E2E_REPO_PATH, "config", "--get-regexp", "^browsitory\\.remote\\.transfer-origin\\."),
    ).toBe("browsitory.remote.transfer-origin.auth-mode ssh-agent");
  });
});
