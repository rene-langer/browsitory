import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { config as e2eConfig } from "./wdio.conf";
import { DEMO_REPO_PATH, buildDemoRepo, isolateHome } from "./screenshots/demoRepo";

// Documentation screenshots (see screenshots/README.md). Reuses the E2E harness — the same
// build, tauri-driver handling, temp config dir and temp webview data dir as wdio.conf.ts — and
// adds:
// - a fixed demo repository (screenshots/demoRepo.ts) instead of the E2E fixtures;
// - a temp HOME, XDG dirs, git global config and GnuPG home, so neither the run nor the app
//   reads or writes the user's own config;
// - no page reload between tests: a reload would make the E2E build reopen its fixture repo.

type Hook = (...args: unknown[]) => unknown;

export const config: WebdriverIO.Config = {
  ...e2eConfig,
  specs: ["./screenshots/**/*.docs.ts"],
  mochaOpts: { ...e2eConfig.mochaOpts, bail: true },

  onPrepare: async (...args: unknown[]) => {
    // Resolve tauri-driver from the real HOME before `isolateHome` replaces it.
    process.env.TAURI_DRIVER_PATH ??= path.resolve(os.homedir(), ".cargo", "bin", "tauri-driver");
    isolateHome();
    await (e2eConfig.onPrepare as Hook)(...args);
    buildDemoRepo();
    // Replace the E2E suite's seeded config: the only recent repository is the demo, so the
    // spec can open it from the repository picker.
    const configDir = process.env.BROWSITORY_CONFIG_DIR;
    if (configDir === undefined) throw new Error("wdio.conf.ts's onPrepare did not set BROWSITORY_CONFIG_DIR");
    fs.writeFileSync(
      path.join(configDir, "config.toml"),
      `recent_repos = ["${DEMO_REPO_PATH.replace(/\\/g, "\\\\")}"]\n`,
    );
  },

  beforeTest: async () => {},
};
