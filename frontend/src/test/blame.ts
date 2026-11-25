import type { Blame, BlameCommit } from "../ipc/RepoClient";

/** A blamed line with its commit written out, as tests list them; `lineNumber` is its place. */
export interface BlameRow extends BlameCommit {
  lineNumber?: number;
  content: string;
}

/** The `Blame` of `rows` (#1158): each commit once, in the order the rows first name it. */
export function blameOf(rows: BlameRow[]): Blame {
  const commits: BlameCommit[] = [];
  const index = new Map<string, number>();
  const lines = rows.map(({ content, commitId, shortId, authorName, timestamp }) => {
    let commit = index.get(commitId);
    if (commit === undefined) {
      commit = commits.length;
      index.set(commitId, commit);
      commits.push({ commitId, shortId, authorName, timestamp });
    }
    return { content, commit };
  });
  return { commits, lines };
}

/** The blame of an empty file. */
export const NO_BLAME: Blame = { commits: [], lines: [] };
