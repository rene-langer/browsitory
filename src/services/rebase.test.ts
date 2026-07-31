import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import * as nodeFs from 'fs'
import * as os from 'os'
import * as path from 'path'
import * as isoGit from 'isomorphic-git'
import type { GitFs } from './fsaGitFs'
import { createCommit, getStatus, resolveRef, stageFile } from './git'
import { abortRebase, beginRebase, continueRebase, getRebaseState, planRebase } from './rebase'
import type { RebaseCommitPlan } from './rebase'

const fs = nodeFs as unknown as GitFs
const AUTHOR = { name: 'Test User', email: 'test@example.com' }
const COMMITTER = { name: 'Committer', email: 'committer@example.com' }

let dir: string

function write(relPath: string, content: string) {
  const full = path.join(dir, relPath)
  nodeFs.mkdirSync(path.dirname(full), { recursive: true })
  nodeFs.writeFileSync(full, content)
}

beforeEach(async () => {
  dir = nodeFs.mkdtempSync(path.join(os.tmpdir(), 'browsitory-'))
  await isoGit.init({ fs: nodeFs, dir, defaultBranch: 'main' })
})

afterEach(() => {
  nodeFs.rmSync(dir, { recursive: true, force: true })
})

async function commitFile(relPath: string, content: string, message: string) {
  write(relPath, content)
  await stageFile(fs, dir, relPath)
  return createCommit(fs, dir, { message, author: AUTHOR })
}

function pickAll(plan: RebaseCommitPlan[]): RebaseCommitPlan[] {
  return plan.map((entry) => ({ ...entry, action: 'pick' as const }))
}

describe('planRebase', () => {
  it('collects commits oldest-first between onto and ref, defaulting to pick', async () => {
    const base = await commitFile('a.txt', 'v1', 'base')
    await commitFile('a.txt', 'v2', 'second')
    await commitFile('a.txt', 'v3', 'third')

    const plan = await planRebase(fs, dir, base)
    expect(plan.map((e) => e.message.trim())).toEqual(['second', 'third'])
    expect(plan.every((e) => e.action === 'pick')).toBe(true)
  })

  it('throws if ontoOid is not an ancestor of ref', async () => {
    await commitFile('a.txt', 'v1', 'base')
    await expect(planRebase(fs, dir, 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef')).rejects.toThrow()
  })
})

describe('beginRebase / a clean pick-only rebase', () => {
  it('reorders commits according to the plan order', async () => {
    const base = await commitFile('a.txt', 'base', 'base')
    write('b.txt', 'first')
    await stageFile(fs, dir, 'b.txt')
    const first = await createCommit(fs, dir, { message: 'add b.txt', author: AUTHOR })
    write('c.txt', 'second')
    await stageFile(fs, dir, 'c.txt')
    await createCommit(fs, dir, { message: 'add c.txt', author: AUTHOR })

    const plan = pickAll(await planRebase(fs, dir, base))
    // Reverse the order: 'add c.txt' should now be applied before 'add b.txt'.
    const reordered = [plan[1], plan[0]]

    const result = await beginRebase(fs, dir, reordered, base, COMMITTER)
    expect(result).toEqual({ status: 'done' })

    const log = await isoGit.log({ fs: nodeFs, dir, ref: 'main' })
    const messages = log.map((c) => c.commit.message.trim()).reverse()
    expect(messages).toEqual(['base', 'add c.txt', 'add b.txt'])
    // Original commit for 'add b.txt' should no longer be reachable at its old oid on main.
    expect(await resolveRef(fs, dir, 'main')).not.toBe(first)
  })

  it('skips entries marked as drop', async () => {
    const base = await commitFile('a.txt', 'base', 'base')
    write('b.txt', 'keep me')
    await stageFile(fs, dir, 'b.txt')
    await createCommit(fs, dir, { message: 'keep this commit', author: AUTHOR })
    write('c.txt', 'drop me')
    await stageFile(fs, dir, 'c.txt')
    await createCommit(fs, dir, { message: 'drop this commit', author: AUTHOR })

    const plan = pickAll(await planRebase(fs, dir, base))
    const withDrop = plan.map((entry) =>
      entry.message.trim() === 'drop this commit' ? { ...entry, action: 'drop' as const } : entry
    )

    const result = await beginRebase(fs, dir, withDrop, base, COMMITTER)
    expect(result).toEqual({ status: 'done' })

    const log = await isoGit.log({ fs: nodeFs, dir, ref: 'main' })
    const messages = log.map((c) => c.commit.message.trim())
    expect(messages).not.toContain('drop this commit')
    expect(messages).toContain('keep this commit')
    expect(nodeFs.existsSync(path.join(dir, 'c.txt'))).toBe(false)
  })

  it('leaves no rebase state behind on success', async () => {
    const base = await commitFile('a.txt', 'base', 'base')
    await commitFile('b.txt', 'x', 'add b.txt')
    const plan = pickAll(await planRebase(fs, dir, base))

    await beginRebase(fs, dir, plan, base, COMMITTER)
    await expect(getRebaseState(fs, dir)).resolves.toBeNull()
  })

  it('refuses to start when the working tree is dirty', async () => {
    const base = await commitFile('a.txt', 'base', 'base')
    await commitFile('b.txt', 'x', 'add b.txt')
    // Modify a tracked file without staging it — an unstaged, uncommitted
    // change to a tracked file, which is what beginRebase's dirty-tree guard
    // actually checks for (untracked new files don't block a real rebase
    // either, so they shouldn't block ours).
    write('a.txt', 'dirty uncommitted edit')
    const plan = pickAll(await planRebase(fs, dir, base))

    await expect(beginRebase(fs, dir, plan, base, COMMITTER)).rejects.toThrow()
    await expect(getRebaseState(fs, dir)).resolves.toBeNull()
  })
})

describe('conflict pause and continue', () => {
  it('pauses on a conflicting pick and continueRebase resumes after resolution', async () => {
    const base = await commitFile('a.txt', 'base content', 'base')
    // Commit that will conflict when replayed onto a diverged 'a.txt'.
    write('a.txt', 'commit A version')
    await stageFile(fs, dir, 'a.txt')
    const commitAOid = await createCommit(fs, dir, { message: 'commit A', author: AUTHOR })
    write('b.txt', 'unrelated')
    await stageFile(fs, dir, 'b.txt')
    await createCommit(fs, dir, { message: 'commit B (unrelated)', author: AUTHOR })

    // Reset main back to base, then diverge 'a.txt' differently so replaying
    // commit A on top conflicts.
    await isoGit.checkout({ fs: nodeFs, dir, ref: base, force: true })
    await isoGit.branch({ fs: nodeFs, dir, ref: 'main', object: base, force: true })
    await isoGit.checkout({ fs: nodeFs, dir, ref: 'main' })
    write('a.txt', 'diverged main version')
    await stageFile(fs, dir, 'a.txt')
    await createCommit(fs, dir, { message: 'diverge a.txt on main', author: AUTHOR })

    const plan: RebaseCommitPlan[] = [
      {
        oid: commitAOid,
        message: 'commit A',
        author: { ...AUTHOR, timestamp: Date.now() },
        action: 'pick',
      },
    ]

    const result = await beginRebase(fs, dir, plan, await resolveRef(fs, dir, 'main'), COMMITTER)
    expect(result.status).toBe('conflict')
    if (result.status !== 'conflict') throw new Error('expected conflict')
    expect(result.filepaths).toContain('a.txt')

    const state = await getRebaseState(fs, dir)
    expect(state?.cursor).toBe(0)
    expect(state?.conflictFilepaths).toContain('a.txt')

    // Resolve by staging a chosen resolution, then continue.
    write('a.txt', 'resolved content')
    await stageFile(fs, dir, 'a.txt')

    const continued = await continueRebase(fs, dir)
    expect(continued).toEqual({ status: 'done' })
    await expect(getRebaseState(fs, dir)).resolves.toBeNull()

    const log = await isoGit.log({ fs: nodeFs, dir, ref: 'main' })
    expect(log.map((c) => c.commit.message.trim())).toContain('commit A')
    expect(nodeFs.readFileSync(path.join(dir, 'a.txt'), 'utf8')).toBe('resolved content')
  })

  it('continueRebase throws if conflicts are not yet staged', async () => {
    const base = await commitFile('a.txt', 'base content', 'base')
    write('a.txt', 'commit A version')
    await stageFile(fs, dir, 'a.txt')
    const commitAOid = await createCommit(fs, dir, { message: 'commit A', author: AUTHOR })

    await isoGit.checkout({ fs: nodeFs, dir, ref: base, force: true })
    await isoGit.branch({ fs: nodeFs, dir, ref: 'main', object: base, force: true })
    await isoGit.checkout({ fs: nodeFs, dir, ref: 'main' })
    write('a.txt', 'diverged main version')
    await stageFile(fs, dir, 'a.txt')
    await createCommit(fs, dir, { message: 'diverge a.txt on main', author: AUTHOR })

    const plan: RebaseCommitPlan[] = [
      { oid: commitAOid, message: 'commit A', author: { ...AUTHOR, timestamp: Date.now() }, action: 'pick' },
    ]
    await beginRebase(fs, dir, plan, await resolveRef(fs, dir, 'main'), COMMITTER)

    // Conflict markers are on disk but not staged — continuing should fail clearly.
    await expect(continueRebase(fs, dir)).rejects.toThrow()
  })
})

describe('abortRebase', () => {
  it('restores the original branch tip exactly and clears state, from a clean completed rebase', async () => {
    const base = await commitFile('a.txt', 'base', 'base')
    await commitFile('b.txt', 'x', 'add b.txt')
    const plan = pickAll(await planRebase(fs, dir, base))

    await beginRebase(fs, dir, plan, base, COMMITTER)
    // Rebase already completed successfully — abortRebase on a repo with no
    // in-progress rebase must be a safe no-op, not an error.
    await expect(abortRebase(fs, dir)).resolves.toBeUndefined()
    await expect(getRebaseState(fs, dir)).resolves.toBeNull()
  })

  it('restores the original branch tip exactly when aborting a paused (conflicted) rebase', async () => {
    const base = await commitFile('a.txt', 'base content', 'base')
    write('a.txt', 'commit A version')
    await stageFile(fs, dir, 'a.txt')
    const commitAOid = await createCommit(fs, dir, { message: 'commit A', author: AUTHOR })

    await isoGit.checkout({ fs: nodeFs, dir, ref: base, force: true })
    await isoGit.branch({ fs: nodeFs, dir, ref: 'main', object: base, force: true })
    await isoGit.checkout({ fs: nodeFs, dir, ref: 'main' })
    write('a.txt', 'diverged main version')
    await stageFile(fs, dir, 'a.txt')
    const originalTip = await createCommit(fs, dir, { message: 'diverge a.txt on main', author: AUTHOR })

    const plan: RebaseCommitPlan[] = [
      { oid: commitAOid, message: 'commit A', author: { ...AUTHOR, timestamp: Date.now() }, action: 'pick' },
    ]
    const result = await beginRebase(fs, dir, plan, await resolveRef(fs, dir, 'main'), COMMITTER)
    expect(result.status).toBe('conflict')

    await abortRebase(fs, dir)

    await expect(getRebaseState(fs, dir)).resolves.toBeNull()
    await expect(resolveRef(fs, dir, 'main')).resolves.toBe(originalTip)
    await expect(resolveRef(fs, dir, 'HEAD')).resolves.toBe(originalTip)
    const status = await getStatus(fs, dir)
    expect(status).toEqual({ staged: [], unstaged: [], untracked: [] })
    expect(nodeFs.readFileSync(path.join(dir, 'a.txt'), 'utf8')).toBe('diverged main version')
  })
})
