// Hand-rolled blame: isomorphic-git has no native `blame` command (confirmed by
// reading its source), so this walks the file's commit history itself. Same
// dependency-injected `(fs, dir, ...)` shape as git.ts so it can be tested
// against a real temp-directory repo instead of mocking isomorphic-git.
import * as git from 'isomorphic-git'
import { diffLines } from 'diff'
import type { GitFs } from './fsaGitFs'
import type { CommitInfo } from './git'

type IsoFs = Parameters<typeof git.log>[0]['fs']
function iso(fs: GitFs): IsoFs {
  return fs as unknown as IsoFs
}

export interface BlameLine {
  lineNumber: number
  content: string
  commit: CommitInfo
}

interface AttributedLine {
  line: string
  commit: CommitInfo
}

function toCommitInfo(c: Awaited<ReturnType<typeof git.log>>[number]): CommitInfo {
  return {
    oid: c.oid,
    message: c.commit.message,
    author: {
      name: c.commit.author.name,
      email: c.commit.author.email,
      timestamp: c.commit.author.timestamp * 1000,
    },
    parents: c.commit.parent,
  }
}

// Splits file content into logical lines, dropping a single trailing newline
// (matching how git counts lines — a trailing "\n" doesn't create a phantom
// empty final line).
function toLines(content: string): string[] {
  const trimmed = content.endsWith('\n') ? content.slice(0, -1) : content
  return trimmed === '' ? [] : trimmed.split('\n')
}

/**
 * Attributes each line of `filepath` (as of `ref`, default HEAD) to the commit
 * that last touched it.
 *
 * Algorithm: `git.log({ filepath })` already restricts the commit list to
 * commits that actually touched the file (equivalent to `git log -- path`),
 * so no manual per-commit diffing against the whole tree is needed to build
 * the candidate list. Walk that list oldest -> newest, maintaining a running
 * "attributed lines" snapshot of the file's content tagged with the commit
 * that last set each line. For each commit, diff the running snapshot against
 * that commit's actual content with `diffLines`, then walk the resulting
 * chunks in order: unchanged/removed chunks consume entries from the
 * *existing* attributed array and keep their original commit (this is what
 * makes it "last touched", not "any commit that ever touched the file"),
 * while added chunks are tagged with the current commit.
 *
 * Performance: this is O(commits touching the file x diffLines cost) with no
 * incremental/paginated optimization. That's fine for MVP-scale local repos;
 * it would not scale to an enterprise monorepo file with thousands of
 * touching commits.
 *
 * Scope cuts: no rename-following (`follow` is intentionally not passed to
 * `git.log`, matching plain `git blame`'s default rather than `--follow`),
 * and blame is against `ref`'s last-committed content, not live working-tree
 * edits (also matching real git's default). If `readBlob` fails for some
 * historical commit in the touching list (e.g. a delete/recreate edge case),
 * that commit is skipped defensively rather than modeling file resurrection.
 */
export async function getBlame(
  fs: GitFs,
  dir: string,
  filepath: string,
  ref = 'HEAD'
): Promise<BlameLine[]> {
  const rawCommits = await git.log({ fs: iso(fs), dir, ref, filepath })
  const commits = rawCommits.map(toCommitInfo).reverse() // oldest -> newest

  let attributed: AttributedLine[] = []

  for (const commit of commits) {
    let content: string
    try {
      const { blob } = await git.readBlob({ fs: iso(fs), dir, oid: commit.oid, filepath })
      content = new TextDecoder().decode(blob)
    } catch {
      continue
    }

    const newLines = toLines(content)
    // Both sides must end with a trailing "\n" (not just be joined with "\n"
    // between lines) — otherwise the *last* line's token lacks a trailing
    // newline on one side but not the other whenever line counts differ
    // between old and new, which makes jsdiff's line-level tokenizer treat
    // an unchanged last line as a distinct token and misreport it as a
    // remove+add instead of unchanged.
    const oldText = attributed.length ? `${attributed.map((a) => a.line).join('\n')}\n` : ''
    const newText = newLines.length ? `${newLines.join('\n')}\n` : ''
    const changes = diffLines(oldText, newText)

    const next: AttributedLine[] = []
    let oldIdx = 0
    let newIdx = 0
    for (const change of changes) {
      const count = change.count ?? 0
      if (change.removed) {
        oldIdx += count
      } else if (change.added) {
        for (let i = 0; i < count; i++) {
          next.push({ line: newLines[newIdx + i], commit })
        }
        newIdx += count
      } else {
        for (let i = 0; i < count; i++) {
          next.push(attributed[oldIdx + i])
        }
        oldIdx += count
        newIdx += count
      }
    }
    attributed = next
  }

  return attributed.map((a, i) => ({
    lineNumber: i + 1,
    content: a.line,
    commit: a.commit,
  }))
}
