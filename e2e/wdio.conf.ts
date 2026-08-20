import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
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
import { closeSharedForgeFixtureServer, startSharedForgeFixtureServer } from "./support/forgeFixtureServer";

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
const E2E_SECOND_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-second-repo");
const E2E_CONFIG_DIR = path.join(os.tmpdir(), "browsitory-e2e-config");
const CREDENTIAL_CERT_DIR = path.join(os.tmpdir(), "browsitory-e2e-credential-cert");
const E2E_PARENT_SOURCE_PATH = path.join(os.tmpdir(), "browsitory-e2e-parent-source");
const E2E_SUBMODULE_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-submodule");
const E2E_SUBMODULE_PATH = "deps/e2e-child";
const CREDENTIAL_KEY_PATH = path.join(CREDENTIAL_CERT_DIR, "key.pem");
const CREDENTIAL_CERT_PATH = path.join(CREDENTIAL_CERT_DIR, "cert.pem");

// The app auto-opens E2E_REPO_PATH as soon as it launches (App.tsx's mount effect), and the
// app launches as part of establishing the WebDriver session — i.e. *before* any mocha
// `before`/`it` hook in the spec file gets to run. So the fixture repo has to exist on disk
// before the session starts, which means `onPrepare` (runs once, before any session) rather
// than a per-spec mocha `before` hook (runs after the session/app is already up).
function setupFixtureRepo(repoPath: string) {
  fs.rmSync(repoPath, { recursive: true, force: true });
  fs.mkdirSync(repoPath, { recursive: true });
  execFileSync("git", ["init"], { cwd: repoPath, stdio: "inherit" });
  // Local (not global) identity — GitHub-hosted runners ship no global git identity, and
  // `git_core::commit::commit`'s `repo.signature()` call errors without one (the same failure
  // mode `crates/git-core/tests/stage_commit.rs`'s
  // `commit_without_a_configured_identity_returns_an_error` test exists to cover). Setting it
  // locally here, rather than relying on the host's `~/.gitconfig`, also makes this fixture
  // hermetic for local runs. Matches the "Test User"/"test@example.com" convention from
  // `crates/git-core/tests/common/mod.rs`'s `init_repo()` helper.
  execFileSync("git", ["config", "user.name", "Test User"], { cwd: repoPath, stdio: "inherit" });
  execFileSync("git", ["config", "user.email", "test@example.com"], {
    cwd: repoPath,
    stdio: "inherit",
  });
  fs.writeFileSync(path.join(repoPath, "README.md"), "e2e fixture repo\n");
}

function resetFixtureRepo() {
  fs.rmSync(E2E_REPO_PATH, { recursive: true, force: true });
  execFileSync("git", ["clone", E2E_PARENT_SOURCE_PATH, E2E_REPO_PATH], { cwd: os.tmpdir(), stdio: "inherit" });
  execFileSync("git", ["config", "user.name", "Test User"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  execFileSync("git", ["config", "user.email", "test@example.com"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  execFileSync("git", ["config", "core.autocrlf", "false"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
}

function setupSubmoduleFixture(repoPath: string) {
  fs.rmSync(E2E_SUBMODULE_REPO_PATH, { recursive: true, force: true });
  fs.mkdirSync(E2E_SUBMODULE_REPO_PATH, { recursive: true });
  execFileSync("git", ["init"], { cwd: E2E_SUBMODULE_REPO_PATH, stdio: "inherit" });
  execFileSync("git", ["config", "user.name", "Test User"], {
    cwd: E2E_SUBMODULE_REPO_PATH,
    stdio: "inherit",
  });
  execFileSync("git", ["config", "user.email", "test@example.com"], {
    cwd: E2E_SUBMODULE_REPO_PATH,
    stdio: "inherit",
  });
  fs.writeFileSync(path.join(E2E_SUBMODULE_REPO_PATH, "README.md"), "e2e submodule\n");
  execFileSync("git", ["add", "."], { cwd: E2E_SUBMODULE_REPO_PATH, stdio: "inherit" });
  execFileSync("git", ["commit", "-m", "e2e: seed submodule"], {
    cwd: E2E_SUBMODULE_REPO_PATH,
    stdio: "inherit",
  });
  execFileSync("git", ["-c", "protocol.file.allow=always", "submodule", "add", E2E_SUBMODULE_REPO_PATH, E2E_SUBMODULE_PATH], { cwd: repoPath, stdio: "inherit" });
  execFileSync("git", ["add", "."], { cwd: repoPath, stdio: "inherit" });
  execFileSync("git", ["commit", "-m", "e2e: add submodule"], { cwd: repoPath, stdio: "inherit" });
}

function setupCredentialCertificate() {
  fs.rmSync(CREDENTIAL_CERT_DIR, { recursive: true, force: true });
  fs.mkdirSync(CREDENTIAL_CERT_DIR, { recursive: true });
  execFileSync("openssl", [
    "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", CREDENTIAL_KEY_PATH,
    "-out", CREDENTIAL_CERT_PATH, "-days", "1", "-subj", "/CN=localhost",
    "-addext", "subjectAltName=DNS:localhost",
  ], { stdio: "ignore" });
  // git2 initializes vendored OpenSSL from the process environment; repository-local
  // http.sslCAInfo is not consulted by that transport. This trusts only the ephemeral
  // loopback fixture certificate, without disabling TLS verification.
  process.env.SSL_CERT_FILE = CREDENTIAL_CERT_PATH;
  process.env.BROWSITORY_E2E_CREDENTIAL_KEY = CREDENTIAL_KEY_PATH;
  process.env.BROWSITORY_E2E_CREDENTIAL_CERT = CREDENTIAL_CERT_PATH;
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

// Poll until something is listening on 127.0.0.1:4444, rather than assuming `tauri-driver` is
// ready right after `spawn()` returns. WebdriverIO's own session-creation retries mask this
// most of the time, but a cold CI runner is slower than a local box, and a plausible flake
// class is cheap to remove outright: retry a raw TCP connect for a few seconds before letting
// `beforeSession` return (and WebdriverIO proceed to actually create the session).
function waitForPort(port: number, host: string, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      const socket = net.connect({ port, host }, () => {
        socket.end();
        resolve();
      });
      socket.on("error", () => {
        socket.destroy();
        if (Date.now() >= deadline) {
          reject(new Error(`Timed out waiting for ${host}:${port} to accept connections`));
          return;
        }
        setTimeout(attempt, 100);
      });
    };
    attempt();
  });
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
      // @ts-expect-error — WebdriverIO applies capability-level maxInstances at runtime.
      maxInstances: 1,
      // Capability-level maxInstances is required because this suite shares one fixture path.
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

  onPrepare: async () => {
    if (!fs.existsSync(tauriAppBinary)) {
      throw new Error(
        `tauri-app binary not found at ${tauriAppBinary}. Run \`cargo build --workspace\` ` +
          `from the repo root first (with frontend/dist already built with ` +
          `VITE_E2E_REPO_PATH=${E2E_REPO_PATH} baked in).`,
      );
    }
    setupFixtureRepo(E2E_PARENT_SOURCE_PATH);
    setupSubmoduleFixture(E2E_PARENT_SOURCE_PATH);
    resetFixtureRepo();
    setupCredentialCertificate();

    // A second, genuinely independent fixture repo (not a branch/clone of E2E_REPO_PATH) for
    // multi-repo.spec.ts, seeded with one real commit so it has a resolvable HEAD immediately.
    setupFixtureRepo(E2E_SECOND_REPO_PATH);
    fs.writeFileSync(path.join(E2E_SECOND_REPO_PATH, "second.txt"), "second repo\n");
    execFileSync("git", ["add", "second.txt"], { cwd: E2E_SECOND_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: second repo base commit"], { cwd: E2E_SECOND_REPO_PATH, stdio: "inherit" });

    // Seed a BROWSITORY_CONFIG_DIR the app will read/write for the whole suite run, so the
    // second repo is already in RepoPicker's recent-repos list (its "Open Folder" button drives
    // a native OS dialog WebDriver can't operate). Deliberately seeds only `recent_repos` here,
    // never `open_repos`/`active_repo` — App.tsx's E2E auto-open effect is written to force a
    // deterministic single starting repo regardless of what's persisted (see its comment), but
    // keeping this seed free of a persisted tab set is also the simplest way to guarantee every
    // spec (including this suite's very first launch) starts from the same known state.
    fs.rmSync(E2E_CONFIG_DIR, { recursive: true, force: true });
    fs.mkdirSync(E2E_CONFIG_DIR, { recursive: true });
    fs.writeFileSync(
      path.join(E2E_CONFIG_DIR, "config.toml"),
      `recent_repos = ["${E2E_SECOND_REPO_PATH.replace(/\\/g, "\\\\")}"]\n`,
    );
    process.env.BROWSITORY_CONFIG_DIR = E2E_CONFIG_DIR;

    // See `e2e/support/forgeFixtureServer.ts`'s header comment for why this has to be a real
    // loopback server started before the app process exists, rather than something the spec
    // file wires up per-test: the app only reads these env vars (via
    // `crates/tauri-app/src/pull_requests.rs`'s `github_api_base`/`bitbucket_api_base`) once,
    // implicitly, at process-environment-inheritance time when `tauri-driver` spawns it below.
    const forgeFixtureServer = await startSharedForgeFixtureServer();
    process.env.BROWSITORY_FORGE_GITHUB_API_BASE_URL = forgeFixtureServer.url;
    process.env.BROWSITORY_FORGE_BITBUCKET_API_BASE_URL = forgeFixtureServer.url;
  },

  // Ensure `tauri-driver` is running before the session starts so we can proxy the WebDriver
  // requests to it (127.0.0.1:4444, matching `hostname`/`port` above).
  beforeSession: async () => {
    const driverPath = path.resolve(os.homedir(), ".cargo", "bin", "tauri-driver");
    tauriDriver = spawn(driverPath, [], { stdio: [null, process.stdout, process.stderr] });
    resetFixtureRepo();
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
    await waitForPort(4444, "127.0.0.1", 10_000);
  },

  // Each spec prepares its fixture with direct git CLI calls in a Mocha `before` hook, but
  // the Tauri app opens the shared repo before that hook runs. Reload after fixture setup so
  // the app refetches status/history from disk before the test interacts with it.
  beforeTest: async () => {
    await browser.refresh();
  },

  afterSession: () => {
    closeTauriDriver();
  },

  onComplete: async () => {
    fs.rmSync(CREDENTIAL_CERT_DIR, { recursive: true, force: true });
    await closeSharedForgeFixtureServer();
  },
};
