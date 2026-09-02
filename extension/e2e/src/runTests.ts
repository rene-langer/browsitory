import { execFileSync } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { runTests } from "@vscode/test-electron";

// Plain CommonJS `__dirname` (this package compiles to CommonJS — see package.json's note
// above) rather than the `fileURLToPath(import.meta.url)` ESM idiom.
const extensionDevelopmentPath = path.resolve(__dirname, "..", "..");
const extensionTestsPath = path.resolve(__dirname, "suite", "index.js");
const E2E_REPO_PATH =
  process.env["VSCODE_E2E_REPO_PATH"] ?? path.join(os.tmpdir(), "browsitory-vscode-e2e-repo");

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

// Best-effort: kill any `dconf watch` helper left over from a previous run of this harness.
// GIO's proxy resolver (see the env vars set in `main()` below) can spawn one of these, and if
// it inherited the previous run's `--remote-debugging-port` listening socket, it can outlive
// that run's Electron process and keep squatting the port, breaking the *next* run's own bind
// (`bind() failed: Address already in use`) even though nothing else is using it on purpose.
function killStaleDconfWatchers() {
  try {
    execFileSync("pkill", ["-9", "-f", "dconf watch"], { stdio: "ignore" });
  } catch {
    // pkill exits non-zero when it finds nothing to kill — that's the common case, not a
    // failure.
  }
}

// Picks an OS-assigned free TCP port by binding a throwaway server to port 0 and reading back
// what the kernel gave it, rather than trusting a hardcoded port number stays free between
// runs of this harness (see `killStaleDconfWatchers` above for why a fixed port can end up
// stuck).
function getFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (address === null || typeof address === "string") {
        server.close();
        reject(new Error("could not determine assigned port"));
        return;
      }
      const { port } = address;
      server.close(() => resolve(port));
    });
  });
}

async function main() {
  setupFixtureRepo();
  killStaleDconfWatchers();

  // VSCode/Electron's GTK proxy-resolver machinery (GIO) can spawn a `dconf watch
  // /system/proxy/` helper process that ends up sharing the `--remote-debugging-port` listening
  // socket (inherited via fork without CLOEXEC), which leaves the CDP HTTP endpoint
  // unresponsive to any client — not just Playwright. `GIO_USE_PROXY_RESOLVER=dummy` is the
  // targeted fix: it tells GIO not to use the dconf-backed proxy resolver at all, so no such
  // helper is spawned in the first place. `GSETTINGS_BACKEND=memory` is kept alongside it as a
  // second layer (skips the dconf-backed settings store generally) — belt and suspenders, since
  // neither alone was confirmed sufficient in every environment this was tested in.
  process.env["GIO_USE_PROXY_RESOLVER"] = "dummy";
  process.env["GSETTINGS_BACKEND"] = "memory";

  const cdpPort = await getFreePort();
  process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"] = String(cdpPort);
  await runTests({
    extensionDevelopmentPath,
    extensionTestsPath,
    launchArgs: [
      E2E_REPO_PATH,
      `--remote-debugging-port=${cdpPort}`,
      "--disable-workspace-trust",
    ],
  });
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
