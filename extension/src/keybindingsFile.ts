import * as path from "node:path";

/**
 * The keys VSCode runs Browsitory's shortcuts with (#900). VSCode has no API for the resolved
 * keybindings, so the extension reads the user's `keybindings.json` itself and applies it on top
 * of the keybindings it contributes, the way VSCode does: an entry for a
 * `browsitory.shortcut.<id>` command adds a key, a `-browsitory.shortcut.<id>` entry removes one
 * (or all of them, without a key), and a key the user gives another command without a `when`
 * clause wins over Browsitory's. A `when` clause can't be evaluated here, so such an entry for
 * another command is left alone. The webview shows and matches what this returns.
 */

const SHORTCUT_COMMAND_PREFIX = "browsitory.shortcut.";

/** Strips `//` and `/* *\/` comments and trailing commas outside strings, as JSONC allows. */
function stripJsonc(text: string): string {
  let out = "";
  let index = 0;
  while (index < text.length) {
    const char = text[index];
    if (char === '"') {
      let end = index + 1;
      while (end < text.length && text[end] !== '"') end += text[end] === "\\" ? 2 : 1;
      out += text.slice(index, end + 1);
      index = end + 1;
    } else if (char === "/" && text[index + 1] === "/") {
      while (index < text.length && text[index] !== "\n") index += 1;
    } else if (char === "/" && text[index + 1] === "*") {
      const end = text.indexOf("*/", index + 2);
      index = end === -1 ? text.length : end + 2;
    } else if (char === ",") {
      let next = index + 1;
      while (next < text.length && /\s/.test(text.charAt(next))) next += 1;
      if (text[next] !== "]" && text[next] !== "}") out += char;
      index += 1;
    } else {
      out += char;
      index += 1;
    }
  }
  return out;
}

/** The entries of a `keybindings.json`; none when it isn't a readable array. */
export function parseKeybindingsJson(text: string): unknown[] {
  try {
    const parsed: unknown = JSON.parse(stripJsonc(text));
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

const MODIFIER_ORDER = ["ctrl", "shift", "alt", "cmd", "meta", "win"];

/** A key in one spelling: lower case, the modifiers in VSCode's order; each step of a two-step key. */
export function normalizeVscodeKey(key: string): string {
  return key
    .trim()
    .toLowerCase()
    .split(/\s+/)
    .map((step) => {
      const parts = step.split("+");
      // "ctrl++" is Ctrl and the plus key.
      const last = parts.length > 1 && parts[parts.length - 1] === "" ? "+" : parts[parts.length - 1];
      const modifiers = parts.slice(0, last === "+" && parts.length > 1 ? -2 : -1);
      modifiers.sort((a, b) => MODIFIER_ORDER.indexOf(a) - MODIFIER_ORDER.indexOf(b));
      return [...modifiers, last].join("+");
    })
    .join(" ");
}

interface Contributed {
  command: string;
  key?: string;
  mac?: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

/** Each contributed shortcut's keys on this platform, with the user's `keybindings.json` entries on top. */
export function effectiveShortcutKeys(
  contributed: readonly unknown[],
  user: readonly unknown[],
  isMac: boolean,
): Record<string, string[]> {
  const keys: Record<string, string[]> = {};
  for (const entry of contributed) {
    if (!isRecord(entry) || typeof entry.command !== "string") continue;
    const { command, key, mac } = entry as unknown as Contributed;
    if (!command.startsWith(SHORTCUT_COMMAND_PREFIX)) continue;
    const id = command.slice(SHORTCUT_COMMAND_PREFIX.length);
    const platformKey = isMac ? (mac ?? key) : key;
    keys[id] ??= [];
    if (typeof platformKey === "string") keys[id].push(normalizeVscodeKey(platformKey));
  }
  for (const entry of user) {
    if (!isRecord(entry) || typeof entry.command !== "string") continue;
    const key = typeof entry.key === "string" ? normalizeVscodeKey(entry.key) : undefined;
    if (entry.key !== undefined && key === undefined) continue;
    const command = entry.command;
    const removing = command.startsWith("-");
    const name = removing ? command.slice(1) : command;
    if (name.startsWith(SHORTCUT_COMMAND_PREFIX)) {
      const id = name.slice(SHORTCUT_COMMAND_PREFIX.length);
      // Only the commands the extension registers run.
      if (keys[id] === undefined) continue;
      if (removing) keys[id] = key === undefined ? [] : keys[id].filter((own) => own !== key);
      else if (key !== undefined && !keys[id].includes(key)) keys[id].push(key);
    } else if (!removing && key !== undefined && entry.when === undefined) {
      // Another command on the same key, everywhere: it runs instead.
      for (const [id, own] of Object.entries(keys)) keys[id] = own.filter((theirs) => theirs !== key);
    }
  }
  return keys;
}

/**
 * Where the user's `keybindings.json` is, from the extension's global storage folder
 * (`<User>/globalStorage/<extension>`, or `<User>/profiles/<id>/globalStorage/<extension>` in a
 * profile). A profile without its own keybindings uses the default profile's, so that file is
 * the fallback.
 */
export function keybindingsFileCandidates(globalStorage: string): string[] {
  const userFolder = path.dirname(path.dirname(globalStorage));
  const candidates = [path.join(userFolder, "keybindings.json")];
  const profiles = path.dirname(userFolder);
  if (path.basename(profiles) === "profiles") candidates.push(path.join(path.dirname(profiles), "keybindings.json"));
  return candidates;
}
