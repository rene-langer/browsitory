import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
// Side-effect-only imports: register webdriverio's and @wdio/types's `declare global {
// namespace WebdriverIO { ... } }` augmentations, which `WebdriverIO.Config` below (and the
// element/browser globals `$`/`browser` used implicitly by the spec files) are typed against.
// Listing "webdriverio"/"@wdio/types" in tsconfig's `types` array doesn't do this, since that
// array only resolves entries under typeRoots (effectively `node_modules/@types/*`), and both
// packages ship their own types rather than `@types/*` packages.
import "webdriverio";
import "@wdio/types";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Path to the debug build produced by `cargo build --workspace` from the repo root.
const tauriAppBinary = path.resolve(__dirname, "../target/debug/tauri-app");

// Fixed fixture-repo path. This CANNOT be a freshly-`mktemp`'d path chosen at test-run time:
// `App.tsx` reads it from `import.meta.env.VITE_E2E_REPO_PATH`, a Vite env var that gets
// baked into `frontend/dist` at *frontend build time* (`pnpm --dir frontend build`), not at
// `wdio run` time — by the time this suite runs, the value is already frozen inside the
// built `tauri-app` binary. So the frontend must be built with
// `VITE_E2E_REPO_PATH=<this exact path>` *before* `cargo build --workspace` embeds
// `frontend/dist` (see the CI `e2e` job, and this file's `onPrepare` below).
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

export const config: WebdriverIO.Config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: 4444,
  specs: ["./specs/**/*.spec.ts"],
  maxInstances: 1,
  capabilities: [
    {
      // @ts-expect-error — tauri:options isn't in WebdriverIO's built-in capability types.
      // NOTE: the brief's draft also included a per-capability `maxInstances: 1` and
      // `browserName: "wry"` here; the live guide's current example capability object has
      // neither (just `maxInstances`+`tauri:options` at this same nesting, without
      // `browserName`), and per-capability `maxInstances` doesn't type-check against
      // WebdriverIO v9's `RequestedStandaloneCapabilities`, so both are dropped — the
      // top-level `maxInstances: 1` above already caps this to one instance (see task report).
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
