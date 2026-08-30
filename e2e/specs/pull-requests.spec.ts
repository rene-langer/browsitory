import { expect } from "@wdio/globals";
import { ForgeFixtureClient } from "../support/forgeFixtureServer";
import { expandSidebarSection } from "../support/sidebar";

// Canned provider responses, matching the exact JSON shapes
// `crates/repo-service/src/pull_requests.rs`'s GitHub/Bitbucket adapters parse (see that module's
// `GITHUB_LIST_FIXTURE`/`BITBUCKET_LIST_FIXTURE`/etc. unit-test constants, mirrored here).
const GITHUB_LIST_FIXTURE = [
  {
    id: 101,
    number: 7,
    title: "Add pull request support",
    html_url: "https://github.com/acme/widget/pull/7",
    user: { login: "rene" },
    head: { ref: "feature/pr" },
    base: { ref: "main" },
    state: "open",
  },
];

const GITHUB_CREATE_FIXTURE = {
  id: 202,
  number: 8,
  title: "Add feature",
  html_url: "https://github.com/acme/widget/pull/8",
  user: { login: "rene" },
  head: { ref: "feature/pr" },
  base: { ref: "main" },
  state: "open",
};

const BITBUCKET_LIST_FIXTURE = {
  values: [
    {
      id: 12,
      title: "Add pull request support",
      links: { html: { href: "https://bitbucket.org/acme/widget/pull-requests/12" } },
      author: { display_name: "Rene Langer" },
      source: { branch: { name: "feature/pr" } },
      destination: { branch: { name: "main" } },
      state: "OPEN",
    },
  ],
};

const BITBUCKET_CREATE_FIXTURE = {
  id: 13,
  title: "Add feature",
  links: { html: { href: "https://bitbucket.org/acme/widget/pull-requests/13" } },
  author: { display_name: "Rene Langer" },
  source: { branch: { name: "feature/pr" } },
  destination: { branch: { name: "main" } },
  state: "OPEN",
};

async function addRemote(name: string, url: string) {
  // The Add-remote form is reached via `BranchTree`'s "Add" toolbar button (opens a context menu
  // with "New Branch…"/"Add Remote…"), not a standalone "Add remote" toggle, and it stays open
  // after a successful add — so only open it when it isn't already showing (the first call in a
  // test, typically).
  if (!(await $("form[aria-label='Add remote']").isExisting())) {
    const addButton = await $('[aria-label="Add"]');
    await addButton.waitForExist({ timeout: 10000 });
    await addButton.click();
    await (await $("button=Add Remote…")).click();
  }
  const remoteNameInput = await $("form[aria-label='Add remote'] input:nth-of-type(1)");
  await remoteNameInput.waitForExist({ timeout: 10000 });
  await remoteNameInput.setValue(name);
  await (await $("[data-testid='add-remote-fetch-url']")).setValue(url);
  await (await $("button=Add remote")).click();
  await browser.waitUntil(async () => (await remoteNameInput.getValue()) === "", {
    timeout: 10000,
    timeoutMsg: `expected remote ${name} to finish being added`,
  });
}

describe("Browsitory pull requests", () => {
  const server = ForgeFixtureClient.fromEnv();

  before(async () => {
    await server.reset();
    await server.setResponse("github-list", 200, GITHUB_LIST_FIXTURE);
    await server.setResponse("github-create", 201, GITHUB_CREATE_FIXTURE);
    await server.setResponse("bitbucket-list", 200, BITBUCKET_LIST_FIXTURE);
    await server.setResponse("bitbucket-create", 201, BITBUCKET_CREATE_FIXTURE);
  });

  it("renders sections only for supported remotes and sends no request for an unsupported one", async () => {
    // "Branches" holds the Add remote form `addRemote` drives (remotes now live inside the
    // unified Branches tree, not their own "Remotes" section); "Pull Requests" holds the
    // per-remote sections asserted on below. Both default closed.
    await expandSidebarSection("Branches");
    await expandSidebarSection("Pull Requests");

    await addRemote("gh-origin", "https://github.com/acme/widget.git");
    await addRemote("bb-origin", "https://bitbucket.org/acme/widget.git");
    await addRemote("gl-origin", "https://gitlab.com/acme/widget.git");

    // `PullRequestPanel`'s reskin onto the `Panel` primitive replaced the old
    // `aria-labelledby="pull-request-section-{remoteName}"` heading-id wiring with a plain
    // `aria-label` on the section, composed as "{provider}: {owner}/{name} ({remoteName})" (see
    // `PullRequestPanel.tsx`'s `sectionLabel`) — selectors below match that new label text.
    const githubSection = await $("section[aria-label='GitHub: acme/widget (gh-origin)']");
    await githubSection.waitForExist({ timeout: 10000 });
    const bitbucketSection = await $("section[aria-label='Bitbucket: acme/widget (bb-origin)']");
    await expect(bitbucketSection).toBeExisting();

    const gitlabSection = await $("section[aria-label='GitLab: acme/widget (gl-origin)']");
    expect(await gitlabSection.isExisting()).toBe(false);

    // Adding remotes (including the unsupported one) never makes a pull-request HTTP call by
    // itself — only an explicit List/Create action does, and gl-origin renders no such controls
    // to click in the first place.
    expect(await server.requestCount()).toBe(0);
  });

  // `PullRequestPanel` keys `state.pullRequests` per remote (see `useAppState.ts`), so each
  // repository's section renders only its own listed rows independently of any other remote's
  // section. These two `it()` blocks still run as separate self-contained scenarios per
  // provider purely for readability — not because listing one remote would otherwise clobber
  // or hide another remote's rows.
  it("lists and creates a GitHub pull request using the saved token, then hides it once forgotten", async () => {
    // "Pull Requests" was already opened by the previous `it()` in this file (its open state
    // persists in localStorage across `beforeTest`'s page refresh), but expand it again here too
    // — idempotent, and keeps this test self-sufficient if run in isolation.
    await expandSidebarSection("Pull Requests");

    const section = await $("section[aria-label='GitHub: acme/widget (gh-origin)']");
    await (await section.$("aria/Account")).setValue("rene");
    await (await section.$("aria/Access token")).setValue("gh-test-token");
    await (await section.$("button=Save token")).click();
    await browser.waitUntil(
      async () => (await (await section.$("aria/Access token")).getValue()) === "",
      { timeout: 10000, timeoutMsg: "expected the GitHub token input to clear after save" },
    );

    await (await section.$("button=List pull requests")).click();
    const listedRow = await section.$("li*=Add pull request support");
    await listedRow.waitForExist({ timeout: 10000 });
    expect(await listedRow.getText()).toContain("#7");
    expect(await listedRow.getText()).toContain("feature/pr");

    const listRequest = await server.lastRequestFor("github-list");
    expect(listRequest?.authorization).toBe("Bearer gh-test-token");

    const createForm = await section.$("form[aria-label='Create pull request for gh-origin']");
    await (await createForm.$("aria/Title")).setValue("Add feature");
    await (await createForm.$("aria/Source branch")).setValue("feature/pr");
    await (await createForm.$("aria/Target branch")).setValue("main");
    await (await createForm.$("button=Create pull request")).click();

    const createdRow = await section.$("li*=Add feature");
    await createdRow.waitForExist({ timeout: 10000 });
    expect(await createdRow.getText()).toContain("#8");
    expect(await (await createForm.$("aria/Title")).getValue()).toBe("");

    const createRequest = await server.lastRequestFor("github-create");
    expect(createRequest?.authorization).toBe("Bearer gh-test-token");
    expect(createRequest?.body).toEqual({
      title: "Add feature",
      body: "",
      head: "feature/pr",
      base: "main",
    });

    await (await section.$("button=Forget token")).click();
    await browser.waitUntil(
      async () => !(await section.$("li*=Add pull request support").isExisting()),
      { timeout: 10000, timeoutMsg: "expected the stale pull-request list to be hidden after forgetting the token" },
    );
  });

  it("warns about app passwords and lists/creates a Bitbucket pull request using the saved token", async () => {
    // See the GitHub test above: idempotent re-expand, self-sufficient if run in isolation.
    await expandSidebarSection("Pull Requests");

    const section = await $("section[aria-label='Bitbucket: acme/widget (bb-origin)']");
    await expect(section).toHaveText(expect.stringContaining("repository or workspace access token (not an app password)"));

    await (await section.$("aria/Account")).setValue("rene");
    await (await section.$("aria/Access token")).setValue("bb-test-token");
    await (await section.$("button=Save token")).click();
    await browser.waitUntil(
      async () => (await (await section.$("aria/Access token")).getValue()) === "",
      { timeout: 10000, timeoutMsg: "expected the Bitbucket token input to clear after save" },
    );

    await (await section.$("button=List pull requests")).click();
    const listedRow = await section.$("li*=Add pull request support");
    await listedRow.waitForExist({ timeout: 10000 });
    expect(await listedRow.getText()).toContain("#12");

    const listRequest = await server.lastRequestFor("bitbucket-list");
    expect(listRequest?.authorization).toBe("Bearer bb-test-token");

    const createForm = await section.$("form[aria-label='Create pull request for bb-origin']");
    await (await createForm.$("aria/Title")).setValue("Add feature");
    await (await createForm.$("aria/Source branch")).setValue("feature/pr");
    await (await createForm.$("aria/Target branch")).setValue("main");
    await (await createForm.$("button=Create pull request")).click();

    const createdRow = await section.$("li*=Add feature");
    await createdRow.waitForExist({ timeout: 10000 });
    expect(await createdRow.getText()).toContain("#13");

    const createRequest = await server.lastRequestFor("bitbucket-create");
    expect(createRequest?.authorization).toBe("Bearer bb-test-token");
    expect(createRequest?.body).toEqual({
      title: "Add feature",
      description: "",
      source: { branch: { name: "feature/pr" } },
      destination: { branch: { name: "main" } },
    });
  });
});
