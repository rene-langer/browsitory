import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

import { packageVsix, parseOptions } from "./package-vsix.mjs";

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
});
