// Thin, dependency-injected wrappers around isomorphic-git. Every function takes
// { fs, dir } explicitly (isomorphic-git's own argument shape) instead of reaching
// for a module-level singleton, so the exact same code path can be exercised in
// tests against a real temp directory via Node's fs, and in the browser against
// fsaGitFs's File System Access adapter — see git.test.ts.
import * as git from 'isomorphic-git'
import { TREE, WORKDIR, STAGE } from 'isomorphic-git'
import type { WalkerEntry } from 'isomorphic-git'
import type { GitFs } from './fsaGitFs'

// isomorphic-git's fs-client type is intentionally duck-typed; both GitFs and
// Node's fs module satisfy it at runtime. Pull the type straight from
// isomorphic-git itself rather than hand-maintaining a matching interface.
type IsoFs = Parameters<typeof git.statusMatrix>[0]['fs']
function iso(fs: GitFs): IsoFs {
  return fs as unknown as IsoFs
}

function join(dir: string, sub: string): string {
  return dir.endsWith('/') ? `${dir}${sub}` : `${dir}/${sub}`
}

export interface CommitAuthor {
  name: string
  email: string
}

export interface CommitInfo {
  oid: string
  message: string
  author: CommitAuthor & { timestamp: number }
  parents: string[]
}

export interface StatusResult {
  staged: string[]
  unstaged: string[]
  untracked: string[]
}

export interface FileDiff {
  filepath: string
  oldContent: string
  newContent: string
}

export async function openRepository(fs: GitFs, dir: string): Promise<void> {
  try {
    const s = await fs.promises.stat(join(dir, '.git'))
    if (!s.isDirectory()) throw new Error('not a directory')
  } catch {
    throw new Error(`No git repository found at ${dir}`)
  }
}

const FILE = 0
const HEAD = 1
const WORKDIR_COL = 2
const STAGE_COL = 3

export async function getStatus(fs: GitFs, dir: string): Promise<StatusResult> {
  const matrix = await git.statusMatrix({ fs: iso(fs), dir })
  const staged: string[] = []
  const unstaged: string[] = []
  const untracked: string[] = []

  for (const row of matrix) {
    const filepath = row[FILE] as string
    const head = row[HEAD]
    const workdir = row[WORKDIR_COL]
    const stage = row[STAGE_COL]

    if (head === 1 && workdir === 1 && stage === 1) continue // unmodified

    if (head === 0 && stage === 0) {
      untracked.push(filepath)
      continue
    }
    if (stage !== head) staged.push(filepath)
    if (workdir !== stage) unstaged.push(filepath)
  }

  return { staged, unstaged, untracked }
}

export async function getCurrentBranch(fs: GitFs, dir: string): Promise<string | undefined> {
  return (await git.currentBranch({ fs: iso(fs), dir })) ?? undefined
}

function isNotFound(err: unknown): boolean {
  const code = (err as { code?: string })?.code
  return code === 'NotFoundError'
}

export async function getLog(
  fs: GitFs,
  dir: string,
  opts: { depth?: number; ref?: string } = {}
): Promise<CommitInfo[]> {
  try {
    const commits = await git.log({
      fs: iso(fs),
      dir,
      depth: opts.depth ?? 50,
      ref: opts.ref ?? 'HEAD',
    })
    return commits.map((c) => ({
      oid: c.oid,
      message: c.commit.message,
      author: {
        name: c.commit.author.name,
        email: c.commit.author.email,
        timestamp: c.commit.author.timestamp * 1000,
      },
      parents: c.commit.parent,
    }))
  } catch (err) {
    if (isNotFound(err)) return [] // brand new repo with no commits yet
    throw err
  }
}

async function readEntryContent(
  fs: GitFs,
  dir: string,
  entry: WalkerEntry | null
): Promise<string> {
  if (!entry) return ''
  const content = await entry.content()
  if (content) return new TextDecoder().decode(content)
  // The STAGE() walker's entries deliberately don't implement content() (see
  // isomorphic-git's GitWalkerIndex) — fall back to reading the blob straight
  // from the object store by oid, which git.add already wrote there.
  const oid = await entry.oid()
  const { blob } = await git.readBlob({ fs: iso(fs), dir, oid })
  return new TextDecoder().decode(blob)
}

async function entryToDiff(
  fs: GitFs,
  dir: string,
  filepath: string,
  before: WalkerEntry | null,
  after: WalkerEntry | null
): Promise<FileDiff | undefined> {
  const beforeType = before ? await before.type() : undefined
  const afterType = after ? await after.type() : undefined
  // Returning undefined (not null!) excludes this entry from the results
  // without pruning its children — returning null for a tree entry tells
  // isomorphic-git to skip walking everything beneath it, which is not what
  // we want for directories we simply can't diff directly.
  if (beforeType === 'tree' || afterType === 'tree') return undefined

  const beforeOid = before ? await before.oid() : undefined
  const afterOid = after ? await after.oid() : undefined
  if (beforeOid === afterOid) return undefined

  return {
    filepath,
    oldContent: await readEntryContent(fs, dir, before),
    newContent: await readEntryContent(fs, dir, after),
  }
}

// git.walk's default `reduce` aggregation isn't documented precisely enough to
// rely on, so we supply our own: each node's own map() result (`parent`) is
// combined with its already-flattened children into a single flat array that
// bubbles all the way up to the root.
async function reduceDiffs(
  parent: FileDiff | undefined,
  children: (FileDiff | undefined)[][]
): Promise<FileDiff[]> {
  const flatChildren = children.flat().filter((entry): entry is FileDiff => entry !== undefined)
  return parent !== undefined ? [...flatChildren, parent] : flatChildren
}

async function diffTrees(
  fs: GitFs,
  dir: string,
  treeA: ReturnType<typeof TREE>,
  treeB: ReturnType<typeof TREE>,
  onlyPath?: string
): Promise<FileDiff[]> {
  return git.walk({
    fs: iso(fs),
    dir,
    trees: [treeA, treeB],
    map: async (filepath: string, [before, after]: (WalkerEntry | null)[]) => {
      if (filepath === '.') return undefined
      if (onlyPath !== undefined && filepath !== onlyPath) return undefined
      return entryToDiff(fs, dir, filepath, before, after)
    },
    reduce: reduceDiffs,
  })
}

export async function getCommitDiff(fs: GitFs, dir: string, oid: string): Promise<FileDiff[]> {
  const { commit } = await git.readCommit({ fs: iso(fs), dir, oid })
  const parentOid = commit.parent[0]

  if (!parentOid) {
    return git.walk({
      fs: iso(fs),
      dir,
      trees: [TREE({ ref: oid })],
      map: async (filepath: string, [after]: (WalkerEntry | null)[]) => {
        if (filepath === '.') return undefined
        return entryToDiff(fs, dir, filepath, null, after)
      },
      reduce: reduceDiffs,
    })
  }

  return diffTrees(fs, dir, TREE({ ref: parentOid }), TREE({ ref: oid }))
}

export async function getStagedDiff(
  fs: GitFs,
  dir: string,
  filepath?: string
): Promise<FileDiff[]> {
  let headOid: string | null
  try {
    headOid = await git.resolveRef({ fs: iso(fs), dir, ref: 'HEAD' })
  } catch {
    headOid = null // no commits yet
  }

  if (!headOid) {
    return git.walk({
      fs: iso(fs),
      dir,
      trees: [STAGE()],
      map: async (fp: string, [entry]: (WalkerEntry | null)[]) => {
        if (fp === '.') return undefined
        if (filepath !== undefined && fp !== filepath) return undefined
        return entryToDiff(fs, dir, fp, null, entry)
      },
      reduce: reduceDiffs,
    })
  }

  return diffTrees(fs, dir, TREE({ ref: headOid }), STAGE(), filepath)
}

export async function getUnstagedDiff(
  fs: GitFs,
  dir: string,
  filepath?: string
): Promise<FileDiff[]> {
  return diffTrees(fs, dir, STAGE(), WORKDIR(), filepath)
}

export async function stageFile(fs: GitFs, dir: string, filepath: string): Promise<void> {
  try {
    await git.add({ fs: iso(fs), dir, filepath })
  } catch (err) {
    // The file was deleted in the working tree — staging a deletion is `remove`,
    // not `add` (which expects the file to exist on disk).
    const code = (err as { code?: string })?.code
    if (code === 'NotFoundError' || code === 'ENOENT') {
      await git.remove({ fs: iso(fs), dir, filepath })
    } else {
      throw err
    }
  }
}

export async function unstageFile(fs: GitFs, dir: string, filepath: string): Promise<void> {
  await git.resetIndex({ fs: iso(fs), dir, filepath })
}

export async function createCommit(
  fs: GitFs,
  dir: string,
  options: { message: string; author: CommitAuthor }
): Promise<string> {
  return git.commit({ fs: iso(fs), dir, message: options.message, author: options.author })
}

export interface GraphCommit {
  oid: string
  parents: string[]
  message: string
  author: CommitAuthor & { timestamp: number }
  refs: string[]
}

// Builds the commit graph across every local branch (not just the current
// one), for GraphView's DAG rendering. isomorphic-git has no single call that
// returns "all commits reachable from any local branch plus which refs point
// at each", so this composes it from listBranches + currentBranch + one log
// per branch, de-duplicating commits by oid as branches are walked (branches
// sharing history will re-visit the same ancestor commits) and collecting,
// per oid, the names of any refs/branches that point directly at it.
export async function getGraphLog(
  fs: GitFs,
  dir: string,
  opts: { maxCount?: number } = {}
): Promise<GraphCommit[]> {
  const maxCount = opts.maxCount ?? 200

  let branches: string[]
  try {
    branches = await git.listBranches({ fs: iso(fs), dir })
  } catch (err) {
    if (isNotFound(err)) return [] // brand new repo, no branches yet
    throw err
  }

  const current = await git.currentBranch({ fs: iso(fs), dir }).catch(() => undefined)

  const byOid = new Map<string, GraphCommit>()
  const refsByOid = new Map<string, string[]>()

  for (const branch of branches) {
    let tipOid: string
    try {
      tipOid = await git.resolveRef({ fs: iso(fs), dir, ref: branch })
    } catch {
      continue
    }
    const refs = refsByOid.get(tipOid) ?? []
    refs.push(branch)
    if (branch === current) refs.push('HEAD')
    refsByOid.set(tipOid, refs)

    let commits
    try {
      commits = await git.log({ fs: iso(fs), dir, ref: branch, depth: maxCount })
    } catch (err) {
      if (isNotFound(err)) continue
      throw err
    }

    for (const c of commits) {
      if (byOid.has(c.oid)) continue
      byOid.set(c.oid, {
        oid: c.oid,
        parents: c.commit.parent,
        message: c.commit.message,
        author: {
          name: c.commit.author.name,
          email: c.commit.author.email,
          timestamp: c.commit.author.timestamp * 1000,
        },
        refs: [],
      })
    }
  }

  for (const [oid, refs] of refsByOid) {
    const commit = byOid.get(oid)
    if (commit) commit.refs = refs
  }

  return Array.from(byOid.values())
}
