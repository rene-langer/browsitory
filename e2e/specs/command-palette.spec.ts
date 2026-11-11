import { expect } from "@wdio/globals";
import { openPalette, waitForAppReady } from "../support/app";
import { sidebarSectionToggle } from "../support/sidebar";

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

    const input = await openPalette();
    await input.setValue("refresh");
    await browser.keys(["Enter"]);
    await input.waitForDisplayed({ reverse: true });

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
    const input = await openPalette();
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
        timeoutMsg: "expected filtering on 'prune worktrees' to surface Prune worktrees as the top result",
      },
    );

    await browser.keys(["Enter"]);
    await input.waitForDisplayed({ reverse: true });
  });

  it("a navigate command expands the target sidebar section", async () => {
    const trigger = await $(sidebarSectionToggle("Worktrees"));
    await trigger.waitForExist();
    // This section is always rendered (even with no linked worktrees), so no worktree fixture
    // data is needed here. Its open/closed state is persisted to localStorage (see
    // `AccordionSection`) in the same on-disk webview profile this whole suite reuses run over
    // run, so — unlike a brand-new profile — it isn't guaranteed closed just because this spec
    // sorts before `worktree.spec.ts` (the only other spec that opens it) within a single run.
    // Force it closed first so the assertion below actually exercises the open transition.
    if ((await trigger.getAttribute("aria-expanded")) === "true") {
      await trigger.click();
      await browser.waitUntil(async () => (await trigger.getAttribute("aria-expanded")) === "false", {
        timeoutMsg: "expected the Worktrees sidebar section to start closed",
      });
    }

    const input = await openPalette();
    await input.setValue("go to worktrees");
    await browser.keys(["Enter"]);
    await input.waitForDisplayed({ reverse: true });

    await browser.waitUntil(async () => (await trigger.getAttribute("aria-expanded")) === "true", {
      timeoutMsg: "expected the Worktrees sidebar section to expand after the navigate command",
    });
  });

  it("shows the third-party licences from the bundled notices file", async () => {
    await waitForAppReady();
    const input = await openPalette();
    await input.setValue("third-party licences");
    await browser.keys(["Enter"]);
    await input.waitForDisplayed({ reverse: true });

    // The text region renders only once third-party-notices.md was fetched from the app's own
    // assets, so this proves the CSP's connect-src and the asset path. `textContent` via
    // `browser.execute`: see the note on `getText()` and native <dialog>s above.
    await browser.waitUntil(
      async () =>
        (await browser.execute(
          () => document.querySelector('pre[aria-label="Licence texts"]')?.textContent ?? "",
        )).includes("option-ext"),
      { timeoutMsg: "expected the licences dialog to show the notices file" },
    );
    await browser.keys(["Escape"]);
  });

  it("shows a command's shortcut, and F6 moves between the panes (#646, #670)", async () => {
    await waitForAppReady();
    const input = await openPalette();
    await input.setValue("refresh");
    // `textContent` via `browser.execute`: see the note on `getText()` and native <dialog>s above.
    await browser.waitUntil(
      async () =>
        (await browser.execute(() => {
          const row = Array.from(document.querySelectorAll("dialog li")).find((li) => li.textContent?.startsWith("Refresh"));
          return row?.querySelector("kbd")?.textContent ?? "";
        })) === "F5",
      { timeoutMsg: "expected Refresh to show its F5 shortcut in the palette" },
    );
    await browser.keys(["Escape"]);
    await input.waitForDisplayed({ reverse: true });

    // The panes in order (the sidebar may be hidden in a narrow window), then where each F6 lands.
    const panes = await browser.execute(() =>
      Array.from(document.querySelectorAll('[data-active-repo="true"] [data-pane]'))
        .filter((pane) => pane.closest("[hidden]") === null)
        .map((pane) => pane.getAttribute("aria-label")),
    );
    expect(panes).toEqual(expect.arrayContaining(["Repository", "History", "Diff"]));
    const visited: (string | null)[] = [];
    for (let press = 0; press < panes.length; press++) {
      await browser.keys(["F6"]);
      visited.push(
        await browser.execute(() => document.activeElement?.closest("[data-pane]")?.getAttribute("aria-label") ?? null),
      );
    }
    // Every pane once, in order, wherever the focus started.
    const start = panes.indexOf(visited[0]);
    expect(start).not.toBe(-1);
    expect(visited).toEqual(panes.map((_, index) => panes[(start + index) % panes.length]));
  });

  it("closes on Escape without running anything", async () => {
    await waitForAppReady();
    const input = await openPalette();
    await input.setValue("prune worktrees");
    await browser.keys(["Escape"]);
    await input.waitForDisplayed({ reverse: true });
  });
});
