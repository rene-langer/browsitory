// Runs inside the VSCode that `runVsixSmoke.ts` starts, with the packaged extension installed.
// Not a Mocha suite: one linear check that throws on the first failure.
import fs from "node:fs";
import path from "node:path";
import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

const EXTENSION_ID = "browsitory.browsitory";

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is not set`);
  return value;
}

function isInside(child: string, parent: string): boolean {
  const relative = path.relative(fs.realpathSync(parent), fs.realpathSync(child));
  return relative !== "" && !relative.startsWith("..") && !path.isAbsolute(relative);
}

// Linux only (the CI smoke job): the PIDs whose executable is `sidecarPath`.
function processesRunning(sidecarPath: string): string[] {
  const matches: string[] = [];
  for (const pid of fs.readdirSync("/proc").filter((name) => /^\d+$/.test(name))) {
    try {
      if (fs.readlinkSync(`/proc/${pid}/exe`) === sidecarPath) matches.push(pid);
    } catch {
      // Gone already, or not ours to inspect.
    }
  }
  return matches;
}

export async function run(): Promise<void> {
  const extensionsDir = requireEnv("BROWSITORY_VSIX_SMOKE_EXTENSIONS_DIR");
  const version = requireEnv("BROWSITORY_VSIX_SMOKE_VERSION");
  const commitMessage = requireEnv("BROWSITORY_VSIX_SMOKE_COMMIT");
  const cdpPort = requireEnv("BROWSITORY_VSCODE_E2E_CDP_PORT");

  const extension = vscode.extensions.getExtension(EXTENSION_ID);
  if (!extension) throw new Error(`${EXTENSION_ID} is not installed`);
  if (!isInside(extension.extensionPath, extensionsDir)) {
    throw new Error(`${EXTENSION_ID} loaded from ${extension.extensionPath}, not from ${extensionsDir}`);
  }
  const loadedVersion = (extension.packageJSON as { version?: unknown }).version;
  if (loadedVersion !== version) {
    throw new Error(`${EXTENSION_ID} version is ${String(loadedVersion)}, expected ${version}`);
  }

  await vscode.commands.executeCommand("browsitory.open");
  if (!extension.isActive) throw new Error(`${EXTENSION_ID} did not activate`);

  // The commit only shows up once the packaged webview loaded and the bundled sidecar opened
  // the repository and read its history.
  const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);
  try {
    await webview.waitForText("li", commitMessage, 30000);
  } finally {
    await webview.close();
  }

  if (process.platform === "linux") {
    const sidecarPath = path.join(fs.realpathSync(extension.extensionPath), "bin", "vscode-sidecar");
    if (processesRunning(sidecarPath).length === 0) {
      throw new Error(`no running process uses the packaged sidecar ${sidecarPath}`);
    }
  }
  console.log(`${EXTENSION_ID} ${version}: activated, sidecar running, repository history shown`);
}
