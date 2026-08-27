#!/usr/bin/env node
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SECTION_KEYS = {
  Added: "added",
  Changed: "changed",
  Fixed: "fixed",
  Removed: "removed",
};

export function parseChangelog(markdown) {
  const entries = [];
  let current = null;
  let currentSectionKey = null;

  for (const line of markdown.split("\n")) {
    if (/^##\s+\[Unreleased\]/i.test(line)) {
      current = null;
      currentSectionKey = null;
      continue;
    }
    const versionMatch = line.match(/^##\s+\[([^\]]+)\]\s*-\s*(\S+)/);
    if (versionMatch !== null) {
      current = { version: versionMatch[1], date: versionMatch[2], sections: {} };
      entries.push(current);
      currentSectionKey = null;
      continue;
    }
    const sectionMatch = line.match(/^###\s+(Added|Changed|Fixed|Removed)/);
    if (sectionMatch !== null && current !== null) {
      currentSectionKey = SECTION_KEYS[sectionMatch[1]];
      current.sections[currentSectionKey] = [];
      continue;
    }
    const bulletMatch = line.match(/^-\s+(.+)/);
    if (bulletMatch !== null && current !== null && currentSectionKey !== null) {
      current.sections[currentSectionKey].push(bulletMatch[1].trim());
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
  const entries = parseChangelog(markdown);
  mkdirSync(dirname(outPath), { recursive: true });
  writeFileSync(outPath, JSON.stringify(entries, null, 2) + "\n");
  console.log(`Wrote ${entries.length} release notes entries to ${outPath}`);
}
