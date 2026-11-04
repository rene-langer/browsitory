#!/usr/bin/env node
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// A bullet that starts with one of these prefixes ("- Fixed: ...") records it as the
// item's kind; the UI shows the kind as a label, so the prefix is stripped from the text.
const KIND_PREFIXES = {
  Added: "added",
  Changed: "changed",
  Fixed: "fixed",
  Removed: "removed",
  Deprecated: "deprecated",
  Security: "security",
};

// Developer-only content ("### Internal", "### Internal: ...", "- Internal: ...") is left
// out of the in-app notes.
const INTERNAL_HEADING = /^Internal\b/i;
const INTERNAL_BULLET = /^Internal:\s*/i;

function parseBullet(raw) {
  const match = raw.match(/^([A-Z][a-z]+):\s+(.+)/);
  if (match !== null && KIND_PREFIXES[match[1]] !== undefined) {
    const rest = match[2];
    return { kind: KIND_PREFIXES[match[1]], text: rest.charAt(0).toUpperCase() + rest.slice(1) };
  }
  return { text: raw };
}

/**
 * Parses the released sections of CHANGELOG.md into release notes.
 *
 * Each `### Heading` under a `## [X.Y.Z] - date` section becomes a group: a topic heading
 * ("### Commit signing") or a Keep a Changelog heading ("### Fixed"). Repeated headings in
 * one release are merged. Indented lines continue the bullet above them; plain lines before
 * the first bullet are the group's intro paragraphs.
 */
export function parseChangelog(markdown) {
  const entries = [];
  let current = null;
  // The group lines are collected into; null while outside a usable `###` section.
  let group = null;
  // Raw bullets of the current group; `null` marks a skipped (internal) bullet.
  let bullets = [];
  let introLines = [];
  let paragraphs = [];

  function flushParagraph() {
    if (introLines.length > 0) paragraphs.push(introLines.join(" "));
    introLines = [];
  }

  function closeGroup() {
    if (group === null) return;
    flushParagraph();
    for (const raw of bullets) {
      if (raw !== null) group.items.push(parseBullet(raw));
    }
    if (paragraphs.length > 0) group.intro = [...(group.intro ?? []), ...paragraphs];
    group = null;
    bullets = [];
    paragraphs = [];
  }

  for (const line of markdown.split("\n")) {
    if (/^##\s/.test(line)) {
      closeGroup();
      const versionMatch = line.match(/^##\s+\[([^\]]+)\]\s*-\s*(\S+)/);
      current =
        versionMatch !== null && !/^Unreleased$/i.test(versionMatch[1])
          ? { version: versionMatch[1], date: versionMatch[2], groups: [] }
          : null;
      if (current !== null) entries.push(current);
      continue;
    }
    const headingMatch = line.match(/^###\s+(.+?)\s*$/);
    if (headingMatch !== null) {
      closeGroup();
      const title = headingMatch[1];
      if (current === null || INTERNAL_HEADING.test(title)) continue;
      group = current.groups.find((g) => g.title === title) ?? null;
      if (group === null) {
        group = { title, items: [] };
        current.groups.push(group);
      }
      continue;
    }
    if (group === null) continue;

    const bulletMatch = line.match(/^-\s+(.+)/);
    if (bulletMatch !== null) {
      flushParagraph();
      const text = bulletMatch[1].trim();
      bullets.push(INTERNAL_BULLET.test(text) ? null : text);
      continue;
    }
    // A wrapped bullet: an indented line under a bullet continues it.
    const continuation = line.match(/^\s+(\S.*)/);
    if (continuation !== null && bullets.length > 0) {
      const last = bullets.length - 1;
      if (bullets[last] !== null) bullets[last] += ` ${continuation[1].trim()}`;
      continue;
    }
    if (line.trim() === "") {
      flushParagraph();
      continue;
    }
    // Plain text before the first bullet is an intro paragraph.
    if (bullets.length === 0) introLines.push(line.trim());
  }
  closeGroup();

  for (const entry of entries) {
    entry.groups = entry.groups.filter((g) => g.items.length > 0);
  }
  return entries;
}

export function isMainModule(moduleUrl, entryPoint) {
  return entryPoint !== undefined && fileURLToPath(moduleUrl) === entryPoint;
}

const isMain = isMainModule(import.meta.url, process.argv[1]);
if (isMain) {
  const scriptDir = dirname(fileURLToPath(import.meta.url));
  const changelogPath = join(scriptDir, "..", "..", "CHANGELOG.md");
  const outPath = join(scriptDir, "..", "src", "generated", "releaseNotes.json");
  const markdown = readFileSync(changelogPath, "utf-8");
  const entries = parseChangelog(markdown);
  const empty = entries.filter((entry) => entry.groups.length === 0).map((entry) => entry.version);
  if (empty.length > 0) {
    console.error(`CHANGELOG.md: no release notes found for ${empty.join(", ")}`);
    process.exit(1);
  }
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, JSON.stringify(entries, null, 2) + "\n");
  console.log(`Wrote ${entries.length} release notes entries to ${outPath}`);
}
