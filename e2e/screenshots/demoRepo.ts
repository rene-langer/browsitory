import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

// The fixed demo repository the documentation screenshots show (see ./README.md).
//
// Everything is deterministic: fixed file contents, authors and dates, so two runs produce the
// same commits (same hashes) and the same pictures. Only the app's relative dates ("2 months
// ago") change with the day you run it.

/** Everything the screenshot run writes lives under this folder. */
export const SCREENSHOT_ROOT = path.join(os.tmpdir(), "browsitory-screenshots");
/** The demo repository the app opens. Its folder name is the tab title. */
export const DEMO_REPO_PATH = path.join(SCREENSHOT_ROOT, "trailmap");
/** Throwaway HOME (and XDG dirs, git and GnuPG config) for the run, see `isolateHome`. */
export const ISOLATED_HOME = path.join(SCREENSHOT_ROOT, "home");

const AUTHORS = {
  alex: { name: "Alex Rivera", email: "alex@example.com" },
  sam: { name: "Sam Chen", email: "sam@example.com" },
} as const;
type Author = keyof typeof AUTHORS;

/**
 * Points HOME, the XDG dirs, git's global config and GnuPG at empty temp dirs, for this process
 * and everything it starts (the WebDriver workers, tauri-driver and the app). The run then never
 * reads the user's own git config, signing keys or app data, and never writes to them.
 */
export function isolateHome(): void {
  fs.rmSync(ISOLATED_HOME, { recursive: true, force: true });
  const dirs = {
    HOME: ISOLATED_HOME,
    XDG_CONFIG_HOME: path.join(ISOLATED_HOME, ".config"),
    XDG_CACHE_HOME: path.join(ISOLATED_HOME, ".cache"),
    XDG_STATE_HOME: path.join(ISOLATED_HOME, ".local", "state"),
    GNUPGHOME: path.join(ISOLATED_HOME, ".gnupg"),
  };
  for (const [name, dir] of Object.entries(dirs)) {
    fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
    process.env[name] = dir;
  }
  const gitConfig = path.join(ISOLATED_HOME, ".gitconfig");
  fs.writeFileSync(gitConfig, "");
  process.env.GIT_CONFIG_GLOBAL = gitConfig;
  process.env.GIT_CONFIG_NOSYSTEM = "1";
  // English dates and a fixed time zone, whatever the machine's own settings are.
  process.env.LANG = "en_US.UTF-8";
  process.env.LC_ALL = "en_US.UTF-8";
  process.env.LANGUAGE = "en_US";
  process.env.TZ = "UTC";
}

let clock = Date.parse("2026-09-01T09:00:00+02:00");

/** The next commit time: one day and a bit after the previous one. */
function nextDate(): string {
  clock += 26 * 60 * 60 * 1000;
  return new Date(clock).toISOString();
}

function git(args: string[], author: Author = "alex", date?: string): string {
  const who = AUTHORS[author];
  const when = date ?? new Date(clock).toISOString();
  return execFileSync("git", args, {
    cwd: DEMO_REPO_PATH,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
      GIT_AUTHOR_NAME: who.name,
      GIT_AUTHOR_EMAIL: who.email,
      GIT_AUTHOR_DATE: when,
      GIT_COMMITTER_NAME: who.name,
      GIT_COMMITTER_EMAIL: who.email,
      GIT_COMMITTER_DATE: when,
    },
  }).trim();
}

function write(file: string, content: string): void {
  const target = path.join(DEMO_REPO_PATH, file);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, content);
}

function commit(author: Author, message: string, files: Record<string, string>): string {
  for (const [file, content] of Object.entries(files)) write(file, content);
  git(["add", "--", ...Object.keys(files)], author);
  git(["commit", "-q", "-m", message], author, nextDate());
  return git(["rev-parse", "HEAD"], author);
}

const README_V1 = `# Trailmap

Plan hiking routes on a map and share them with friends.
`;

const README_V2 = `${README_V1}
## Setup

1. Install Node.js 22 or newer.
2. Run \`npm install\`.
3. Run \`npm start\` and open http://localhost:8080.
`;

const PACKAGE_JSON = `{
  "name": "trailmap",
  "version": "0.2.0",
  "private": true,
  "scripts": {
    "start": "node server.js"
  }
}
`;

const APP_V1 = `import { createMap } from "./map.js";

const map = createMap(document.getElementById("map"));
map.show({ lat: 47.42, lon: 10.98 });
`;

const APP_V2 = `import { createMap } from "./map.js";
import { createSearch } from "./search.js";

const map = createMap(document.getElementById("map"));
createSearch(document.getElementById("search"), (place) => map.show(place));
map.show({ lat: 47.42, lon: 10.98 });
`;

// Unstaged change in the demo's working tree.
const APP_V3 = `import { createMap } from "./map.js";
import { createSearch } from "./search.js";
import { addZoomControls } from "./zoom.js";

const map = createMap(document.getElementById("map"));
createSearch(document.getElementById("search"), (place) => map.show(place));
addZoomControls(document.getElementById("toolbar"), map);
map.show({ lat: 47.42, lon: 10.98, zoom: 12 });
`;

function mapJs(cacheSize: number, themed: boolean, zoom = 11): string {
  return `const TILE_CACHE_SIZE = ${cacheSize};
const DEFAULT_ZOOM = ${zoom};

export function createMap(element) {
  const tiles = new Map();
  return {
    show({ lat, lon, zoom = DEFAULT_ZOOM }) {
      element.dataset.center = \`\${lat},\${lon}\`;
      element.dataset.zoom = String(zoom);
      if (tiles.size > TILE_CACHE_SIZE) tiles.clear();
    },
  };
}
${
  themed
    ? `
export function mapColours(theme) {
  return theme === "dark"
    ? { land: "#1e2a24", water: "#16324a", trail: "#f2b84b" }
    : { land: "#eef3e8", water: "#b9d7f0", trail: "#c0392b" };
}
`
    : ""
}`;
}

function searchJs(placeholder: string): string {
  return `export function createSearch(input, onPick) {
  input.placeholder = "${placeholder}";
  input.addEventListener("change", () => onPick(lookup(input.value)));
}

function lookup(name) {
  return { lat: 47.42, lon: 10.98, name };
}
`;
}

const THEME_JS = `export function currentTheme() {
  return localStorage.getItem("theme") ?? "light";
}

export function toggleTheme() {
  const next = currentTheme() === "dark" ? "light" : "dark";
  localStorage.setItem("theme", next);
  document.documentElement.dataset.theme = next;
}
`;

const OFFLINE_V1 = `const CACHE_NAME = "trailmap-tiles";

export async function saveTile(url) {
  const cache = await caches.open(CACHE_NAME);
  await cache.add(url);
}
`;

const OFFLINE_V2 = `${OFFLINE_V1}
export function showOfflineBanner(element) {
  element.hidden = navigator.onLine;
}
`;

const ZOOM_JS = `export function addZoomControls(toolbar, map) {
  for (const [label, step] of [["+", 1], ["-", -1]]) {
    const button = document.createElement("button");
    button.textContent = label;
    button.addEventListener("click", () => map.zoomBy(step));
    toolbar.append(button);
  }
}
`;

/**
 * Builds the demo repository from scratch at DEMO_REPO_PATH:
 *
 * - `main` with a merged feature branch (`feature/dark-mode`), tags `v0.1.0` and `v0.2.0`, one
 *   commit ahead of `origin/main`;
 * - `feature/offline-tiles` (two commits, one ahead of its upstream) and `fix/search-label`,
 *   which conflicts with `main` when merged (see `startConflict`);
 * - a remote `origin` with remote-tracking branches (it is never contacted);
 * - two stashes, a staged new file, an unstaged edit and an untracked file.
 */
export function buildDemoRepo(): void {
  fs.rmSync(DEMO_REPO_PATH, { recursive: true, force: true });
  fs.mkdirSync(DEMO_REPO_PATH, { recursive: true });
  git(["init", "-q", "-b", "main"]);
  for (const [key, value] of [
    ["user.name", AUTHORS.alex.name],
    ["user.email", AUTHORS.alex.email],
    ["core.autocrlf", "false"],
    ["commit.gpgsign", "false"],
    ["tag.gpgsign", "false"],
  ]) {
    git(["config", key, value]);
  }

  commit("alex", "Initial commit", { "README.md": README_V1, "package.json": PACKAGE_JSON, "src/app.js": APP_V1 });
  commit("sam", "Add the map view", { "src/map.js": mapJs(500, false) });
  commit("alex", "Add route search", { "src/search.js": searchJs("Search trails"), "src/app.js": APP_V2 });
  git(["tag", "-a", "v0.1.0", "-m", "First preview"], "alex", nextDate());

  git(["switch", "-q", "-c", "feature/dark-mode"]);
  commit("sam", "Add a theme toggle", { "src/theme.js": THEME_JS });
  const darkMode = commit("sam", "Use theme colours on the map", { "src/map.js": mapJs(500, true) });

  git(["switch", "-q", "main"]);
  commit("alex", "Limit the tile cache to 200 entries", { "src/map.js": mapJs(200, false) });
  // Both sides changed map.js in different places; resolve to the combined version.
  git(["merge", "-q", "--no-ff", "--no-commit", "feature/dark-mode"], "alex");
  write("src/map.js", mapJs(200, true));
  git(["add", "src/map.js"]);
  git(["commit", "-q", "-m", "Merge branch 'feature/dark-mode'"], "alex", nextDate());
  git(["tag", "-a", "v0.2.0", "-m", "Dark mode"], "alex", nextDate());

  git(["switch", "-q", "-c", "fix/search-label"]);
  commit("sam", "Clarify the search placeholder", { "src/search.js": searchJs("Search trails and peaks") });

  git(["switch", "-q", "-c", "feature/offline-tiles", "main"]);
  const offline1 = commit("sam", "Cache map tiles for offline use", { "src/offline.js": OFFLINE_V1 });
  commit("sam", "Show a banner while offline", { "src/offline.js": OFFLINE_V2 });

  git(["switch", "-q", "main"]);
  const renamed = commit("alex", "Shorten the search placeholder", { "src/search.js": searchJs("Find a trail") });
  commit("alex", "Add setup steps to the README", { "README.md": README_V2 });

  // A remote that is never contacted: remote-tracking refs are written directly.
  git(["remote", "add", "origin", "https://github.com/example/trailmap.git"]);
  git(["update-ref", "refs/remotes/origin/main", renamed]);
  git(["update-ref", "refs/remotes/origin/feature/dark-mode", darkMode]);
  git(["update-ref", "refs/remotes/origin/feature/offline-tiles", offline1]);
  for (const branch of ["main", "feature/offline-tiles"]) {
    git(["config", `branch.${branch}.remote`, "origin"]);
    git(["config", `branch.${branch}.merge`, `refs/heads/${branch}`]);
  }

  // Stashes: oldest first.
  write("src/map.js", mapJs(200, true, 13));
  git(["stash", "push", "-q", "-m", "Try a closer default zoom"], "alex", nextDate());
  write("README.md", `${README_V2}\n## Licence\n\nMIT\n`);
  git(["stash", "push", "-q", "-m", "Licence section draft"], "alex", nextDate());

  // Working tree: a staged new file, an unstaged edit and an untracked file.
  write("src/zoom.js", ZOOM_JS);
  git(["add", "src/zoom.js"]);
  write("src/app.js", APP_V3);
  write("NOTES.md", "- Ask Sam about tile licences\n");
}

/**
 * Puts the demo repository into a conflicted merge: unstages everything (a merge refuses to
 * start with staged changes), then merges `fix/search-label`, which changes the same line of
 * src/search.js as `main` did.
 */
export function startConflict(): void {
  git(["reset", "-q"]);
  try {
    git(["merge", "fix/search-label"], "alex", nextDate());
  } catch {
    // Expected: the merge stops on the conflict.
  }
  if (!fs.existsSync(path.join(DEMO_REPO_PATH, ".git", "MERGE_HEAD"))) {
    throw new Error("expected the demo merge to stop on a conflict");
  }
}
