// Smoke test for a packaged .vsix (#271): installs the BUILT package into a downloaded VSCode
// with `code --install-extension`, using a throwaway extensions dir and user data dir, then
// starts that VSCode on a temp repository and runs `vsix-smoke/index.ts` inside it. That check
// proves the installed copy (not this checkout) activates, loads its own webview bundle, and
// starts its bundled sidecar, which opens the repository and lists its history.
//
//   pnpm --ignore-workspace smoke:vsix <path/to/browsitory-...-linux-x64.vsix>
//
// The E2E suite (`runTests.ts`) loads the extension from this checkout in development mode
// instead; neither one replaces the other.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  downloadAndUnzipVSCode,
  resolveCliArgsFromVSCodeExecutablePath,
  runTests,
} from "@vscode/test-electron";
import {
  getFreePort,
  killStaleDconfWatchers,
  PLATFORM_LAUNCH_ARGS,
  prepareLaunchEnvironment,
} from "./support/launch";

const EXTENSION_ID = "browsitory.browsitory";
const COMMIT_MESSAGE = "smoke: packaged extension sees this commit";

function createRepository(repoPath: string, gitEnv: NodeJS.ProcessEnv): void {
  fs.mkdirSync(repoPath, { recursive: true });
  const git = (...args: string[]) => execFileSync("git", args, { cwd: repoPath, env: gitEnv, stdio: "inherit" });
  git("init", "-q");
  fs.writeFileSync(path.join(repoPath, "README.md"), "vsix smoke fixture repo\n");
  git("add", "README.md");
  git(
    "-c",
    "user.name=Smoke Test",
    "-c",
    "user.email=smoke@example.com",
    "-c",
    "commit.gpgsign=false",
    "commit",
    "-q",
    "-m",
    COMMIT_MESSAGE,
  );
}

async function main(): Promise<void> {
  const vsixArgument = process.argv[2];
  if (!vsixArgument) throw new Error("usage: node out/runVsixSmoke.js <path/to/package.vsix>");
  const vsixPath = path.resolve(vsixArgument);
  if (!fs.existsSync(vsixPath)) throw new Error(`no such package: ${vsixPath}`);

  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "browsitory-vsix-smoke-")));
  const extensionsDir = path.join(root, "extensions");
  const userDataDir = path.join(root, "user-data");
  const configDir = path.join(root, "browsitory-config");
  const hostDir = path.join(root, "host");
  const repoPath = path.join(root, "repo");

  // Keeps the developer's git config, gpg agent and Browsitory config out of the test. The
  // same variables reach VSCode, the extension host and the sidecar it spawns.
  const isolatedEnv = {
    GIT_CONFIG_GLOBAL: path.join(root, "gitconfig"),
    GIT_CONFIG_NOSYSTEM: "1",
    GNUPGHOME: path.join(root, "gnupg"),
    BROWSITORY_CONFIG_DIR: configDir,
  };
  fs.writeFileSync(isolatedEnv.GIT_CONFIG_GLOBAL, "");
  fs.mkdirSync(isolatedEnv.GNUPGHOME, { mode: 0o700 });
  createRepository(repoPath, { ...process.env, ...isolatedEnv });

  // The sidecar reopens the repositories listed here when it starts, so the webview shows the
  // repository without the folder picker. JSON strings are valid TOML basic strings.
  fs.mkdirSync(configDir);
  fs.writeFileSync(
    path.join(configDir, "config.toml"),
    `active_repo = ${JSON.stringify(repoPath)}\n\n[[open_repos]]\npath = ${JSON.stringify(repoPath)}\n`,
  );

  // `runTests` needs a development extension to host the test runner. This empty one keeps the
  // real extension out of development mode, so VSCode loads the installed package.
  fs.mkdirSync(hostDir);
  fs.writeFileSync(
    path.join(hostDir, "package.json"),
    JSON.stringify({
      name: "browsitory-vsix-smoke-host",
      publisher: "browsitory-tests",
      version: "0.0.0",
      engines: { vscode: "*" },
    }),
  );

  const vscodeExecutablePath = await downloadAndUnzipVSCode();
  const [cli] = resolveCliArgsFromVSCodeExecutablePath(vscodeExecutablePath);
  if (!cli) throw new Error("could not resolve the VSCode CLI");
  const profileArgs = [`--extensions-dir=${extensionsDir}`, `--user-data-dir=${userDataDir}`];
  const runCli = (...args: string[]) =>
    execFileSync(cli, [...profileArgs, ...args], {
      encoding: "utf8",
      shell: process.platform === "win32",
    });

  console.log(runCli("--install-extension", vsixPath));
  const installed = runCli("--list-extensions", "--show-versions")
    .split(/\r?\n/)
    .find((line) => line.toLowerCase().startsWith(`${EXTENSION_ID}@`));
  if (!installed) throw new Error(`${EXTENSION_ID} is not listed after installing ${vsixPath}`);
  const version = installed.slice(EXTENSION_ID.length + 1);
  console.log(`installed ${EXTENSION_ID} ${version} into ${extensionsDir}`);

  killStaleDconfWatchers();
  prepareLaunchEnvironment();
  const cdpPort = await getFreePort();

  await runTests({
    vscodeExecutablePath,
    extensionDevelopmentPath: hostDir,
    extensionTestsPath: path.resolve(__dirname, "vsix-smoke", "index.js"),
    launchArgs: [
      repoPath,
      ...profileArgs,
      `--remote-debugging-port=${cdpPort}`,
      "--disable-workspace-trust",
      ...PLATFORM_LAUNCH_ARGS,
    ],
    extensionTestsEnv: {
      ...isolatedEnv,
      BROWSITORY_VSCODE_E2E_CDP_PORT: String(cdpPort),
      BROWSITORY_VSIX_SMOKE_EXTENSIONS_DIR: extensionsDir,
      BROWSITORY_VSIX_SMOKE_VERSION: version,
      BROWSITORY_VSIX_SMOKE_COMMIT: COMMIT_MESSAGE,
    },
  });

  fs.rmSync(root, { recursive: true, force: true });
  console.log("VSIX smoke test passed");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
