import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import * as nodeFs from 'fs'
import * as os from 'os'
import * as path from 'path'
import * as isoGit from 'isomorphic-git'
import type { GitFs } from './fsaGitFs'
import {
  abortCurrentMerge,
  createCommit,
  getConflictDiff,
  getStatus,
  listBranches,
  mergeBranch,
  resolveRef,
  stageFile,
} from './git'

// Same real-temp-repo pattern as git.test.ts — exercises the exact code path
// the browser uses, just backed by Node's fs instead of the FSA adapter.
const fs = nodeFs as unknown as GitFs
const AUTHOR = { name: 'Test User', email: 'test@example.com' }

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

describe('resolveRef / listBranches', () => {
  it('resolves HEAD and a branch name to their oids', async () => {
    const oid = await commitFile('a.txt', 'hello', 'initial')
    await expect(resolveRef(fs, dir, 'HEAD')).resolves.toBe(oid)
    await expect(resolveRef(fs, dir, 'main')).resolves.toBe(oid)
  })

  it('lists all local branches', async () => {
    await commitFile('a.txt', 'hello', 'initial')
    await isoGit.branch({ fs: nodeFs, dir, ref: 'feature' })
    await expect(listBranches(fs, dir)).resolves.toEqual(expect.arrayContaining(['main', 'feature']))
  })
})

describe('mergeBranch', () => {
  it('reports already-up-to-date when the branches are identical', async () => {
    await commitFile('a.txt', 'hello', 'initial')
    await isoGit.branch({ fs: nodeFs, dir, ref: 'feature' })
    const result = await mergeBranch(fs, dir, 'feature', AUTHOR)
    expect(result).toEqual({ status: 'already-up-to-date' })
  })

  it('fast-forwards when the target is a direct descendant', async () => {
    await commitFile('a.txt', 'v1', 'initial')
    await isoGit.branch({ fs: nodeFs, dir, ref: 'feature', checkout: true })
    await commitFile('b.txt', 'new file', 'add b.txt on feature')
    await isoGit.checkout({ fs: nodeFs, dir, ref: 'main' })

    const result = await mergeBranch(fs, dir, 'feature', AUTHOR)
    expect(result).toEqual({ status: 'fast-forward' })
    expect(nodeFs.existsSync(path.join(dir, 'b.txt'))).toBe(true)
  })

  it('creates a merge commit for a clean three-way merge and syncs the working tree', async () => {
    await commitFile('a.txt', 'v1', 'initial')
    await isoGit.branch({ fs: nodeFs, dir, ref: 'feature', checkout: true })
    await commitFile('b.txt', 'from feature', 'add b.txt on feature')
    await isoGit.checkout({ fs: nodeFs, dir, ref: 'main' })
    await commitFile('c.txt', 'from main', 'add c.txt on main')

    const result = await mergeBranch(fs, dir, 'feature', AUTHOR)
    expect(result).toEqual({ status: 'merged' })
    // Working tree must reflect both sides after the merge.
    expect(nodeFs.existsSync(path.join(dir, 'b.txt'))).toBe(true)
    expect(nodeFs.existsSync(path.join(dir, 'c.txt'))).toBe(true)
    const status = await getStatus(fs, dir)
    expect(status).toEqual({ staged: [], unstaged: [], untracked: [] })
  })

  it('returns conflict status and leaves the branch ref untouched when both sides edit the same line', async () => {
    await commitFile('a.txt', 'base content', 'initial')
    await isoGit.branch({ fs: nodeFs, dir, ref: 'feature', checkout: true })
    await commitFile('a.txt', 'feature version', 'edit on feature')
    await isoGit.checkout({ fs: nodeFs, dir, ref: 'main' })
    const mainTip = await commitFile('a.txt', 'main version', 'edit on main')

    const result = await mergeBranch(fs, dir, 'feature', AUTHOR)
    expect(result.status).toBe('conflict')
    expect(result.conflicts).toContain('a.txt')
    // Branch ref must not have moved — merge() throws before committing.
    await expect(resolveRef(fs, dir, 'HEAD')).resolves.toBe(mainTip)

    await abortCurrentMerge(fs, dir)
    const status = await getStatus(fs, dir)
    expect(status).toEqual({ staged: [], unstaged: [], untracked: [] })
    await expect(resolveRef(fs, dir, 'HEAD')).resolves.toBe(mainTip)
  })
})

describe('getConflictDiff', () => {
  it('reads the base/ours/theirs content for a conflicted path', async () => {
    await commitFile('a.txt', 'base content', 'initial')
    await isoGit.branch({ fs: nodeFs, dir, ref: 'feature', checkout: true })
    await commitFile('a.txt', 'feature version', 'edit on feature')
    await isoGit.checkout({ fs: nodeFs, dir, ref: 'main' })
    await commitFile('a.txt', 'main version', 'edit on main')

    const result = await mergeBranch(fs, dir, 'feature', AUTHOR)
    expect(result.status).toBe('conflict')

    const diff = await getConflictDiff(fs, dir, 'a.txt')
    expect(diff.ours).toEqual({
      filepath: 'a.txt',
      oldContent: 'base content',
      newContent: 'main version',
    })
    expect(diff.theirs).toEqual({
      filepath: 'a.txt',
      oldContent: 'base content',
      newContent: 'feature version',
    })
  })

  it('throws for a path that has no merge conflict', async () => {
    await commitFile('a.txt', 'hello', 'initial')
    await expect(getConflictDiff(fs, dir, 'a.txt')).rejects.toThrow()
  })
})
