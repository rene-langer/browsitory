import type { DiffHunk } from "../ipc/RepoClient";

/**
 * Large and generated diffs (#686). A diff with more lines than `LARGE_DIFF_LINES`, or of a file
 * that tools generate (a lockfile, minified code, a source map), starts collapsed: `DiffView`
 * shows "Load diff (N lines)" instead of rendering it. A loaded diff renders `DIFF_WINDOW_LINES`
 * lines at a time, with buttons for more.
 */
export const LARGE_DIFF_LINES = 1000;
export const DIFF_WINDOW_LINES = 2000;

// File names package managers and build tools write: their diffs are rarely read line by line.
const GENERATED_NAMES = new Set([
  "Cargo.lock",
  "package-lock.json",
  "npm-shrinkwrap.json",
  "pnpm-lock.yaml",
  "yarn.lock",
  "bun.lock",
  "composer.lock",
  "Gemfile.lock",
  "Podfile.lock",
  "Pipfile.lock",
  "poetry.lock",
  "uv.lock",
  "pdm.lock",
  "go.sum",
  "flake.lock",
  "mix.lock",
  "pubspec.lock",
  "Package.resolved",
  "packages.lock.json",
  "gradle.lockfile",
  "deno.lock",
]);

const GENERATED_SUFFIXES = [".min.js", ".min.mjs", ".min.cjs", ".min.css", ".js.map", ".mjs.map", ".css.map"];

/** Whether `path` names a generated file (a lockfile, minified code, a source map). */
export function isGeneratedPath(path: string): boolean {
  const name = path.slice(path.lastIndexOf("/") + 1);
  if (GENERATED_NAMES.has(name)) return true;
  const lower = name.toLowerCase();
  return GENERATED_SUFFIXES.some((suffix) => lower.endsWith(suffix));
}

/** Every line of `hunks`, context included: what the diff would render. */
export function diffLineCount(hunks: readonly DiffHunk[]): number {
  return hunks.reduce((sum, hunk) => sum + hunk.lines.length, 0);
}

/** Why a diff starts collapsed, or `null` when it renders right away. */
export function collapseReason(path: string | undefined, lineCount: number): "generated" | "large" | null {
  if (lineCount === 0) return null;
  if (path !== undefined && isGeneratedPath(path)) return "generated";
  return lineCount > LARGE_DIFF_LINES ? "large" : null;
}

/**
 * The first `limit` lines of `hunks`, as hunks: whole hunks while they fit, then the start of
 * the hunk the limit falls in. Hunks past it are left out.
 */
export function windowHunks(hunks: DiffHunk[], limit: number): DiffHunk[] {
  if (diffLineCount(hunks) <= limit) return hunks;
  const out: DiffHunk[] = [];
  let left = limit;
  for (const hunk of hunks) {
    if (left <= 0) break;
    out.push(hunk.lines.length <= left ? hunk : { ...hunk, lines: hunk.lines.slice(0, left) });
    left -= hunk.lines.length;
  }
  return out;
}
