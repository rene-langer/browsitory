import { execFileSync } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";

// Shared by the E2E suite (`runTests.ts`) and the packaged-VSIX smoke test (`runVsixSmoke.ts`).

// Best-effort: kill any `dconf watch` helper left over from a previous run of this harness.
// GIO's proxy resolver (see `prepareLaunchEnvironment` below) can spawn one of these, and if
// it inherited the previous run's `--remote-debugging-port` listening socket, it can outlive
// that run's Electron process and keep squatting the port, breaking the *next* run's own bind
// (`bind() failed: Address already in use`) even though nothing else is using it on purpose.
export function killStaleDconfWatchers(): void {
  try {
    execFileSync("pkill", ["-9", "-f", "dconf watch"], { stdio: "ignore" });
  } catch {
    // pkill exits non-zero when it finds nothing to kill — that's the common case, not a
    // failure.
  }
}

// VSCode/Electron's GTK proxy-resolver machinery (GIO) can spawn a `dconf watch
// /system/proxy/` helper process that ends up sharing the `--remote-debugging-port` listening
// socket (inherited via fork without CLOEXEC), which leaves the CDP HTTP endpoint
// unresponsive to any client — not just Playwright. `GIO_USE_PROXY_RESOLVER=dummy` is the
// targeted fix: it tells GIO not to use the dconf-backed proxy resolver at all, so no such
// helper is spawned in the first place. `GSETTINGS_BACKEND=memory` is kept alongside it as a
// second layer (skips the dconf-backed settings store generally) — belt and suspenders, since
// neither alone was confirmed sufficient in every environment this was tested in.
export function prepareLaunchEnvironment(): void {
  process.env["GIO_USE_PROXY_RESOLVER"] = "dummy";
  process.env["GSETTINGS_BACKEND"] = "memory";
}

// Issue #100: on a Wayland desktop, `xvfb-run` only swaps DISPLAY and Electron's ozone
// auto-detection still picks the developer's real Wayland session, so the window opened
// there, unfocused/occluded, and the compositor sent it no frames — the webview's
// IntersectionObserver never fired and diff lines never rendered. Pin it to X11 (the Xvfb
// display); a no-op on CI, which has no Wayland session.
export const PLATFORM_LAUNCH_ARGS: string[] = process.platform === "linux" ? ["--ozone-platform=x11"] : [];

// Picks an OS-assigned free TCP port by binding a throwaway server to port 0 and reading back
// what the kernel gave it, rather than trusting a hardcoded port number stays free between
// runs of this harness (see `killStaleDconfWatchers` above for why a fixed port can end up
// stuck).
export function getFreePort(): Promise<number> {
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

// Issue #546: gives the E2E host its own, empty Browsitory config dir, as the Tauri suite's
// `wdio.conf.ts` does, so a spec can set app-wide preferences (`[fetch] auto_fetch_minutes`) and
// the sidecar never reads or writes the developer's real `config.toml` (recent repositories,
// open tabs). VSCode, its extension host and the sidecar inherit it from this process.
export function prepareConfigDir(): string {
  const configDir = path.join(os.tmpdir(), "browsitory-vscode-e2e-config");
  fs.rmSync(configDir, { recursive: true, force: true });
  fs.mkdirSync(configDir, { recursive: true });
  process.env["BROWSITORY_CONFIG_DIR"] = configDir;
  return configDir;
}
