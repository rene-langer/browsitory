import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { crc32, deflateRawSync } from "node:zlib";

import { checkVsix, parseOptions, readZipEntries, runCheck } from "./check-vsix.mjs";

// A minimal zip writer (deflate, Unix modes) so the tests build real archives for the reader.
function writeZip(files) {
  const locals = [];
  const centrals = [];
  let offset = 0;
  for (const { name, data, mode = 0o100644 } of files) {
    const nameBytes = Buffer.from(name, "utf8");
    const content = Buffer.isBuffer(data) ? data : Buffer.from(data, "utf8");
    const compressed = deflateRawSync(content);
    const checksum = crc32(content);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(8, 8);
    local.writeUInt32LE(checksum, 14);
    local.writeUInt32LE(compressed.length, 18);
    local.writeUInt32LE(content.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE((3 << 8) | 20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(8, 10);
    central.writeUInt32LE(checksum, 16);
    central.writeUInt32LE(compressed.length, 20);
    central.writeUInt32LE(content.length, 24);
    central.writeUInt16LE(nameBytes.length, 28);
    central.writeUInt32LE((mode << 16) >>> 0, 38);
    central.writeUInt32LE(offset, 42);
    locals.push(local, nameBytes, compressed);
    centrals.push(central, nameBytes);
    offset += local.length + nameBytes.length + compressed.length;
  }
  const directory = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(directory.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, directory, end]);
}

function elfX64() {
  const header = Buffer.alloc(64);
  Buffer.from([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1]).copy(header);
  header.writeUInt16LE(0x3e, 18);
  return header;
}

function machO(cpuType) {
  const header = Buffer.alloc(32);
  header.writeUInt32LE(0xfeedfacf, 0);
  header.writeUInt32LE(cpuType, 4);
  return header;
}

function peX64() {
  const header = Buffer.alloc(0x90);
  header.write("MZ", 0, "latin1");
  header.writeUInt32LE(0x80, 0x3c);
  header.write("PE\0\0", 0x80, "latin1");
  header.writeUInt16LE(0x8664, 0x84);
  return header;
}

const SIDECARS = {
  "linux-x64": { name: "vscode-sidecar", data: elfX64() },
  "darwin-x64": { name: "vscode-sidecar", data: machO(0x01000007) },
  "darwin-arm64": { name: "vscode-sidecar", data: machO(0x0100000c) },
  "win32-x64": { name: "vscode-sidecar.exe", data: peX64() },
};

function manifestXml({ version, target, preRelease }) {
  return `<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011">
  <Metadata>
    <Identity Language="en-US" Id="browsitory" Version="${version}" Publisher="browsitory" TargetPlatform="${target}"/>
    <Properties>
      <Property Id="Microsoft.VisualStudio.Services.Links.Source" Value="https://github.com/rene-langer/browsitory.git" />
      ${preRelease ? '<Property Id="Microsoft.VisualStudio.Code.PreRelease" Value="true" />' : ""}
    </Properties>
    <Icon>extension/icon.png</Icon>
  </Metadata>
</PackageManifest>`;
}

function packageFiles({ version = "1.2.3", target = "linux-x64", preRelease = true } = {}) {
  const sidecar = SIDECARS[target];
  return [
    { name: "extension.vsixmanifest", data: manifestXml({ version, target, preRelease }) },
    { name: "[Content_Types].xml", data: "<Types/>" },
    {
      name: "extension/package.json",
      data: JSON.stringify({
        name: "browsitory",
        publisher: "browsitory",
        version,
        icon: "icon.png",
        license: "MIT",
        main: "./dist/extension.js",
        repository: { type: "git", url: "https://github.com/rene-langer/browsitory", directory: "extension" },
      }),
    },
    { name: "extension/dist/extension.js", data: "exports.activate = () => {};" },
    { name: "extension/dist/sidecarBridge.js", data: "" },
    { name: "extension/webview/index.html", data: "<html></html>" },
    { name: "extension/webview/favicon.svg", data: "<svg/>" },
    { name: "extension/webview/assets/vscode-main.js", data: "main();" },
    { name: "extension/webview/assets/vscode-main.css", data: "body{}" },
    { name: `extension/bin/${sidecar.name}`, data: sidecar.data, mode: 0o100755 },
    { name: "extension/icon.png", data: Buffer.from([0x89, 0x50, 0x4e, 0x47]) },
    { name: "extension/readme.md", data: "# Browsitory" },
    { name: "extension/changelog.md", data: "# Changelog" },
    { name: "extension/LICENSE.txt", data: "MIT" },
    { name: "extension/THIRD_PARTY_NOTICES.md", data: "# Notices\n" },
  ];
}

function check(files, expected = {}) {
  return checkVsix({
    entries: readZipEntries(writeZip(files)),
    target: "linux-x64",
    version: "1.2.3",
    preRelease: true,
    ...expected,
  });
}

test("reads names, Unix modes and contents back from a zip", () => {
  const entries = readZipEntries(
    writeZip([
      { name: "a.txt", data: "alpha" },
      { name: "bin/tool", data: "beta", mode: 0o100755 },
    ]),
  );
  assert.deepEqual(
    entries.map((entry) => [entry.name, entry.mode, entry.read().toString("utf8")]),
    [
      ["a.txt", 0o100644, "alpha"],
      ["bin/tool", 0o100755, "beta"],
    ],
  );
});

test("accepts a well-formed package for every target", () => {
  for (const target of Object.keys(SIDECARS)) {
    assert.deepEqual(check(packageFiles({ target }), { target }), [], target);
  }
});

test("accepts a stable package when no pre-release is expected", () => {
  assert.deepEqual(check(packageFiles({ preRelease: false }), { preRelease: false }), []);
});

test("accepts any notices file content, but not a missing or empty one", () => {
  const regenerated = packageFiles().map((file) =>
    file.name === "extension/THIRD_PARTY_NOTICES.md"
      ? { ...file, data: "Generated notices\n\n".repeat(500) }
      : file,
  );
  assert.deepEqual(check(regenerated), []);

  const missing = packageFiles().filter((file) => file.name !== "extension/THIRD_PARTY_NOTICES.md");
  assert.match(check(missing).join("\n"), /missing extension\/THIRD_PARTY_NOTICES\.md/);

  const empty = packageFiles().map((file) =>
    file.name === "extension/THIRD_PARTY_NOTICES.md" ? { ...file, data: "" } : file,
  );
  assert.match(check(empty).join("\n"), /extension\/THIRD_PARTY_NOTICES\.md is empty/);
});

test("reports source maps, tests, sources and other stray files", () => {
  const problems = check([
    ...packageFiles(),
    { name: "extension/dist/extension.js.map", data: "{}" },
    { name: "extension/dist/extension.test.js", data: "" },
    { name: "extension/src/extension.ts", data: "" },
    { name: "extension/e2e/package.json", data: "{}" },
    { name: "extension/notes.txt", data: "" },
  ]).join("\n");
  for (const name of [
    "extension/dist/extension.js.map",
    "extension/dist/extension.test.js",
    "extension/src/extension.ts",
    "extension/e2e/package.json",
    "extension/notes.txt",
  ]) {
    assert.match(problems, new RegExp(`unexpected file ${name.replace(/[.]/g, "\\.")}`));
  }
});

test("reports a wrong version, target, identity or pre-release flag", () => {
  const problems = check(packageFiles({ version: "0.0.1", preRelease: false })).join("\n");
  assert.match(problems, /manifest Version is "0\.0\.1", expected "1\.2\.3"/);
  assert.match(problems, /package\.json version is "0\.0\.1", expected "1\.2\.3"/);
  assert.match(problems, /not marked as a pre-release/);

  assert.match(
    check(packageFiles({ preRelease: true }), { preRelease: false }).join("\n"),
    /marked as a pre-release/,
  );
  assert.match(
    check(packageFiles({ target: "linux-x64" }), { target: "win32-x64" }).join("\n"),
    /manifest TargetPlatform is "linux-x64", expected "win32-x64"/,
  );

  const renamed = packageFiles().map((file) =>
    file.name === "extension/package.json"
      ? { ...file, data: JSON.stringify({ ...JSON.parse(file.data), publisher: "someone", icon: null, repository: null }) }
      : file,
  );
  const renamedProblems = check(renamed).join("\n");
  assert.match(renamedProblems, /package\.json publisher is "someone", expected "browsitory"/);
  assert.match(renamedProblems, /package\.json icon is null/);
  assert.match(renamedProblems, /package\.json repository/);
});

test("reports a missing icon file", () => {
  const files = packageFiles().filter((file) => file.name !== "extension/icon.png");
  assert.match(check(files).join("\n"), /missing extension\/icon\.png/);
});

test("reports a missing, extra, foreign or non-executable sidecar", () => {
  const withoutSidecar = packageFiles().filter((file) => !file.name.startsWith("extension/bin/"));
  assert.match(check(withoutSidecar).join("\n"), /missing extension\/bin\/vscode-sidecar/);

  const extra = [...packageFiles(), { name: "extension/bin/vscode-sidecar.exe", data: peX64() }];
  assert.match(check(extra).join("\n"), /unexpected file extension\/bin\/vscode-sidecar\.exe/);

  const intelInArm = packageFiles({ target: "darwin-arm64" }).map((file) =>
    file.name === "extension/bin/vscode-sidecar" ? { ...file, data: machO(0x01000007) } : file,
  );
  assert.match(
    check(intelInArm, { target: "darwin-arm64" }).join("\n"),
    /sidecar is not a Mach-O arm64 binary/,
  );

  const notExecutable = packageFiles().map((file) =>
    file.name === "extension/bin/vscode-sidecar" ? { ...file, mode: 0o100644 } : file,
  );
  assert.match(check(notExecutable).join("\n"), /sidecar is not executable/);
});

test("the command line reads the package and exits non-zero on a problem", async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), "browsitory-check-vsix-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const good = path.join(root, "good.vsix");
  const bad = path.join(root, "bad.vsix");
  await writeFile(good, writeZip(packageFiles()));
  await writeFile(bad, writeZip([...packageFiles(), { name: "extension/dist/extension.js.map", data: "{}" }]));
  const lines = [];
  const log = (line) => lines.push(line);

  const options = parseOptions(["--", "--target", "linux-x64", "--version", "1.2.3", "--pre-release", good]);
  assert.deepEqual(options, { target: "linux-x64", version: "1.2.3", preRelease: true, vsixPath: good });
  assert.equal(await runCheck(options, log), true);
  assert.equal(await runCheck({ ...options, vsixPath: bad }, log), false);
  assert.match(lines.join("\n"), /unexpected file extension\/dist\/extension\.js\.map/);
});

test("the command line requires a target, a version and a package", () => {
  assert.throws(() => parseOptions(["--version", "1.2.3", "x.vsix"]), /--target/);
  assert.throws(() => parseOptions(["--target", "linux-x64", "x.vsix"]), /--version/);
  assert.throws(() => parseOptions(["--target", "linux-x64", "--version", "1.2.3"]), /package path/);
  assert.throws(() => parseOptions(["--target", "linux-arm64", "--version", "1.2.3", "x.vsix"]), /unsupported/);
});
