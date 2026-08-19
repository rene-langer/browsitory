import { expect } from "@wdio/globals";

// App.tsx's Ctrl/Cmd+K handler no-ops until `state.repoPath` is set (see its early return), and
// `wdio.conf.ts`'s `beforeTest` reloads the page before every `it`, so the repo's async auto-open
// (and this guard) can still be in flight right when a test starts — sending Ctrl+K too early is
// silently swallowed. Sidebar sections only render once the repo has loaded, so waiting for one
// to exist is this suite's established way to gate on "app finished loading" (see how
// `pull-requests.spec.ts`/`remote-management.spec.ts`/etc. all wait on a section's trigger or
// form before their first interaction).
async function waitForAppReady(): Promise<void> {
  await $('section[aria-label="Branches"] button[aria-expanded]').waitForExist({ timeout: 10000 });
}

describe("command palette", () => {
  it("opens with Ctrl/Cmd+K, filters, and runs a zero-arg command", async () => {
    // Toggle theme is a safe, observable zero-arg effect, but it isn't in `buildCommands`
    // (App.tsx injects it separately, outside this spec's scope per the design spec). Use
    // "Refresh" instead — a real `buildCommands` entry — and assert the palette closes
    // afterward, the observable effect every command shares. The theme snapshot is a sanity
    // check that Refresh didn't touch anything unrelated. Snapshot only after the app is ready:
    // `applyTheme`'s effect (which sets this dataset attribute) hasn't necessarily run yet on
    // the very first tick after `beforeTest`'s reload, so reading it too early races the theme
    // effect itself and can observe `null` before the app finishes mounting.
    await waitForAppReady();
    const before = await browser.execute(() => document.documentElement.dataset.theme);

    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed({ timeout: 10000 });
    await input.setValue("refresh");
    await browser.keys(["Enter"]);
    await input.waitForDisplayed({ reverse: true, timeout: 10000 });

    const after = await browser.execute(() => document.documentElement.dataset.theme);
    expect(after).toBe(before);
  });

  it("runs a single-pick command directly from the flat list", async () => {
    // "Fetch origin" isn't available here: the fixture repo (see `wdio.conf.ts`'s
    // `setupFixtureRepo`/`resetFixtureRepo`) has no remotes, and this spec sorts alphabetically
    // before `remote-management.spec.ts`, the only spec that adds one (transiently, via the UI,
    // within its own test). "Prune worktrees" is an unconditional `buildCommands` entry (unlike
    // "Pull", which only appears when `state.upstream` is set) that needs no fixture data and is
    // a harmless no-op with no worktrees present, so it stands in as the single-pick command.
    // Scoped to `dialog li`, since the palette's own commands list is the only `<li>` list
    // inside a `<dialog>` (the `Overlay` primitive) — the app's sidebar/history `<li>`s live
    // outside any dialog, so an unscoped `$$("li")` would pull those in too.
    await waitForAppReady();
    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed({ timeout: 10000 });
    await input.setValue("prune worktrees");

    // Filtering re-renders on React state driven by `setValue`'s keystrokes; querying the DOM
    // immediately can race that re-render, so poll rather than reading the list once. Reads
    // `textContent` via `browser.execute` rather than WebdriverIO's `getText()`: the palette's
    // list sits inside a native `<dialog>` opened with `showModal()` (see `Overlay`), and this
    // environment's WebKitGTK-based `tauri-driver` returns an empty string from `getText()` for
    // elements promoted to the top layer that way (confirmed empirically — `isDisplayed()` and
    // the underlying DOM both report the element correctly populated) even though `getText()`
    // works fine for other dialogs in this suite that aren't backed by a native `<dialog>`.
    await browser.waitUntil(
      async () => (await browser.execute(() => document.querySelector("dialog li")?.textContent ?? "")).includes("Prune worktrees"),
      {
        timeout: 10000,
        timeoutMsg: "expected filtering on 'prune worktrees' to surface Prune worktrees as the top result",
      },
    );

    await browser.keys(["Enter"]);
    await input.waitForDisplayed({ reverse: true, timeout: 10000 });
  });

  it("a navigate command expands the target sidebar section", async () => {
    const trigger = await $('section[aria-label="Worktrees"] button[aria-expanded]');
    await trigger.waitForExist({ timeout: 10000 });
    // This section is always rendered (even with no linked worktrees), so no worktree fixture
    // data is needed here. Its open/closed state is persisted to localStorage (see
    // `AccordionSection`) in the same on-disk webview profile this whole suite reuses run over
    // run, so — unlike a brand-new profile — it isn't guaranteed closed just because this spec
    // sorts before `worktree.spec.ts` (the only other spec that opens it) within a single run.
    // Force it closed first so the assertion below actually exercises the open transition.
    if ((await trigger.getAttribute("aria-expanded")) === "true") {
      await trigger.click();
      await browser.waitUntil(async () => (await trigger.getAttribute("aria-expanded")) === "false", {
        timeout: 10000,
        timeoutMsg: "expected the Worktrees sidebar section to start closed",
      });
    }

    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed({ timeout: 10000 });
    await input.setValue("go to worktrees");
    await browser.keys(["Enter"]);
    await input.waitForDisplayed({ reverse: true, timeout: 10000 });

    await browser.waitUntil(async () => (await trigger.getAttribute("aria-expanded")) === "true", {
      timeout: 10000,
      timeoutMsg: "expected the Worktrees sidebar section to expand after the navigate command",
    });
  });

  it("closes on Escape without running anything", async () => {
    await waitForAppReady();
    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed({ timeout: 10000 });
    await input.setValue("prune worktrees");
    await browser.keys(["Escape"]);
    await input.waitForDisplayed({ reverse: true, timeout: 10000 });
  });
});
