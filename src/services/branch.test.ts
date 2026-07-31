import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import * as nodeFs from 'fs'
import * as os from 'os'
import * as path from 'path'
import * as isoGit from 'isomorphic-git'
import type { GitFs } from './fsaGitFs'
import {
  applyStash,
  createBranch,
  createCommit,
  createStash,
  deleteBranchByName,
  dropStash,
  getCurrentBranch,
  getStatus,
  listAllBranches,
  listStashes,
  popStash,
  renameBranchTo,
  stageFile,
  switchBranch,
} from './git'

// Exercises the exact same service functions the browser uses, but backed by
// Node's real fs against real temp-directory repos — see git.test.ts for the
// rationale behind this pattern.
const fs = nodeFs as unknown as GitFs
const AUTHOR = { name: 'Test User', email: 'test@example.com' }

let dir: string

function write(relPath: string, content: string) {
  const full = path.join(dir, relPath)
  nodeFs.mkdirSync(path.dirname(full), { recursive: true })
  nodeFs.writeFileSync(full, content)
}

async function commitFile(relPath: string, content: string, message: string) {
  write(relPath, content)
  await stageFile(fs, dir, relPath)
  return createCommit(fs, dir, { message, author: AUTHOR })
}

beforeEach(async () => {
  dir = nodeFs.mkdtempSync(path.join(os.tmpdir(), 'browsitory-'))
  await isoGit.init({ fs: nodeFs, dir, defaultBranch: 'main' })
  // isomorphic-git's stash API (unlike commit) has no way to pass an author
  // explicitly — it reads user.name/user.email from .git/config, so tests
  // that stash need it set even though createCommit's explicit AUTHOR arg
  // never touches config.
  await isoGit.setConfig({ fs: nodeFs, dir, path: 'user.name', value: AUTHOR.name })
  await isoGit.setConfig({ fs: nodeFs, dir, path: 'user.email', value: AUTHOR.email })
})

afterEach(() => {
  nodeFs.rmSync(dir, { recursive: true, force: true })
})

describe('listAllBranches', () => {
  it('returns an empty list for a repo with no commits', async () => {
    await expect(listAllBranches(fs, dir)).resolves.toEqual([])
  })

  it('lists all branches with the current one flagged', async () => {
    await commitFile('a.txt', 'hello', 'initial')
    await createBranch(fs, dir, 'feature')

    const branches = await listAllBranches(fs, dir)
    const names = branches.map((b) => b.name).sort()
    expect(names).toEqual(['feature', 'main'])

    const main = branches.find((b) => b.name === 'main')
    const feature = branches.find((b) => b.name === 'feature')
    expect(main?.isCurrent).toBe(true)
    expect(feature?.isCurrent).toBe(false)
    expect(main?.oid).toBe(feature?.oid) // feature was branched off main's tip
  })
})

describe('createBranch', () => {
  it('creates a new branch pointing at HEAD by default', async () => {
    const oid = await commitFile('a.txt', 'hello', 'initial')
    await createBranch(fs, dir, 'feature')

    const branches = await listAllBranches(fs, dir)
    expect(branches.find((b) => b.name === 'feature')?.oid).toBe(oid)
  })

  it('creates a branch at an explicit start point', async () => {
    const firstOid = await commitFile('a.txt', 'v1', 'first')
    await commitFile('a.txt', 'v2', 'second')

    await createBranch(fs, dir, 'from-first', firstOid)
    const branches = await listAllBranches(fs, dir)
    expect(branches.find((b) => b.name === 'from-first')?.oid).toBe(firstOid)
  })
})

describe('deleteBranchByName', () => {
  it('deletes a non-current branch', async () => {
    await commitFile('a.txt', 'hello', 'initial')
    await createBranch(fs, dir, 'feature')

    await deleteBranchByName(fs, dir, 'feature')
    const branches = await listAllBranches(fs, dir)
    expect(branches.map((b) => b.name)).not.toContain('feature')
  })

  it('refuses to delete the current branch', async () => {
    await commitFile('a.txt', 'hello', 'initial')
    await expect(deleteBranchByName(fs, dir, 'main')).rejects.toThrow(/currently checked out/i)

    // and it must not have actually deleted anything
    const branches = await listAllBranches(fs, dir)
    expect(branches.map((b) => b.name)).toContain('main')
  })
})

describe('renameBranchTo', () => {
  it('renames a non-current branch', async () => {
    await commitFile('a.txt', 'hello', 'initial')
    await createBranch(fs, dir, 'old-name')

    await renameBranchTo(fs, dir, 'old-name', 'new-name')
    const branches = await listAllBranches(fs, dir)
    const names = branches.map((b) => b.name)
    expect(names).toContain('new-name')
    expect(names).not.toContain('old-name')
  })

  it('renames the current branch and keeps HEAD pointed at it', async () => {
    await commitFile('a.txt', 'hello', 'initial')
    await renameBranchTo(fs, dir, 'main', 'trunk')

    await expect(getCurrentBranch(fs, dir)).resolves.toBe('trunk')
  })
})

describe('switchBranch', () => {
  it('switches HEAD and the working tree to another branch', async () => {
    await commitFile('a.txt', 'on main', 'initial')
    await createBranch(fs, dir, 'feature')
    await switchBranch(fs, dir, 'feature')
    await commitFile('a.txt', 'on feature', 'feature commit')

    await switchBranch(fs, dir, 'main')
    await expect(getCurrentBranch(fs, dir)).resolves.toBe('main')
    expect(nodeFs.readFileSync(path.join(dir, 'a.txt'), 'utf8')).toBe('on main')

    await switchBranch(fs, dir, 'feature')
    await expect(getCurrentBranch(fs, dir)).resolves.toBe('feature')
    expect(nodeFs.readFileSync(path.join(dir, 'a.txt'), 'utf8')).toBe('on feature')
  })

  it('refuses to switch when it would overwrite uncommitted changes', async () => {
    await commitFile('a.txt', 'on main', 'initial')
    await createBranch(fs, dir, 'feature')
    await switchBranch(fs, dir, 'feature')
    await commitFile('a.txt', 'on feature', 'feature commit')
    await switchBranch(fs, dir, 'main')

    // dirty the working tree in a way that conflicts with feature's version
    write('a.txt', 'uncommitted local edit')

    await expect(switchBranch(fs, dir, 'feature')).rejects.toThrow(/local changes/i)
    // must still be on main, untouched
    await expect(getCurrentBranch(fs, dir)).resolves.toBe('main')
    expect(nodeFs.readFileSync(path.join(dir, 'a.txt'), 'utf8')).toBe('uncommitted local edit')
  })
})

describe('stash', () => {
  it('lists no stashes for a clean new repo', async () => {
    await commitFile('a.txt', 'hello', 'initial')
    await expect(listStashes(fs, dir)).resolves.toEqual([])
  })

  it('creates a stash and removes changes from the working tree', async () => {
    await commitFile('a.txt', 'v1', 'initial')
    write('a.txt', 'v2 uncommitted')

    await createStash(fs, dir, 'my wip')

    const status = await getStatus(fs, dir)
    expect(status).toEqual({ staged: [], unstaged: [], untracked: [] })
    expect(nodeFs.readFileSync(path.join(dir, 'a.txt'), 'utf8')).toBe('v1')

    const stashes = await listStashes(fs, dir)
    expect(stashes).toHaveLength(1)
    expect(stashes[0].index).toBe(0)
    expect(stashes[0].message).toContain('my wip')
  })

  it('applies a stash without removing it', async () => {
    await commitFile('a.txt', 'v1', 'initial')
    write('a.txt', 'v2 uncommitted')
    await createStash(fs, dir)

    await applyStash(fs, dir, 0)

    expect(nodeFs.readFileSync(path.join(dir, 'a.txt'), 'utf8')).toBe('v2 uncommitted')
    await expect(listStashes(fs, dir)).resolves.toHaveLength(1)
  })

  it('pops a stash, applying it and removing it from the list', async () => {
    await commitFile('a.txt', 'v1', 'initial')
    write('a.txt', 'v2 uncommitted')
    await createStash(fs, dir)

    await popStash(fs, dir, 0)

    expect(nodeFs.readFileSync(path.join(dir, 'a.txt'), 'utf8')).toBe('v2 uncommitted')
    await expect(listStashes(fs, dir)).resolves.toEqual([])
  })

  it('drops a stash without applying it', async () => {
    await commitFile('a.txt', 'v1', 'initial')
    write('a.txt', 'v2 uncommitted')
    await createStash(fs, dir)

    await dropStash(fs, dir, 0)

    expect(nodeFs.readFileSync(path.join(dir, 'a.txt'), 'utf8')).toBe('v1')
    await expect(listStashes(fs, dir)).resolves.toEqual([])
  })

  it('tracks multiple stashes with increasing indices, newest first', async () => {
    await commitFile('a.txt', 'v1', 'initial')

    write('a.txt', 'first stash content')
    await createStash(fs, dir, 'first')

    write('a.txt', 'second stash content')
    await createStash(fs, dir, 'second')

    const stashes = await listStashes(fs, dir)
    expect(stashes).toHaveLength(2)
    expect(stashes[0].index).toBe(0)
    expect(stashes[0].message).toContain('second')
    expect(stashes[1].index).toBe(1)
    expect(stashes[1].message).toContain('first')
  })
})
