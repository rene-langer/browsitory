import * as path from "node:path";

/**
 * Pure helpers for the extension host's external-tool methods: work tree path checks, and the
 * `browsitory-file:` URIs that VSCode's diff and merge editors read repository versions through
 * (`extension.ts`'s content provider, backed by the sidecar's internal `read_file_version`).
 *
 * The frontend's `RepoClient.DiffSource` is duplicated here on purpose: the extension never
 * imports from `frontend/`, the same way the sidecar keeps its own DTO copies.
 */

export const FILE_VERSION_SCHEME = "browsitory-file";

export type FileVersion =
  | { kind: "Head" }
  | { kind: "Index" }
  | { kind: "Stage"; stage: 1 | 2 | 3 }
  | { kind: "Commit"; commitId: string }
  | { kind: "FirstParent"; commitId: string };

/** `Range.toId === null` compares `fromId` with the working tree. */
export type DiffSource =
  | { kind: "Unstaged" }
  | { kind: "Staged"; oldPath: string | null }
  | { kind: "Commit"; commitId: string; oldPath: string | null }
  | { kind: "Range"; fromId: string; toId: string | null; oldPath: string | null };

export interface FileVersionRef {
  repoPath: string;
  path: string;
  version: FileVersion;
}

const INVALID_PATH = "Invalid path.";
const COMMIT_ID = /^[0-9a-fA-F]{4,64}$/;

function isWindowsPath(absolute: string): boolean {
  return /^[A-Za-z]:/.test(absolute) || absolute.startsWith("\\\\");
}

/**
 * True for a work-tree-relative path that cannot leave the work tree on any OS: not empty, not
 * absolute or drive-relative (POSIX or Windows), and no empty, `.` or `..` segment when split on
 * either separator.
 */
export function isSafeRelativePath(rel: string): boolean {
  if (rel === "" || rel.includes("\0")) return false;
  if (path.posix.isAbsolute(rel) || path.win32.isAbsolute(rel) || /^[A-Za-z]:/.test(rel)) return false;
  return rel.split(/[\\/]/).every((part) => part !== "" && part !== "." && part !== "..");
}

/** The absolute path of `rel` inside `workdir`. Throws "Invalid path." for anything else. */
export function resolveRepoFile(workdir: string, rel: string): string {
  if (!isSafeRelativePath(rel)) throw new Error(INVALID_PATH);
  if (isWindowsPath(workdir)) {
    // `name:stream` would address an NTFS alternate data stream, not a work tree file.
    if (rel.includes(":")) throw new Error(INVALID_PATH);
    return path.win32.join(workdir, ...rel.split(/[\\/]/));
  }
  return path.posix.join(workdir, ...rel.split("/"));
}

export function fileVersionUri(ref: FileVersionRef): string {
  const encodedPath = ref.path.split("/").map(encodeURIComponent).join("/");
  // base64url keeps the query free of `%` escapes, which VSCode's `Uri` would decode once more.
  const query = Buffer.from(JSON.stringify(ref), "utf8").toString("base64url");
  return `${FILE_VERSION_SCHEME}:/${encodedPath}?${query}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const own = Object.keys(value);
  return own.length === keys.length && keys.every((key) => own.includes(key));
}

function parseVersion(value: unknown): FileVersion {
  if (!isRecord(value)) throw new Error("Malformed browsitory-file URI.");
  switch (value["kind"]) {
    case "Head":
    case "Index":
      if (hasExactKeys(value, ["kind"])) return { kind: value["kind"] };
      break;
    case "Stage": {
      const stage = value["stage"];
      if (hasExactKeys(value, ["kind", "stage"]) && (stage === 1 || stage === 2 || stage === 3)) {
        return { kind: "Stage", stage };
      }
      break;
    }
    case "Commit":
    case "FirstParent": {
      const commitId = value["commitId"];
      if (hasExactKeys(value, ["kind", "commitId"]) && typeof commitId === "string" && COMMIT_ID.test(commitId)) {
        return { kind: value["kind"], commitId };
      }
      break;
    }
  }
  throw new Error("Malformed browsitory-file URI.");
}

/**
 * Parses a `browsitory-file:` URI (`path` decoded, as `vscode.Uri` exposes it). Any other
 * extension can hand VSCode such a URI, so every field is checked strictly.
 */
export function parseFileVersionUri(uri: { path: string; query: string }): FileVersionRef {
  let ref: unknown;
  try {
    ref = JSON.parse(Buffer.from(uri.query, "base64url").toString("utf8"));
  } catch {
    throw new Error("Malformed browsitory-file URI.");
  }
  if (
    !isRecord(ref) ||
    !hasExactKeys(ref, ["repoPath", "path", "version"]) ||
    typeof ref["repoPath"] !== "string" ||
    ref["repoPath"] === "" ||
    typeof ref["path"] !== "string" ||
    !isSafeRelativePath(ref["path"]) ||
    uri.path !== `/${ref["path"]}`
  ) {
    throw new Error("Malformed browsitory-file URI.");
  }
  return { repoPath: ref["repoPath"], path: ref["path"], version: parseVersion(ref["version"]) };
}

/** A `file:` URI string for an absolute POSIX or Windows path. */
export function fileUri(absolute: string): string {
  const encode = (parts: string[]) => parts.map(encodeURIComponent).join("/");
  if (absolute.startsWith("\\\\")) {
    const [host = "", ...rest] = absolute.slice(2).split(/[\\/]/);
    return `file://${host}/${encode(rest)}`;
  }
  const drive = /^([A-Za-z]:)[\\/]?(.*)$/.exec(absolute);
  if (drive !== null) {
    return `file:///${drive[1]}/${encode((drive[2] ?? "").split(/[\\/]/))}`;
  }
  return `file://${encode(absolute.split("/"))}`;
}

const short = (id: string) => id.slice(0, 7);
const baseName = (rel: string) => rel.split(/[\\/]/).at(-1) ?? rel;

export function diffSides(
  repoPath: string,
  workdir: string,
  rel: string,
  source: DiffSource,
): { left: string; right: string; title: string } {
  const name = baseName(rel);
  const at = (p: string, version: FileVersion) => fileVersionUri({ repoPath, path: p, version });
  const worktree = () => fileUri(resolveRepoFile(workdir, rel));
  switch (source.kind) {
    case "Unstaged":
      return { left: at(rel, { kind: "Index" }), right: worktree(), title: `${name} (Index ↔ Working Tree)` };
    case "Staged":
      return {
        left: at(source.oldPath ?? rel, { kind: "Head" }),
        right: at(rel, { kind: "Index" }),
        title: `${name} (HEAD ↔ Index)`,
      };
    case "Commit":
      return {
        left: at(source.oldPath ?? rel, { kind: "FirstParent", commitId: source.commitId }),
        right: at(rel, { kind: "Commit", commitId: source.commitId }),
        title: `${name} (${short(source.commitId)} ↔ parent)`,
      };
    case "Range": {
      const left = at(source.oldPath ?? rel, { kind: "Commit", commitId: source.fromId });
      if (source.toId === null) {
        return { left, right: worktree(), title: `${name} (${short(source.fromId)} ↔ Working Tree)` };
      }
      return {
        left,
        right: at(rel, { kind: "Commit", commitId: source.toId }),
        title: `${name} (${short(source.fromId)} ↔ ${short(source.toId)})`,
      };
    }
  }
}

export function mergeSides(
  repoPath: string,
  workdir: string,
  rel: string,
): { base: string; ours: string; theirs: string; output: string } {
  const stage = (s: 1 | 2 | 3) => fileVersionUri({ repoPath, path: rel, version: { kind: "Stage", stage: s } });
  return { base: stage(1), ours: stage(2), theirs: stage(3), output: fileUri(resolveRepoFile(workdir, rel)) };
}
