import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { runTests } from "@vscode/test-electron";

// Plain CommonJS `__dirname` (this package compiles to CommonJS — see package.json's note
// above) rather than the `fileURLToPath(import.meta.url)` ESM idiom.
const extensionDevelopmentPath = path.resolve(__dirname, "..", "..");
const extensionTestsPath = path.resolve(__dirname, "suite", "index.js");
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-vscode-e2e-repo");
const CDP_PORT = 9229;

function setupFixtureRepo() {
  fs.rmSync(E2E_REPO_PATH, { recursive: true, force: true });
  fs.mkdirSync(E2E_REPO_PATH, { recursive: true });
  execFileSync("git", ["init"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  execFileSync("git", ["config", "user.name", "Test User"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  execFileSync("git", ["config", "user.email", "test@example.com"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  fs.writeFileSync(path.join(E2E_REPO_PATH, "README.md"), "vscode e2e fixture repo\n");
  execFileSync("git", ["add", "README.md"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  execFileSync("git", ["commit", "-m", "e2e: base commit"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
}

async function main() {
  setupFixtureRepo();
  // Without this, VSCode/Electron's GTK proxy-resolver helper (`dconf watch /system/proxy/`)
  // can end up holding the `--remote-debugging-port` listening socket (inherited via fork
  // without CLOEXEC), which leaves the CDP HTTP endpoint unresponsive to any client — not just
  // Playwright. Forcing the in-memory GSettings backend skips that dconf watch entirely and
  // makes the port bind cleanly every time.
  process.env["GSETTINGS_BACKEND"] = "memory";
  process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"] = String(CDP_PORT);
  await runTests({
    extensionDevelopmentPath,
    extensionTestsPath,
    launchArgs: [
      E2E_REPO_PATH,
      `--remote-debugging-port=${CDP_PORT}`,
      "--disable-workspace-trust",
    ],
  });
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
