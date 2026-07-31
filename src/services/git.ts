// Thin, dependency-injected wrappers around isomorphic-git. Every function takes
// { fs, dir } explicitly (isomorphic-git's own argument shape) instead of reaching
// for a module-level singleton, so the exact same code path can be exercised in
// tests against a real temp directory via Node's fs, and in the browser against
// fsaGitFs's File System Access adapter — see git.test.ts.
import * as git from 'isomorphic-git'
import { TREE, WORKDIR, STAGE } from 'isomorphic-git'
import type { WalkerEntry } from 'isomorphic-git'
import { GitIndexManager } from 'isomorphic-git/managers'
import { FileSystem } from 'isomorphic-git/models'
import type { GitFs } from './fsaGitFs'

// isomorphic-git's fs-client type is intentionally duck-typed; both GitFs and
// Node's fs module satisfy it at runtime. Pull the type straight from
// isomorphic-git itself rather than hand-maintaining a matching interface.
type IsoFs = Parameters<typeof git.statusMatrix>[0]['fs']
function iso(fs: GitFs): IsoFs {
  return fs as unknown as IsoFs
}

// isomorphic-git's `cache` option is how GitIndexManager memoizes the parsed
// .git/index across calls — passing none (the default on every call site
// below, before this existed) means EVERY statusMatrix/log/walk/etc. call
// re-reads and re-parses the index from disk from scratch, and — far more
// expensive — throws away the "racy git" stat-comparison fast path that lets
// isomorphic-git skip re-hashing a file's content when its stat metadata
// already matches the index. That fast path was already compromised by our
// adapter (see fsaGitFs.ts — the File System Access API can't expose real
// uid/gid/inode, so those fields are always 0), but isomorphic-git
// self-heals the index with whatever stat values *did* get used once a
// file's content-hash is confirmed unchanged — and that healing only helps
// on a SUBSEQUENT call if the same in-memory cache (and thus parsed index)
// is reused. Without this, every single call pays the full "hash every
// tracked file's content" cost — measured live at ~18s for the first
// statusMatrix on a real ~50-commit repo, only dropping to ~8s on a second,
// separate page load (disk-persisted healing helping some, but each call
// still re-parsing the index from scratch). One cache object per `fs`
// instance (i.e. per opened repository, since fsaGitFs.createFsaGitFs is
// called once per repo-open and that same object is reused for the whole
// session) lets the healing compound within a session instead of restarting
// every time.
const cacheByFs = new WeakMap<GitFs, object>()
function getCache(fs: GitFs): object {
  let cache = cacheByFs.get(fs)
  if (!cache) {
    cache = {}
    cacheByFs.set(fs, cache)
  }
  return cache
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
  const matrix = await git.statusMatrix({ fs: iso(fs), dir, cache: getCache(fs) })
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
      cache: getCache(fs),
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
  const { blob } = await git.readBlob({ fs: iso(fs), dir, oid, cache: getCache(fs) })
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
    cache: getCache(fs),
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
  const { commit } = await git.readCommit({ fs: iso(fs), dir, oid, cache: getCache(fs) })
  const parentOid = commit.parent[0]

  if (!parentOid) {
    return git.walk({
      fs: iso(fs),
      dir,
      cache: getCache(fs),
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
      cache: getCache(fs),
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
    await git.add({ fs: iso(fs), dir, filepath, cache: getCache(fs) })
  } catch (err) {
    // The file was deleted in the working tree — staging a deletion is `remove`,
    // not `add` (which expects the file to exist on disk).
    const code = (err as { code?: string })?.code
    if (code === 'NotFoundError' || code === 'ENOENT') {
      await git.remove({ fs: iso(fs), dir, filepath, cache: getCache(fs) })
    } else {
      throw err
    }
  }
}

export async function unstageFile(fs: GitFs, dir: string, filepath: string): Promise<void> {
  await git.resetIndex({ fs: iso(fs), dir, filepath, cache: getCache(fs) })
}

export async function createCommit(
  fs: GitFs,
  dir: string,
  options: { message: string; author: CommitAuthor }
): Promise<string> {
  return git.commit({
    fs: iso(fs),
    dir,
    message: options.message,
    author: options.author,
    cache: getCache(fs),
  })
}

// --- Branches --------------------------------------------------------------

export interface BranchInfo {
  name: string
  oid: string
  isCurrent: boolean
}

export async function listAllBranches(fs: GitFs, dir: string): Promise<BranchInfo[]> {
  const [names, current] = await Promise.all([
    git.listBranches({ fs: iso(fs), dir }),
    getCurrentBranch(fs, dir),
  ])
  return Promise.all(
    names.map(async (name) => ({
      name,
      oid: await git.resolveRef({ fs: iso(fs), dir, ref: name }),
      isCurrent: name === current,
    }))
  )
}

export async function createBranch(
  fs: GitFs,
  dir: string,
  name: string,
  startPoint?: string
): Promise<void> {
  await git.branch({
    fs: iso(fs),
    dir,
    ref: name,
    object: startPoint,
    checkout: false,
  })
}

// isomorphic-git's deleteBranch happily deletes the current branch (it just
// detaches HEAD at the branch's current commit instead of refusing) — real
// git refuses outright, so we replicate that guard here rather than leaving
// the user with a detached HEAD they didn't ask for.
export async function deleteBranchByName(fs: GitFs, dir: string, name: string): Promise<void> {
  const current = await getCurrentBranch(fs, dir)
  if (current === name) {
    throw new Error(`Cannot delete branch "${name}" because it is currently checked out.`)
  }
  await git.deleteBranch({ fs: iso(fs), dir, ref: name })
}

export async function renameBranchTo(
  fs: GitFs,
  dir: string,
  oldName: string,
  newName: string
): Promise<void> {
  // renameBranch already updates HEAD itself when the renamed branch is the
  // current one, so no extra `checkout` flag or manual HEAD handling needed.
  await git.renameBranch({ fs: iso(fs), dir, ref: newName, oldref: oldName })
}

// isomorphic-git's checkout() already refuses (throws CheckoutConflictError)
// when uncommitted working-tree/staged changes would be overwritten by the
// target branch's tree, as long as `force` isn't passed — verified by reading
// `analyze()` in isomorphic-git's checkout implementation, which walks
// TREE/WORKDIR/STAGE and reports a 'conflict' entry for any path whose
// working copy differs from both the stage and the incoming commit. So no
// extra dirty-check is needed here; we just re-throw with a friendlier
// message pointing the user at commit/stash/discard, mirroring real git's UX.
export async function switchBranch(fs: GitFs, dir: string, name: string): Promise<void> {
  try {
    await git.checkout({ fs: iso(fs), dir, ref: name, cache: getCache(fs) })
  } catch (err) {
    const code = (err as { code?: string })?.code
    if (code === 'CheckoutConflictError') {
      throw new Error(
        `Cannot switch to "${name}": local changes would be overwritten. Commit, stash, or discard them first.`
      )
    }
    throw err
  }
}

export async function resolveRef(fs: GitFs, dir: string, ref: string): Promise<string> {
  return git.resolveRef({ fs: iso(fs), dir, ref })
}

// Simple name-only listing used by merge/rebase, which only need branch
// names (not the richer BranchInfo above) and are deliberately independent
// of the branch-management workstream's data model.
export async function listBranches(fs: GitFs, dir: string): Promise<string[]> {
  return git.listBranches({ fs: iso(fs), dir })
}

// --- Stash -------------------------------------------------------------

export interface StashEntry {
  index: number
  message: string
}

// git.stash({op:'list'}) returns string[] formatted as "stash@{N}: <message>"
// (see isomorphic-git's GitRefStash.getStashReflogEntry with parsed=true) —
// not an array of objects, despite the generic `Promise<string | void>`
// return type declared for `stash()` overall. Parse that format ourselves.
const STASH_LIST_ENTRY = /^stash@\{(\d+)\}: (.*)$/

export async function listStashes(fs: GitFs, dir: string): Promise<StashEntry[]> {
  const entries = (await git.stash({ fs: iso(fs), dir, op: 'list' })) as unknown as
    | string[]
    | undefined
  if (!entries) return []
  return entries.map((entry, i) => {
    const match = STASH_LIST_ENTRY.exec(entry)
    return match ? { index: Number(match[1]), message: match[2] } : { index: i, message: entry }
  })
}

export async function createStash(fs: GitFs, dir: string, message?: string): Promise<void> {
  await git.stash({ fs: iso(fs), dir, op: 'push', message: message ?? '' })
}

export async function applyStash(fs: GitFs, dir: string, refIdx = 0): Promise<void> {
  await git.stash({ fs: iso(fs), dir, op: 'apply', refIdx })
}

export async function popStash(fs: GitFs, dir: string, refIdx = 0): Promise<void> {
  await git.stash({ fs: iso(fs), dir, op: 'pop', refIdx })
}

export async function dropStash(fs: GitFs, dir: string, refIdx = 0): Promise<void> {
  await git.stash({ fs: iso(fs), dir, op: 'drop', refIdx })
}

// --- Merge ---------------------------------------------------------------

export interface MergeResult {
  status: 'fast-forward' | 'already-up-to-date' | 'merged' | 'conflict'
  conflicts?: string[]
}

function errorCode(err: unknown): string | undefined {
  return (err as { code?: string })?.code
}

export async function mergeBranch(
  fs: GitFs,
  dir: string,
  theirs: string,
  author: CommitAuthor
): Promise<MergeResult> {
  try {
    const result = await git.merge({
      fs: iso(fs),
      dir,
      theirs,
      author,
      abortOnConflict: false,
      cache: getCache(fs),
    })

    if (result.alreadyMerged) return { status: 'already-up-to-date' }

    // Confirmed by reading isomorphic-git's _merge()/mergeTree() source
    // directly: on a successful fast-forward OR a clean three-way merge,
    // git.merge() only moves the branch ref / writes objects — it never
    // syncs the working tree or index to match (mergeTree only writes files
    // to disk in the conflict case). An explicit checkout of the
    // now-updated current branch is required to bring the workdir back in
    // sync; without it the UI would show a moved HEAD with stale file
    // contents on screen.
    const ours = await git.currentBranch({ fs: iso(fs), dir })
    if (ours) {
      await git.checkout({ fs: iso(fs), dir, ref: ours, force: true, cache: getCache(fs) })
    }

    return result.fastForward ? { status: 'fast-forward' } : { status: 'merged' }
  } catch (err) {
    if (errorCode(err) === 'MergeConflictError') {
      const filepaths = (err as { data?: { filepaths?: string[] } }).data?.filepaths ?? []
      return { status: 'conflict', conflicts: filepaths }
    }
    throw err
  }
}

export async function abortCurrentMerge(fs: GitFs, dir: string): Promise<void> {
  await git.abortMerge({ fs: iso(fs), dir, cache: getCache(fs) })
}

async function readBlobText(fs: GitFs, dir: string, oid: string | undefined): Promise<string> {
  if (!oid) return ''
  const { blob } = await git.readBlob({ fs: iso(fs), dir, oid, cache: getCache(fs) })
  return new TextDecoder().decode(blob)
}

// A conflicted path has index entries at stage 1 (common ancestor/base),
// stage 2 (ours), and stage 3 (theirs) instead of the single stage 0 entry
// a resolved path has. isomorphic-git's public API has no function to read
// these directly — the STAGE() walker used elsewhere in this file
// (readEntryContent()) only ever surfaces one representative entry per
// path, not each conflict side individually.
//
// Read directly from isomorphic-git/models/index.js: `git.commit`,
// `git.merge`, etc. all wrap the raw fs client in an internal `FileSystem`
// class before touching the index, and the low-level `GitIndexManager`
// (from isomorphic-git's separate `isomorphic-git/managers` entry point,
// which *is* part of the package's public API surface even though the
// top-level `isomorphic-git` export doesn't re-export it) expects that
// wrapped `FileSystem`, not a raw fs client — confirmed empirically, a raw
// fs client crashes inside `GitIndexManager.acquire` because it calls
// `fs.lstat(...)` without a callback (Node's raw fs.lstat is
// callback-based; FileSystem's wrapped .lstat is a promise).
//
// Once acquired, `index.entriesMap.get(filepath)` returns an entry whose
// `.stages` array is sparse-indexed by git's stage number: `stages[1]` is
// the base, `stages[2]` is ours, `stages[3]` is theirs. Each stage entry
// carries a plain blob `.oid` that `readBlob` can resolve directly.
export async function getConflictDiff(
  fs: GitFs,
  dir: string,
  filepath: string
): Promise<{ ours: FileDiff; theirs: FileDiff }> {
  const gitdir = join(dir, '.git')
  const wrappedFs = new FileSystem(iso(fs))

  const entry = await GitIndexManager.acquire(
    { fs: wrappedFs, gitdir, cache: getCache(fs) },
    (index) => index.entriesMap.get(filepath)
  )

  const stages = entry?.stages as
    | Array<{ oid: string } | undefined | null>
    | undefined
  const hasConflict = stages !== undefined && (stages[2] !== undefined || stages[3] !== undefined)
  if (!hasConflict) {
    throw new Error(`No merge conflict found for ${filepath}`)
  }

  const [baseContent, oursContent, theirsContent] = await Promise.all([
    readBlobText(fs, dir, stages[1]?.oid),
    readBlobText(fs, dir, stages[2]?.oid),
    readBlobText(fs, dir, stages[3]?.oid),
  ])

  return {
    ours: { filepath, oldContent: baseContent, newContent: oursContent },
    theirs: { filepath, oldContent: baseContent, newContent: theirsContent },
  }
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
      commits = await git.log({
        fs: iso(fs),
        dir,
        ref: branch,
        depth: maxCount,
        cache: getCache(fs),
      })
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
