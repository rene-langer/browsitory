import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { assertClassicScriptBundle, packageVsix, parseOptions } from "./package-vsix.mjs";

test("stages only the selected sidecar and cleans generated assets", async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), "browsitory-vsix-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const extensionRoot = path.join(root, "extension");
  const frontendDist = path.join(root, "frontend", "dist-vscode");
  const sidecarPath = path.join(root, "vscode-sidecar");
  const outputPath = path.join(root, "browsitory-linux-x64.vsix");
  await mkdir(path.join(frontendDist, "assets"), { recursive: true });
  await writeFile(path.join(frontendDist, "assets", "vscode-main.js"), "webview");
  await writeFile(sidecarPath, "sidecar");
  const calls = [];

  await packageVsix({
    target: "linux-x64", extensionRoot, frontendDist, sidecarPath, outputPath,
    run: async (...args) => {
      calls.push(args);
      assert.equal(await readFile(path.join(extensionRoot, "bin", "vscode-sidecar"), "utf8"), "sidecar");
      assert.equal(await readFile(path.join(extensionRoot, "webview", "assets", "vscode-main.js"), "utf8"), "webview");
    },
  });

  assert.deepEqual(calls, [["pnpm", ["exec", "vsce", "package", "--target", "linux-x64", "--out", outputPath], extensionRoot]]);
  await assert.rejects(readFile(path.join(extensionRoot, "bin", "vscode-sidecar")), { code: "ENOENT" });
  await assert.rejects(readFile(path.join(extensionRoot, "webview", "assets", "vscode-main.js")), { code: "ENOENT" });
});

test("packages a pre-release when asked", async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), "browsitory-vsix-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const frontendDist = path.join(root, "frontend", "dist-vscode");
  const sidecarPath = path.join(root, "vscode-sidecar");
  const outputPath = path.join(root, "browsitory-win32-x64.vsix");
  await mkdir(path.join(frontendDist, "assets"), { recursive: true });
  await writeFile(path.join(frontendDist, "assets", "vscode-main.js"), "webview");
  await writeFile(sidecarPath, "sidecar");
  const calls = [];

  await packageVsix({
    target: "win32-x64", extensionRoot: path.join(root, "extension"), frontendDist, sidecarPath, outputPath,
    preRelease: true,
    run: async (...args) => calls.push(args),
  });

  assert.deepEqual(calls[0][1], ["exec", "vsce", "package", "--target", "win32-x64", "--out", outputPath, "--pre-release"]);
});

test("rejects unsupported targets before staging", async () => {
  await assert.rejects(packageVsix({ target: "linux-arm64" }), /unsupported VSIX target/);
});

test("accepts pnpm's argument separator", () => {
  const options = parseOptions([
    "--", "--target", "linux-x64", "--sidecar", "/tmp/vscode-sidecar", "--out", "/tmp/browsitory.vsix",
  ]);

  assert.equal(options.target, "linux-x64");
  assert.equal(options.sidecarPath, "/tmp/vscode-sidecar");
  assert.equal(options.outputPath, "/tmp/browsitory.vsix");
  assert.equal(options.preRelease, false);
});

test("reads the --pre-release flag anywhere in the arguments", () => {
  const options = parseOptions([
    "--", "--target", "darwin-arm64", "--pre-release", "--sidecar", "/tmp/vscode-sidecar", "--out", "/tmp/b.vsix",
  ]);

  assert.equal(options.preRelease, true);
  assert.equal(options.target, "darwin-arm64");
  assert.equal(options.sidecarPath, "/tmp/vscode-sidecar");
  assert.equal(options.outputPath, "/tmp/b.vsix");
});

test("rejects a webview bundle with import.meta or extra chunks", async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), "browsitory-bundle-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const assets = path.join(root, "assets");
  await mkdir(assets, { recursive: true });
  await writeFile(path.join(assets, "vscode-main.js"), "ok()");
  await assertClassicScriptBundle(root);

  await writeFile(path.join(assets, "vscode-main.js"), "u=import.meta.url");
  await assert.rejects(assertClassicScriptBundle(root), /import\.meta/);

  await writeFile(path.join(assets, "vscode-main.js"), "ok()");
  await writeFile(path.join(assets, "rust.js"), "x");
  await assert.rejects(assertClassicScriptBundle(root), /extra files: rust\.js/);
});

test("manifest ships the listing page, icon and licence notices", async () => {
  const extensionRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const manifest = JSON.parse(await readFile(path.join(extensionRoot, "package.json"), "utf8"));

  assert.equal(manifest.icon, "icon.png");
  assert.equal(manifest.repository?.url, "https://github.com/rene-langer/browsitory");
  assert.equal(manifest.repository?.directory, "extension");
  for (const file of ["icon.png", "README.md", "CHANGELOG.md", "LICENSE", "THIRD_PARTY_NOTICES.md"]) {
    assert.ok(manifest.files.includes(file), `package.json "files" must list ${file}`);
    await readFile(path.join(extensionRoot, file));
  }
  // PNG IHDR: width and height are big-endian u32s at byte 16 and 20.
  const icon = await readFile(path.join(extensionRoot, "icon.png"));
  assert.equal(icon.readUInt32BE(16), 128, "icon width");
  assert.equal(icon.readUInt32BE(20), 128, "icon height");
  const notices = await readFile(path.join(extensionRoot, "THIRD_PARTY_NOTICES.md"), "utf8");
  assert.match(notices, /libgit2/);
  assert.match(notices, /linking exception/i);
});
