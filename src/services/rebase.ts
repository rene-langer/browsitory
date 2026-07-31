// Interactive rebase (v1) — deliberate scope cuts, decided up front:
//
// - Only `pick` and `drop` actions. `reword` and `squash` are explicitly OUT
//   of scope for v1 (see RebaseCommitPlan['action']).
// - `onto` must be an ancestor of the branch being rebased — rebasing onto a
//   diverged branch (which would require a real merge-base computation and
//   possibly conflicts on commits that were never conflicting before) is not
//   supported. planRebase() throws a clear error if `onto` isn't found while
//   walking the branch's ancestry.
// - The branch being rebased must be a named branch (not detached HEAD) —
//   beginRebase() throws otherwise.
// - The working tree must be clean before starting — mirrors real git and
//   avoids the much harder problem of interleaving "rebase in progress" with
//   "uncommitted changes in progress".
// - Only one rebase may be in progress per repo at a time (tracked by the
//   sidecar state file below).
// - No in-app conflict editor — this app has no file-editing capability at
//   all. Conflict resolution is: show which files conflict plus a read-only
//   ours/theirs diff (via git.ts's getConflictDiff, which works identically
//   here since cherry-pick's conflict index layout is the same
//   stage-1/2/3-base/ours/theirs shape merge uses), the user edits the real
//   on-disk files with their own OS editor/file manager (this just works —
//   it's the File System Access API operating on real files), then
//   "mark resolved" (stageFile) + "continue" (continueRebase) in-app.
//
// State is persisted to a JSON sidecar file at `.git/browsitory-rebase.json`
// via the same injected `fs` parameter every other function in this module
// takes, so a rebase survives a page reload and the exact same code path is
// exercised in tests (real temp-directory repos) and in the browser
// (fsaGitFs). The branch ref itself is never touched until the rebase
// completes successfully — every intermediate step happens in detached HEAD
// against `ontoOid`, which is what makes abortRebase() a trivial checkout
// back to the original branch tip.
import * as git from 'isomorphic-git'
import type { GitFs } from './fsaGitFs'
import { getStatus, type CommitAuthor } from './git'

type IsoFs = Parameters<typeof git.log>[0]['fs']
function iso(fs: GitFs): IsoFs {
  return fs as unknown as IsoFs
}

function join(dir: string, sub: string): string {
  return dir.endsWith('/') ? `${dir}${sub}` : `${dir}/${sub}`
}

const STATE_FILE = '.git/browsitory-rebase.json'

function statePath(dir: string): string {
  return join(dir, STATE_FILE)
}

export interface RebaseCommitPlan {
  oid: string
  message: string
  author: CommitAuthor & { timestamp: number }
  action: 'pick' | 'drop'
}

export interface PersistedRebaseState {
  branch: string
  originalTip: string
  ontoOid: string
  committer: CommitAuthor
  /** Oldest-first; array order IS the new commit order. */
  plan: RebaseCommitPlan[]
  cursor: number
  conflictFilepaths?: string[]
}

export type RebaseResult = { status: 'done' } | { status: 'conflict'; oid: string; filepaths: string[] }

function errorCode(err: unknown): string | undefined {
  return (err as { code?: string })?.code
}

async function readState(fs: GitFs, dir: string): Promise<PersistedRebaseState | null> {
  try {
    const raw = await fs.promises.readFile(statePath(dir), 'utf8')
    const text = typeof raw === 'string' ? raw : new TextDecoder().decode(raw)
    return JSON.parse(text) as PersistedRebaseState
  } catch (err) {
    if (errorCode(err) === 'ENOENT' || errorCode(err) === 'NotFoundError') return null
    throw err
  }
}

async function writeState(fs: GitFs, dir: string, state: PersistedRebaseState): Promise<void> {
  await fs.promises.writeFile(statePath(dir), JSON.stringify(state, null, 2))
}

async function deleteState(fs: GitFs, dir: string): Promise<void> {
  try {
    await fs.promises.unlink(statePath(dir))
  } catch (err) {
    if (errorCode(err) !== 'ENOENT' && errorCode(err) !== 'NotFoundError') throw err
  }
}

/** Non-throwing — drives "resume paused rebase" UI on repo load. */
export async function getRebaseState(fs: GitFs, dir: string): Promise<PersistedRebaseState | null> {
  return readState(fs, dir)
}

export async function planRebase(
  fs: GitFs,
  dir: string,
  ontoOid: string,
  ref = 'HEAD'
): Promise<RebaseCommitPlan[]> {
  // No depth cap here (unlike git.ts's getLog wrapper, which defaults to 50)
  // — we need the full ancestry to reliably find ontoOid, so call
  // isomorphic-git's log directly.
  const commits = await git.log({ fs: iso(fs), dir, ref })

  const collected: RebaseCommitPlan[] = []
  let found = false
  for (const c of commits) {
    if (c.oid === ontoOid) {
      found = true
      break
    }
    collected.push({
      oid: c.oid,
      message: c.commit.message,
      author: {
        name: c.commit.author.name,
        email: c.commit.author.email,
        timestamp: c.commit.author.timestamp * 1000,
      },
      action: 'pick',
    })
  }

  if (!found) {
    throw new Error(
      `${ontoOid} is not an ancestor of ${ref} — rebasing onto a diverged commit is not supported.`
    )
  }

  return collected.reverse() // oldest-first
}

async function runSteps(fs: GitFs, dir: string, state: PersistedRebaseState): Promise<RebaseResult> {
  while (state.cursor < state.plan.length) {
    const entry = state.plan[state.cursor]

    if (entry.action === 'drop') {
      state.cursor += 1
      state.conflictFilepaths = undefined
      await writeState(fs, dir, state)
      continue
    }

    try {
      await git.cherryPick({
        fs: iso(fs),
        dir,
        oid: entry.oid,
        committer: state.committer,
        abortOnConflict: false,
      })
    } catch (err) {
      if (errorCode(err) === 'MergeConflictError') {
        const filepaths = (err as { data?: { filepaths?: string[] } }).data?.filepaths ?? []
        state.conflictFilepaths = filepaths
        await writeState(fs, dir, state)
        return { status: 'conflict', oid: entry.oid, filepaths }
      }
      throw err
    }

    state.cursor += 1
    state.conflictFilepaths = undefined
    await writeState(fs, dir, state)
  }

  const newTip = await git.resolveRef({ fs: iso(fs), dir, ref: 'HEAD' })
  await git.branch({ fs: iso(fs), dir, ref: state.branch, object: newTip, force: true })
  await git.checkout({ fs: iso(fs), dir, ref: state.branch })
  await deleteState(fs, dir)
  return { status: 'done' }
}

export async function beginRebase(
  fs: GitFs,
  dir: string,
  plan: RebaseCommitPlan[],
  ontoOid: string,
  committer: CommitAuthor
): Promise<RebaseResult> {
  if ((await getRebaseState(fs, dir)) !== null) {
    throw new Error('A rebase is already in progress. Continue or abort it first.')
  }

  const status = await getStatus(fs, dir)
  if (status.staged.length > 0 || status.unstaged.length > 0) {
    throw new Error(
      'The working tree has uncommitted changes. Commit or discard them before rebasing.'
    )
  }

  const originalTip = await git.resolveRef({ fs: iso(fs), dir, ref: 'HEAD' })
  const branch = await git.currentBranch({ fs: iso(fs), dir })
  if (!branch) {
    throw new Error('Cannot rebase in a detached HEAD state — checkout a branch first.')
  }

  await git.checkout({ fs: iso(fs), dir, ref: ontoOid, force: true })

  const state: PersistedRebaseState = {
    branch,
    originalTip,
    ontoOid,
    committer,
    plan,
    cursor: 0,
  }
  await writeState(fs, dir, state)

  return runSteps(fs, dir, state)
}

export async function continueRebase(fs: GitFs, dir: string): Promise<RebaseResult> {
  const state = await readState(fs, dir)
  if (!state) throw new Error('No rebase in progress.')
  if (!state.conflictFilepaths || state.conflictFilepaths.length === 0) {
    throw new Error('No conflict to continue from.')
  }

  const entry = state.plan[state.cursor]

  try {
    // The failed cherry-pick never moved HEAD, so committing now against the
    // current (unmerged -> now-resolved) index reproduces exactly what the
    // cherry-pick would have committed. isomorphic-git's commit author
    // timestamp is seconds-since-epoch (confirmed by reading
    // normalizeAuthorObject's `Math.floor(Date.now() / 1000)` default and
    // verifying empirically that no ms->s conversion happens internally),
    // while RebaseCommitPlan.author.timestamp is milliseconds (matching
    // CommitInfo's convention elsewhere in this file/getLog) — convert back.
    await git.commit({
      fs: iso(fs),
      dir,
      message: entry.message,
      author: {
        name: entry.author.name,
        email: entry.author.email,
        timestamp: Math.round(entry.author.timestamp / 1000),
      },
      committer: state.committer,
    })
  } catch (err) {
    if (errorCode(err) === 'UnmergedPathsError') {
      throw new Error('Resolve and stage all conflicted files before continuing.')
    }
    throw err
  }

  state.cursor += 1
  state.conflictFilepaths = undefined
  await writeState(fs, dir, state)

  return runSteps(fs, dir, state)
}

export async function abortRebase(fs: GitFs, dir: string): Promise<void> {
  const state = await readState(fs, dir)
  if (!state) return
  // The branch ref is never touched until final success, so aborting is just
  // discarding the detached-HEAD attempt and going back to the real branch.
  await git.checkout({ fs: iso(fs), dir, ref: state.branch, force: true })
  await deleteState(fs, dir)
}
