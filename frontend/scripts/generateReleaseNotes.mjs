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

// Developer-only content ("### Internal", "### Internal: ...", "- Internal: ...") is left
// out of the in-app notes.
const INTERNAL_HEADING = /^Internal\b/i;
const INTERNAL_BULLET = /^Internal:\s*/i;

const UNRELEASED = /^##\s+\[?Unreleased\]?\s*$/i;
const RELEASE = /^##\s+\[([^\]]+)\]\s*[-–—]\s*(\S+)\s*$/;
const FENCE = /^\s*(```|~~~)/;

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
 * sub-items. Code fences are skipped. A release with nothing for users is skipped with a
 * warning; a `## ` heading that is neither `[Unreleased]` nor a release throws.
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
  let inFence = false;

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

    if (FENCE.test(line)) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;

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
    if (bullet !== null && bullet[1].length === 0) {
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
      appendText(subItem !== null && indent > subIndent ? subItem : item, text);
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
  closeGroup();

  const released = [];
  for (const entry of entries) {
    for (const g of entry.groups) {
      g.blocks = g.blocks.filter((block) => !("items" in block) || block.items.length > 0);
    }
    entry.groups = entry.groups.filter((g) => g.blocks.length > 0);
    if (entry.groups.length === 0) {
      onWarning(`CHANGELOG.md: release ${entry.version} has no release notes for users; leaving it out.`);
    } else {
      released.push(entry);
    }
  }
  return released;
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
