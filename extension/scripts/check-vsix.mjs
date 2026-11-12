// Checks a packaged .vsix before anyone publishes it (#271): the identity and version the
// registries will see, the pre-release flag, the listing metadata (icon, repository), and the
// exact file list, including that the bundled sidecar is an executable for the package's target.
//
// It reads the .vsix itself rather than `vsce ls`: `vsce ls` lists the extension folder, and
// package-vsix.mjs stages the sidecar and webview there only while `vsce package` runs.
//
//   node scripts/check-vsix.mjs --target linux-x64 --version 0.1.0 [--pre-release] <file.vsix>
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { inflateRawSync } from "node:zlib";

const PUBLISHER = "browsitory";
const NAME = "browsitory";
const REPOSITORY = "https://github.com/rene-langer/browsitory";

// What the sidecar must be for each target: file name, and a check of its executable header.
const SIDECARS = {
  "linux-x64": { file: "vscode-sidecar", format: "an ELF x86-64 binary", matches: isElfX64 },
  "darwin-x64": { file: "vscode-sidecar", format: "a Mach-O x86-64 binary", matches: (b) => isMachO(b, 0x01000007) },
  "darwin-arm64": { file: "vscode-sidecar", format: "a Mach-O arm64 binary", matches: (b) => isMachO(b, 0x0100000c) },
  "win32-x64": { file: "vscode-sidecar.exe", format: "a PE x86-64 binary", matches: isPeX64 },
};

// Every file the package may contain. Anything else (source maps, tests, sources, stray files)
// is reported. The notices file is checked for presence only, so a regenerated one passes.
const ALLOWED_FILES = [
  /^extension\.vsixmanifest$/,
  /^\[Content_Types\]\.xml$/,
  /^extension\/package\.json$/,
  /^extension\/dist\/(?:[\w.-]+\/)*[\w.-]+\.js$/,
  /^extension\/webview\/(?:[\w.-]+\/)*[\w.-]+\.(?:html|js|css|svg|png|woff2?|ttf)$/,
  /^extension\/icon\.png$/,
  /^extension\/readme\.md$/i,
  /^extension\/changelog\.md$/i,
  /^extension\/license(?:\.txt|\.md)?$/i,
  /^extension\/THIRD_PARTY_NOTICES\.md$/,
  // The same notices, which the webview's "Third-party licences" dialog fetches.
  /^extension\/webview\/third-party-notices\.md$/,
];
const FORBIDDEN_FILES = [/\.map$/, /\.test\.[cm]?js$/, /\.spec\.[cm]?js$/, /\.ts$/, /(?:^|\/)node_modules\//];

const REQUIRED_NON_EMPTY = [
  "extension/package.json",
  "extension/dist/extension.js",
  "extension/webview/assets/vscode-main.js",
  "extension/webview/assets/vscode-main.css",
  // The syntax highlighting worker (#843), fetched by the webview and started from a blob.
  "extension/webview/assets/highlight-worker.js",
  "extension/icon.png",
  "extension/THIRD_PARTY_NOTICES.md",
  "extension/webview/third-party-notices.md",
];

/** Reads a zip's central directory: name, Unix mode (or null) and a lazy reader per file. */
export function readZipEntries(buffer) {
  const end = findEndOfCentralDirectory(buffer);
  const count = buffer.readUInt16LE(end + 10);
  let offset = buffer.readUInt32LE(end + 16);
  const entries = [];
  for (let index = 0; index < count; index += 1) {
    if (buffer.readUInt32LE(offset) !== 0x02014b50) throw new Error("corrupt zip central directory");
    const madeBy = buffer.readUInt16LE(offset + 4) >> 8;
    const method = buffer.readUInt16LE(offset + 10);
    const compressedSize = buffer.readUInt32LE(offset + 20);
    const nameLength = buffer.readUInt16LE(offset + 28);
    const extraLength = buffer.readUInt16LE(offset + 30);
    const commentLength = buffer.readUInt16LE(offset + 32);
    const attributes = buffer.readUInt32LE(offset + 38);
    const localOffset = buffer.readUInt32LE(offset + 42);
    const name = buffer.toString("utf8", offset + 46, offset + 46 + nameLength);
    entries.push({
      name,
      mode: madeBy === 3 ? attributes >>> 16 : null,
      read: () => {
        const dataStart =
          localOffset + 30 + buffer.readUInt16LE(localOffset + 26) + buffer.readUInt16LE(localOffset + 28);
        const data = buffer.subarray(dataStart, dataStart + compressedSize);
        if (method === 0) return data;
        if (method === 8) return inflateRawSync(data);
        throw new Error(`${name}: unsupported zip compression method ${method}`);
      },
    });
    offset += 46 + nameLength + extraLength + commentLength;
  }
  return entries;
}

function findEndOfCentralDirectory(buffer) {
  for (let offset = buffer.length - 22; offset >= Math.max(0, buffer.length - 22 - 0xffff); offset -= 1) {
    if (buffer.readUInt32LE(offset) === 0x06054b50) return offset;
  }
  throw new Error("not a zip file (no end of central directory)");
}

function isElfX64(bytes) {
  return bytes.length >= 20 && bytes.readUInt32BE(0) === 0x7f454c46 && bytes[4] === 2 && bytes.readUInt16LE(18) === 0x3e;
}

function isMachO(bytes, cpuType) {
  return bytes.length >= 8 && bytes.readUInt32LE(0) === 0xfeedfacf && bytes.readUInt32LE(4) === cpuType;
}

function isPeX64(bytes) {
  if (bytes.length < 0x40 || bytes.toString("latin1", 0, 2) !== "MZ") return false;
  const pe = bytes.readUInt32LE(0x3c);
  return (
    bytes.length >= pe + 6 && bytes.toString("latin1", pe, pe + 4) === "PE\0\0" && bytes.readUInt16LE(pe + 4) === 0x8664
  );
}

function attribute(element, name) {
  return new RegExp(`\\b${name}="([^"]*)"`).exec(element)?.[1] ?? null;
}

function quoted(value) {
  return value === undefined ? "missing" : JSON.stringify(value);
}

/** Returns the list of problems with a package; empty when it is fit to publish. */
export function checkVsix({ entries, target, version, preRelease }) {
  const problems = [];
  const sidecar = SIDECARS[target];
  if (!sidecar) return [`unsupported target ${target}`];
  const sidecarPath = `extension/bin/${sidecar.file}`;
  const byName = new Map(entries.map((entry) => [entry.name, entry]));

  for (const { name } of entries) {
    if (name.endsWith("/") || name === sidecarPath) continue;
    if (FORBIDDEN_FILES.some((pattern) => pattern.test(name)) || !ALLOWED_FILES.some((pattern) => pattern.test(name))) {
      problems.push(`unexpected file ${name}`);
    }
  }
  for (const name of [...REQUIRED_NON_EMPTY, sidecarPath]) {
    const entry = byName.get(name);
    if (!entry) problems.push(`missing ${name}`);
    else if (entry.read().length === 0) problems.push(`${name} is empty`);
  }
  for (const pattern of [/^extension\/readme\.md$/i, /^extension\/changelog\.md$/i, /^extension\/license(?:\.txt|\.md)?$/i]) {
    if (!entries.some((entry) => pattern.test(entry.name))) problems.push(`missing a file matching ${pattern}`);
  }

  const sidecarEntry = byName.get(sidecarPath);
  if (sidecarEntry && sidecarEntry.read().length > 0) {
    if (!sidecar.matches(sidecarEntry.read())) problems.push(`sidecar is not ${sidecar.format}`);
    if (target !== "win32-x64" && (sidecarEntry.mode === null || (sidecarEntry.mode & 0o111) === 0)) {
      problems.push(`sidecar is not executable (mode ${sidecarEntry.mode?.toString(8) ?? "unknown"})`);
    }
  }

  const manifestEntry = byName.get("extension.vsixmanifest");
  if (!manifestEntry) {
    problems.push("missing extension.vsixmanifest");
  } else {
    const manifest = manifestEntry.read().toString("utf8");
    const identity = /<Identity\b[^>]*>/.exec(manifest)?.[0] ?? "";
    for (const [name, expected] of [
      ["Id", NAME],
      ["Publisher", PUBLISHER],
      ["Version", version],
      ["TargetPlatform", target],
    ]) {
      const actual = attribute(identity, name);
      if (actual !== expected) problems.push(`manifest ${name} is ${JSON.stringify(actual)}, expected "${expected}"`);
    }
    const preReleaseProperty = /<Property Id="Microsoft\.VisualStudio\.Code\.PreRelease" Value="true"/.test(manifest);
    if (preRelease && !preReleaseProperty) problems.push("package is not marked as a pre-release");
    if (!preRelease && preReleaseProperty) problems.push("package is marked as a pre-release");
    const source = /<Property Id="Microsoft\.VisualStudio\.Services\.Links\.Source" Value="([^"]*)"/.exec(manifest)?.[1];
    if (!source?.startsWith(REPOSITORY)) problems.push(`manifest repository link is ${quoted(source)}`);
    const icon = /<Icon>([^<]*)<\/Icon>/.exec(manifest)?.[1];
    if (icon !== "extension/icon.png") problems.push(`manifest icon is ${quoted(icon)}, expected "extension/icon.png"`);
  }

  const packageEntry = byName.get("extension/package.json");
  if (packageEntry && packageEntry.read().length > 0) {
    const manifest = JSON.parse(packageEntry.read().toString("utf8"));
    for (const [name, expected] of [
      ["name", NAME],
      ["publisher", PUBLISHER],
      ["version", version],
      ["license", "MIT"],
      ["main", "./dist/extension.js"],
    ]) {
      if (manifest[name] !== expected) {
        problems.push(`package.json ${name} is ${quoted(manifest[name])}, expected "${expected}"`);
      }
    }
    if (manifest.icon !== "icon.png") problems.push(`package.json icon is ${quoted(manifest.icon)}, expected "icon.png"`);
    if (manifest.repository?.url !== REPOSITORY) {
      problems.push(`package.json repository.url is ${quoted(manifest.repository?.url)}, expected "${REPOSITORY}"`);
    }
  }
  return problems;
}

export function parseOptions(args) {
  const list = args[0] === "--" ? args.slice(1) : [...args];
  const options = { target: undefined, version: undefined, preRelease: false, vsixPath: undefined };
  for (let index = 0; index < list.length; index += 1) {
    const argument = list[index];
    if (argument === "--pre-release") options.preRelease = true;
    else if (argument === "--target") options.target = list[++index];
    else if (argument === "--version") options.version = list[++index];
    else options.vsixPath = argument;
  }
  if (!options.target) throw new Error("--target is required");
  if (!SIDECARS[options.target]) throw new Error(`unsupported VSIX target: ${options.target}`);
  if (!options.version) throw new Error("--version is required");
  if (!options.vsixPath) throw new Error("the .vsix package path is required");
  return options;
}

export async function runCheck({ target, version, preRelease, vsixPath }, log = console.log) {
  const entries = readZipEntries(await readFile(vsixPath));
  const problems = checkVsix({ entries, target, version, preRelease });
  for (const problem of problems) log(`${vsixPath}: ${problem}`);
  if (problems.length === 0) {
    log(`${vsixPath}: ok (${entries.length} files, ${target}, ${version}${preRelease ? ", pre-release" : ""})`);
  }
  return problems.length === 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  Promise.resolve()
    .then(() => runCheck(parseOptions(process.argv.slice(2))))
    .then((ok) => {
      if (!ok) process.exitCode = 1;
    })
    .catch((error) => {
      console.error(error instanceof Error ? error.message : String(error));
      process.exitCode = 1;
    });
}
