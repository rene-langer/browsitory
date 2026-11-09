// Wait budgets for the Tauri E2E suite, in milliseconds (#500).

/**
 * The default for every WebdriverIO wait (`waitFor*`, `waitUntil`, the element commands' implicit
 * wait and `expect`'s retries): `wdio.conf.ts` sets it as `waitforTimeout`, so a wait that is
 * fine with it passes no `timeout`. Tune it here for slow runners.
 */
export const UI_TIMEOUT = 10_000;

/** For a wait on a slower round trip: a push to a local remote, a search, a hook that is cancelled. */
export const SLOW_UI_TIMEOUT = 15_000;

/** For a wait across an app restart or the end of a session (the app and its WebKit helpers exiting). */
export const RESTART_TIMEOUT = 20_000;
