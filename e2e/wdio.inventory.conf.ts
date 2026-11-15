import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { config as screenshotsConfig } from "./wdio.screenshots.conf";
import { SCREENSHOT_ROOT } from "./screenshots/demoRepo";

// UX-review screenshot inventory (not the guide screenshots; see screenshots/README.md). Reuses the documentation-screenshot harness
// (demo repo, isolated HOME, temp config and webview data) and adds a few extra repositories to
// the recent list: an empty repository, so the picker can open it.

type Hook = (...args: unknown[]) => unknown;

export const EMPTY_REPO_PATH = path.join(SCREENSHOT_ROOT, "empty-repo");

export const config: WebdriverIO.Config = {
  ...screenshotsConfig,
  specs: ["./screenshots/capture.inventory.ts"],
  mochaOpts: { ...screenshotsConfig.mochaOpts, bail: false, timeout: 0 },
  onPrepare: async (...args: unknown[]) => {
    await (screenshotsConfig.onPrepare as Hook)(...args);
    fs.rmSync(EMPTY_REPO_PATH, { recursive: true, force: true });
    execFileSync("git", ["init", "-q", "-b", "main", EMPTY_REPO_PATH]);
    const configDir = process.env.BROWSITORY_CONFIG_DIR!;
    const demo = path.join(SCREENSHOT_ROOT, "trailmap");
    fs.writeFileSync(path.join(configDir, "config.toml"), `recent_repos = ["${demo}", "${EMPTY_REPO_PATH}", "${path.join(SCREENSHOT_ROOT, "moved-away")}"]\n`);
  },
};
