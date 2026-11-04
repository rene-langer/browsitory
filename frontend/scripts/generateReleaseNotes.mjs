#!/usr/bin/env node
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// A bullet that starts with one of these prefixes ("- Fixed: ...") records it as the
// item's kind; the UI shows the kind as a label, so the prefix is removed from the text.
const KIND_PREFIXES = {
  Added: "added",
  Changed: "changed",
  Fixed: "fixed",
  Removed: "removed",
  Deprecated: "deprecated",
  Security: "security",
};

// Developer-only content is left out of the in-app notes: a section headed exactly
// "### Internal" or "### Internal: ...", and a bullet starting "- Internal: ...". A topic that
// merely starts with the word ("### Internal links") is kept. See changelog.d/README.md.
const INTERNAL_HEADING = /^Internal(:|\s*$)/i;
const INTERNAL_BULLET = /^Internal:\s*/i;

const UNRELEASED = /^##\s+\[?Unreleased\b/i;
// Text after the date ("(yanked)") is allowed and ignored.
const RELEASE = /^##\s+\[([^\]]+)\]\s*[-–—]\s*(\S+)/;
// A code fence: three or more backticks or tildes. It is closed only by a line of the same
// character, at least as long, with nothing after it.
const FENCE = /^\s*(`{3,}|~{3,})(.*)$/;

/** Parses a bullet's text; `null` for an internal bullet. */
function parseItem(raw) {
  if (INTERNAL_BULLET.test(raw)) return null;
  const match = raw.match(/^([A-Z][a-z]+):\s+(.+)/);
  if (match !== null && KIND_PREFIXES[match[1]] !== undefined) {
    return { kind: KIND_PREFIXES[match[1]], text: match[2] };
  }
  return { text: raw };
}

function appendText(item, text) {
  item.text += ` ${text}`;
}

/**
 * Parses the released sections of CHANGELOG.md into release notes.
 *
 * Each `### Heading` under a `## [X.Y.Z] - date` section becomes a group: a topic heading
 * ("### Commit signing") or a Keep a Changelog heading ("### Fixed"). Repeated headings in
 * one release are merged. A group holds blocks in changelog order: paragraphs and bullet
 * lists. Indented lines continue the bullet above them, and indented bullets become its
 * sub-items. Code fences are skipped. A release with nothing for users is kept with no
 * groups and a warning; a `## ` heading that is neither `[Unreleased]` nor a release, or a
 * code fence that is never closed, throws.
 */
export function parseChangelog(markdown, { onWarning = console.warn } = {}) {
  const entries = [];
  let current = null;
  let group = null; // null while outside a usable `###` section
  let paragraph = []; // lines of the paragraph being read
  let list = null; // the open list block
  let item = null; // its last top-level item (a detached one for an internal bullet)
  let subItem = null; // the last sub-item of `item`
  let subIndent = 0;
  let afterBlank = false;
  let openFence = ""; // the marker of the open code fence ("```", "~~~~"), or "" outside one
  let fenceLine = 0; // the line that opened it

  function flushParagraph() {
    if (paragraph.length > 0) group.blocks.push({ paragraph: paragraph.join(" ") });
    paragraph = [];
  }

  function closeList() {
    list = null;
    item = null;
    subItem = null;
  }

  function closeGroup() {
    if (group !== null) flushParagraph();
    group = null;
    closeList();
  }

  const lines = markdown.split("\n");
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index].replace(/\r$/, "");

    const fence = line.match(FENCE);
    if (openFence !== "") {
      const closes =
        fence !== null &&
        fence[1][0] === openFence[0] &&
        fence[1].length >= openFence.length &&
        fence[2].trim() === "";
      if (closes) {
        openFence = "";
        // A fence ends the bullet before it, like a blank line.
        afterBlank = true;
      }
      continue;
    }
    if (fence !== null) {
      if (group !== null) flushParagraph();
      openFence = fence[1];
      fenceLine = index + 1;
      continue;
    }

    if (/^##\s/.test(line)) {
      closeGroup();
      if (UNRELEASED.test(line)) {
        current = null;
        continue;
      }
      const release = line.match(RELEASE);
      if (release === null) {
        throw new Error(
          `CHANGELOG.md line ${index + 1}: can't read the release heading "${line}". ` +
            'Expected "## [X.Y.Z] - YYYY-MM-DD" or "## [Unreleased]".',
        );
      }
      current = { version: release[1], date: release[2], groups: [] };
      entries.push(current);
      continue;
    }

    const heading = line.match(/^###\s+(.+?)\s*$/);
    if (heading !== null) {
      closeGroup();
      const title = heading[1];
      if (current === null || INTERNAL_HEADING.test(title)) continue;
      group = current.groups.find((g) => g.title === title) ?? null;
      if (group === null) {
        group = { title, blocks: [] };
        current.groups.push(group);
      }
      continue;
    }
    if (group === null) continue;

    if (line.trim() === "") {
      flushParagraph();
      afterBlank = true;
      continue;
    }
    const wasAfterBlank = afterBlank;
    afterBlank = false;

    const bullet = line.match(/^(\s*)[-*+]\s+(.+)$/);
    // A top-level bullet, or an indented one with no bullet above it to nest under.
    if (bullet !== null && (bullet[1].length === 0 || item === null)) {
      flushParagraph();
      if (list === null) {
        const last = group.blocks[group.blocks.length - 1];
        list = last !== undefined && "items" in last ? last : { items: [] };
        if (list !== last) group.blocks.push(list);
      }
      const parsed = parseItem(bullet[2].trim());
      item = parsed ?? { text: "" };
      if (parsed !== null) list.items.push(parsed);
      subItem = null;
      continue;
    }
    if (bullet !== null && item !== null) {
      const parsed = parseItem(bullet[2].trim());
      subItem = parsed ?? { text: "" };
      subIndent = bullet[1].length;
      if (parsed !== null && list.items.includes(item)) (item.children ??= []).push(parsed);
      continue;
    }

    const text = line.trim();
    const indent = line.length - line.trimStart().length;
    if (item !== null && indent > 0) {
      // A wrapped line: it continues the sub-item it is indented under, or the bullet.
      appendText(subItem !== null && indent >= subIndent ? subItem : item, text);
      continue;
    }
    if (item !== null && !wasAfterBlank) {
      // Lazy continuation, as in Markdown: unindented text right after a bullet.
      appendText(subItem ?? item, text);
      continue;
    }
    closeList();
    paragraph.push(text);
  }
  if (openFence !== "") {
    throw new Error(`CHANGELOG.md line ${fenceLine}: this code fence is never closed.`);
  }
  closeGroup();

  for (const entry of entries) {
    for (const g of entry.groups) {
      g.blocks = g.blocks.filter((block) => !("items" in block) || block.items.length > 0);
    }
    entry.groups = entry.groups.filter((g) => g.blocks.length > 0);
    // Kept with no groups, so the app can still find it as the last-seen version.
    if (entry.groups.length === 0) {
      onWarning(`CHANGELOG.md: release ${entry.version} has no release notes for users.`);
    }
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
  let entries;
  try {
    entries = parseChangelog(markdown);
  } catch (error) {
    console.error(error instanceof Error ? error.message : error);
    process.exit(1);
  }
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, JSON.stringify(entries, null, 2) + "\n");
  console.log(`Wrote ${entries.length} release notes entries to ${outPath}`);
}
