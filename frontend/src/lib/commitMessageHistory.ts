import type { GraphCommit } from "../ipc/RepoClient";

// Per repo, per machine: a UI convenience, not repository state. Same mechanism as the per-repo
// Prune choice (`fetchPrune.ts`), so it works in both frontends.
const storageKey = (repoPath: string) => `commit-messages:${repoPath}`;

/** How many messages committed through the app are kept per repository. */
export const RECORDED_MESSAGE_LIMIT = 20;
/** How many of the current branch's recent commits the history offers. */
export const BRANCH_MESSAGE_LIMIT = 10;

/** Messages committed (or amended) through the app in this repository, newest first. */
export function readCommitMessages(repoPath: string): string[] {
  try {
    const stored = localStorage.getItem(storageKey(repoPath));
    if (stored === null) return [];
    const parsed: unknown = JSON.parse(stored);
    return Array.isArray(parsed) ? parsed.filter((m): m is string => typeof m === "string" && m.trim() !== "") : [];
  } catch {
    return [];
  }
}

/**
 * Puts `message` at the front of the repository's history, dropping an earlier copy of it (the
 * same text up to surrounding whitespace) and anything past `RECORDED_MESSAGE_LIMIT`.
 */
export function recordCommitMessage(repoPath: string, message: string): void {
  const text = message.trim();
  if (text === "") return;
  const next = [text, ...readCommitMessages(repoPath).filter((m) => m.trim() !== text)].slice(
    0,
    RECORDED_MESSAGE_LIMIT,
  );
  try {
    localStorage.setItem(storageKey(repoPath), JSON.stringify(next));
  } catch {
    // Blocked or full storage: the history just doesn't grow.
  }
}

/**
 * The current branch's most recent commits in the loaded graph: HEAD, then its first parents,
 * while they are loaded. Merges' other parents belong to other branches and are skipped.
 */
export function recentBranchCommits(
  commits: GraphCommit[],
  headId: string | null,
  limit = BRANCH_MESSAGE_LIMIT,
): GraphCommit[] {
  const byId = new Map(commits.map((c) => [c.id, c]));
  const result: GraphCommit[] = [];
  let next = headId === null ? undefined : byId.get(headId);
  while (next !== undefined && result.length < limit) {
    result.push(next);
    const parent = next.parentIds[0];
    next = parent === undefined ? undefined : byId.get(parent);
  }
  return result;
}

/** One entry of the commit message box's history menu. */
export interface MessageHistoryEntry {
  /** The message's first line (what the menu shows). */
  summary: string;
  /** Where it comes from: committed through the app, or a recent commit on this branch. */
  source: "recorded" | "branch";
  /** The commit's short id, for a branch entry. */
  shortId?: string;
  /** The full message; a branch entry's is loaded when picked (the graph holds summaries only). */
  load: () => Promise<string>;
}

export function summaryOf(message: string): string {
  return message.trim().split(/\r?\n/, 1)[0] ?? "";
}

/**
 * The history menu's entries: recorded messages first, then the branch's commits whose summary
 * no recorded message (or earlier entry) already shows, so each summary appears once.
 */
export function messageHistoryEntries(
  recorded: string[],
  branch: GraphCommit[],
  loadMessage: (commitId: string) => Promise<string>,
): MessageHistoryEntry[] {
  const seen = new Set<string>();
  const entries: MessageHistoryEntry[] = [];
  for (const message of recorded) {
    const summary = summaryOf(message);
    if (summary === "" || seen.has(summary)) continue;
    seen.add(summary);
    entries.push({ summary, source: "recorded", load: () => Promise.resolve(message) });
  }
  for (const commit of branch) {
    const summary = commit.summary.trim();
    if (summary === "" || seen.has(summary)) continue;
    seen.add(summary);
    entries.push({ summary, source: "branch", shortId: commit.shortId, load: () => loadMessage(commit.id) });
  }
  return entries;
}
