import { spawn } from "node:child_process";
import { cp, copyFile, mkdir, readdir, readFile, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const supportedTargets = new Set([
  "linux-x64",
  "darwin-x64",
  "darwin-arm64",
  "win32-x64",
]);

// The webview loads the bundle as a classic <script> under a nonce-only CSP: `import.meta` is a
// SyntaxError there and lazy chunks/wasm would be blocked. frontend/vite.vscode.config.ts inlines
// everything; this fails the package step if that regresses.
export async function assertClassicScriptBundle(frontendDist) {
  const assets = path.join(frontendDist, "assets");
  const extra = (await readdir(assets)).filter(
    (name) => (name.endsWith(".js") && name !== "vscode-main.js") || name.endsWith(".wasm"),
  );
  if (extra.length > 0) {
    throw new Error(`webview bundle must be a single script; found extra files: ${extra.join(", ")}`);
  }
  const main = await readFile(path.join(assets, "vscode-main.js"), "utf8");
  if (main.includes("import.meta")) {
    throw new Error("webview bundle must not contain import.meta (classic script)");
  }
}

export async function packageVsix({
  target,
  extensionRoot,
  frontendDist,
  sidecarPath,
  outputPath,
  run = runCommand,
}) {
  if (!supportedTargets.has(target)) {
    throw new Error(`unsupported VSIX target: ${target}`);
  }
  for (const [name, value] of Object.entries({ extensionRoot, frontendDist, sidecarPath, outputPath })) {
    if (typeof value !== "string" || value.length === 0) {
      throw new Error(`${name} is required`);
    }
  }

  await assertClassicScriptBundle(frontendDist);
  const binDirectory = path.join(extensionRoot, "bin");
  const webviewDirectory = path.join(extensionRoot, "webview");
  const sidecarName = target === "win32-x64" ? "vscode-sidecar.exe" : "vscode-sidecar";
  try {
    await Promise.all([
      rm(binDirectory, { recursive: true, force: true }),
      rm(webviewDirectory, { recursive: true, force: true }),
    ]);
    await mkdir(binDirectory, { recursive: true });
    await mkdir(path.dirname(outputPath), { recursive: true });
    await cp(frontendDist, webviewDirectory, { recursive: true });
    await copyFile(sidecarPath, path.join(binDirectory, sidecarName));
    await run(
      "pnpm",
      ["exec", "vsce", "package", "--target", target, "--out", outputPath],
      extensionRoot,
    );
    return outputPath;
  } finally {
    await Promise.all([
      rm(binDirectory, { recursive: true, force: true }),
      rm(webviewDirectory, { recursive: true, force: true }),
    ]);
  }
}

function runCommand(command, arguments_, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, arguments_, { cwd, stdio: "inherit", shell: process.platform === "win32" });
    child.once("error", reject);
    child.once("exit", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} exited with status ${code ?? "unknown"}`));
    });
  });
}

export function parseOptions(args) {
  const argumentList = args[0] === "--" ? args.slice(1) : args;
  const values = new Map();
  for (let index = 0; index < argumentList.length; index += 2) {
    values.set(argumentList[index], argumentList[index + 1]);
  }
  const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
  const extensionRoot = path.resolve(scriptDirectory, "..");
  const sidecarPath = values.get("--sidecar");
  return {
    target: values.get("--target"),
    sidecarPath: sidecarPath && path.resolve(extensionRoot, "..", sidecarPath),
    outputPath: values.get("--out"),
    extensionRoot,
    frontendDist: path.resolve(extensionRoot, "..", "frontend", "dist-vscode"),
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  packageVsix(parseOptions(process.argv.slice(2))).catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
