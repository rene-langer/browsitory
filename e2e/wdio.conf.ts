import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import type { Options } from "@wdio/types";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Path to the debug build produced by `cargo build --workspace` from the repo root.
const tauriAppBinary = path.resolve(__dirname, "../target/debug/tauri-app");

// Fixed fixture-repo path. This CANNOT be a freshly-`mktemp`'d path chosen at test-run time:
// `App.tsx` reads it from `import.meta.env.VITE_E2E_REPO_PATH`, a Vite env var that gets
// baked into `frontend/dist` at *frontend build time* (`pnpm --dir frontend build`), not at
// `wdio run` time — by the time this suite runs, the value is already frozen inside the
// built `tauri-app` binary. So the frontend must be built with
// `VITE_E2E_REPO_PATH=<this exact path>` *before* `cargo build --workspace` embeds
// `frontend/dist`. See e2e/README-less inline docs below and the CI `e2e` job.
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

// The app auto-opens E2E_REPO_PATH as soon as it launches (App.tsx's mount effect), and the
// app launches as part of establishing the WebDriver session — i.e. *before* any mocha
// `before`/`it` hook in the spec file gets to run. So the fixture repo has to exist on disk
// before the session starts, which means `onPrepare` (runs once, before any session) rather
// than a per-spec mocha `before` hook (runs after the session/app is already up).
function setupFixtureRepo(repoPath: string) {
  fs.rmSync(repoPath, { recursive: true, force: true });
  fs.mkdirSync(repoPath, { recursive: true });
  execFileSync("git", ["init"], { cwd: repoPath, stdio: "inherit" });
  fs.writeFileSync(path.join(repoPath, "README.md"), "e2e fixture repo\n");
}

// Keep track of the `tauri-driver` child process, following the official Tauri WebdriverIO
// example (https://v2.tauri.app/develop/tests/webdriver/example/webdriverio/) — tauri-driver
// isn't a WebdriverIO "service" (no @wdio/tauri-service is used here), so this config spawns
// and reaps it itself around each session.
let tauriDriver: ChildProcess | undefined;
let exiting = false;

function closeTauriDriver() {
  exiting = true;
  tauriDriver?.kill();
}

function onShutdown(fn: () => void) {
  const cleanup = () => {
    try {
      fn();
    } finally {
      process.exit();
    }
  };
  process.on("exit", cleanup);
  process.on("SIGINT", cleanup);
  process.on("SIGTERM", cleanup);
  process.on("SIGHUP", cleanup);
}
onShutdown(() => closeTauriDriver());

export const config: Options.Testrunner = {
  runner: "local",
  hostname: "127.0.0.1",
  port: 4444,
  specs: ["./specs/**/*.spec.ts"],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      // @ts-expect-error — tauri:options isn't in WebdriverIO's built-in capability types.
      // NOTE: the brief's draft also included `browserName: "wry"` here; the live guide's
      // current example capability object omits `browserName` entirely, so it's dropped here
      // too (see task report for detail).
      "tauri:options": {
        application: tauriAppBinary,
      },
    },
  ],
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 60000,
  },

  onPrepare: () => {
    if (!fs.existsSync(tauriAppBinary)) {
      throw new Error(
        `tauri-app binary not found at ${tauriAppBinary}. Run \`cargo build --workspace\` ` +
          `from the repo root first (with frontend/dist already built with ` +
          `VITE_E2E_REPO_PATH=${E2E_REPO_PATH} baked in).`,
      );
    }
    setupFixtureRepo(E2E_REPO_PATH);
  },

  // Ensure `tauri-driver` is running before the session starts so we can proxy the WebDriver
  // requests to it (127.0.0.1:4444, matching `hostname`/`port` above).
  beforeSession: () => {
    const driverPath = path.resolve(os.homedir(), ".cargo", "bin", "tauri-driver");
    tauriDriver = spawn(driverPath, [], { stdio: [null, process.stdout, process.stderr] });
    tauriDriver.on("error", (error) => {
      console.error("tauri-driver error:", error);
      process.exit(1);
    });
    tauriDriver.on("exit", (code) => {
      if (!exiting) {
        console.error("tauri-driver exited with code:", code);
        process.exit(1);
      }
    });
  },

  afterSession: () => {
    closeTauriDriver();
  },
};
