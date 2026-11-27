import { execFileSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { runTests } from "@vscode/test-electron";
import { acquireFixtureLock } from "./support/fixtureLock";
import { initFixtureRepo } from "./support/fixtureRepo";
import { isolateHome } from "./support/isolatedHome";
import {
  getFreePort,
  killStaleDconfWatchers,
  PLATFORM_LAUNCH_ARGS,
  prepareConfigDir,
  prepareLaunchEnvironment,
} from "./support/launch";
import { downloadVSCode } from "./support/vscodeDownload";

// Plain CommonJS `__dirname` (this package compiles to CommonJS — see package.json's note
// above) rather than the `fileURLToPath(import.meta.url)` ESM idiom.
const extensionDevelopmentPath = path.resolve(__dirname, "..", "..");
const extensionTestsPath = path.resolve(__dirname, "suite", "index.js");
const E2E_REPO_PATH =
  process.env["VSCODE_E2E_REPO_PATH"] ?? path.join(os.tmpdir(), "browsitory-vscode-e2e-repo");

function setupFixtureRepo() {
  initFixtureRepo(E2E_REPO_PATH, "vscode e2e fixture repo\n");
  execFileSync("git", ["add", "README.md"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  execFileSync("git", ["commit", "-m", "e2e: base commit"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
}

async function main() {
  // Before anything is deleted (#1144): a second run stops here instead of wiping the fixtures
  // and the config dir of a run going on in another worktree.
  const releaseLock = acquireFixtureLock(
    path.join(os.tmpdir(), "browsitory-vscode-e2e.lock"),
    "Wait for it to finish, or give this run a TMPDIR of its own (see AGENTS.md).",
  );
  process.on("exit", releaseLock);
  // VSCode, the sidecar and the fixture git commands get a throwaway HOME (#1112).
  isolateHome(path.join(os.tmpdir(), "browsitory-vscode-e2e-home"));
  setupFixtureRepo();
  killStaleDconfWatchers();

  prepareLaunchEnvironment();
  prepareConfigDir();

  const cdpPort = await getFreePort();
  process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"] = String(cdpPort);
  const vscodeExecutablePath = await downloadVSCode(extensionDevelopmentPath);
  await runTests({
    vscodeExecutablePath,
    extensionDevelopmentPath,
    extensionTestsPath,
    launchArgs: [
      E2E_REPO_PATH,
      `--remote-debugging-port=${cdpPort}`,
      "--disable-workspace-trust",
      ...PLATFORM_LAUNCH_ARGS,
    ],
  });
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
