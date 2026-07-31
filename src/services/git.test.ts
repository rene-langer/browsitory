import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import * as nodeFs from 'fs'
import * as os from 'os'
import * as path from 'path'
import * as isoGit from 'isomorphic-git'
import type { GitFs } from './fsaGitFs'
import {
  createCommit,
  getCommitDiff,
  getCurrentBranch,
  getGraphLog,
  getLog,
  getStagedDiff,
  getStatus,
  getUnstagedDiff,
  openRepository,
  stageFile,
  unstageFile,
} from './git'

// Exercises the exact same service functions the browser uses, but backed by
// Node's real fs against real temp-directory repos instead of the File System
// Access adapter — isomorphic-git treats both fs implementations identically,
// so this validates git.ts's logic without needing a browser.
const fs = nodeFs as unknown as GitFs
const AUTHOR = { name: 'Test User', email: 'test@example.com' }

let dir: string

function write(relPath: string, content: string) {
  const full = path.join(dir, relPath)
  nodeFs.mkdirSync(path.dirname(full), { recursive: true })
  nodeFs.writeFileSync(full, content)
}

function remove(relPath: string) {
  nodeFs.rmSync(path.join(dir, relPath))
}

beforeEach(async () => {
  dir = nodeFs.mkdtempSync(path.join(os.tmpdir(), 'browsitory-'))
  await isoGit.init({ fs: nodeFs, dir, defaultBranch: 'main' })
})

afterEach(() => {
  nodeFs.rmSync(dir, { recursive: true, force: true })
})

describe('openRepository', () => {
  it('resolves for a directory containing .git', async () => {
    await expect(openRepository(fs, dir)).resolves.toBeUndefined()
  })

  it('rejects for a directory without .git', async () => {
    const bare = nodeFs.mkdtempSync(path.join(os.tmpdir(), 'browsitory-nogit-'))
    await expect(openRepository(fs, bare)).rejects.toThrow()
    nodeFs.rmSync(bare, { recursive: true, force: true })
  })
})

describe('getLog', () => {
  it('returns an empty list for a repo with no commits', async () => {
    await expect(getLog(fs, dir)).resolves.toEqual([])
  })

  it('returns commits newest-first after committing', async () => {
    write('a.txt', 'hello')
    await stageFile(fs, dir, 'a.txt')
    await createCommit(fs, dir, { message: 'first commit', author: AUTHOR })

    const log = await getLog(fs, dir)
    expect(log).toHaveLength(1)
    expect(log[0].message).toBe('first commit\n')
    expect(log[0].author.email).toBe(AUTHOR.email)
  })
})

describe('getStatus', () => {
  it('reports a new file as untracked', async () => {
    write('new.txt', 'content')
    const status = await getStatus(fs, dir)
    expect(status.untracked).toContain('new.txt')
    expect(status.staged).not.toContain('new.txt')
  })

  it('reports a staged new file as staged, not untracked', async () => {
    write('new.txt', 'content')
    await stageFile(fs, dir, 'new.txt')
    const status = await getStatus(fs, dir)
    expect(status.staged).toContain('new.txt')
    expect(status.untracked).not.toContain('new.txt')
  })

  it('reports a modified tracked file as unstaged', async () => {
    write('a.txt', 'first version')
    await stageFile(fs, dir, 'a.txt')
    await createCommit(fs, dir, { message: 'add a.txt', author: AUTHOR })

    write('a.txt', 'second version, longer')
    const status = await getStatus(fs, dir)
    expect(status.unstaged).toContain('a.txt')
    expect(status.staged).not.toContain('a.txt')
  })

  it('moves a file from unstaged to staged after stageFile', async () => {
    write('a.txt', 'first version')
    await stageFile(fs, dir, 'a.txt')
    await createCommit(fs, dir, { message: 'add a.txt', author: AUTHOR })

    write('a.txt', 'second version, longer')
    await stageFile(fs, dir, 'a.txt')
    const status = await getStatus(fs, dir)
    expect(status.staged).toContain('a.txt')
    expect(status.unstaged).not.toContain('a.txt')
  })

  it('unstageFile moves a staged change back to unstaged', async () => {
    write('a.txt', 'first version')
    await stageFile(fs, dir, 'a.txt')
    await createCommit(fs, dir, { message: 'add a.txt', author: AUTHOR })

    write('a.txt', 'second version, longer')
    await stageFile(fs, dir, 'a.txt')
    await unstageFile(fs, dir, 'a.txt')

    const status = await getStatus(fs, dir)
    expect(status.unstaged).toContain('a.txt')
    expect(status.staged).not.toContain('a.txt')
  })

  it('stageFile stages a deletion', async () => {
    write('a.txt', 'first version')
    await stageFile(fs, dir, 'a.txt')
    await createCommit(fs, dir, { message: 'add a.txt', author: AUTHOR })

    remove('a.txt')
    await stageFile(fs, dir, 'a.txt')
    const status = await getStatus(fs, dir)
    expect(status.staged).toContain('a.txt')
  })

  it('reports a clean tree once everything is committed', async () => {
    write('a.txt', 'first version')
    await stageFile(fs, dir, 'a.txt')
    await createCommit(fs, dir, { message: 'add a.txt', author: AUTHOR })

    const status = await getStatus(fs, dir)
    expect(status).toEqual({ staged: [], unstaged: [], untracked: [] })
  })
})

describe('diffs', () => {
  it('getCommitDiff shows an added file on the initial commit', async () => {
    write('a.txt', 'hello world')
    await stageFile(fs, dir, 'a.txt')
    const oid = await createCommit(fs, dir, { message: 'initial', author: AUTHOR })

    const diff = await getCommitDiff(fs, dir, oid)
    expect(diff).toEqual([{ filepath: 'a.txt', oldContent: '', newContent: 'hello world' }])
  })

  it('getCommitDiff shows a modification against the parent commit', async () => {
    write('a.txt', 'first version')
    await stageFile(fs, dir, 'a.txt')
    await createCommit(fs, dir, { message: 'first version', author: AUTHOR })

    write('a.txt', 'second version, longer')
    await stageFile(fs, dir, 'a.txt')
    const oid = await createCommit(fs, dir, { message: 'second version, longer', author: AUTHOR })

    const diff = await getCommitDiff(fs, dir, oid)
    expect(diff).toEqual([{ filepath: 'a.txt', oldContent: 'first version', newContent: 'second version, longer' }])
  })

  it('getUnstagedDiff compares the index against the working tree', async () => {
    write('a.txt', 'first version')
    await stageFile(fs, dir, 'a.txt')
    await createCommit(fs, dir, { message: 'first version', author: AUTHOR })

    write('a.txt', 'second version, longer')
    const diff = await getUnstagedDiff(fs, dir, 'a.txt')
    expect(diff).toEqual([{ filepath: 'a.txt', oldContent: 'first version', newContent: 'second version, longer' }])
  })

  it('getStagedDiff compares HEAD against the index', async () => {
    write('a.txt', 'first version')
    await stageFile(fs, dir, 'a.txt')
    await createCommit(fs, dir, { message: 'first version', author: AUTHOR })

    write('a.txt', 'second version, longer')
    await stageFile(fs, dir, 'a.txt')
    const diff = await getStagedDiff(fs, dir, 'a.txt')
    expect(diff).toEqual([{ filepath: 'a.txt', oldContent: 'first version', newContent: 'second version, longer' }])
  })
})

describe('getCurrentBranch', () => {
  it('returns the default branch name', async () => {
    await expect(getCurrentBranch(fs, dir)).resolves.toBe('main')
  })
})

describe('getGraphLog', () => {
  it('returns an empty list for a repo with no commits', async () => {
    await expect(getGraphLog(fs, dir)).resolves.toEqual([])
  })

  it('includes commits from every local branch, de-duplicated, with refs attached', async () => {
    write('a.txt', 'first')
    await stageFile(fs, dir, 'a.txt')
    const base = await createCommit(fs, dir, { message: 'base', author: AUTHOR })

    await isoGit.branch({ fs: nodeFs, dir, ref: 'feature' })

    write('a.txt', 'on main')
    await stageFile(fs, dir, 'a.txt')
    const mainTip = await createCommit(fs, dir, { message: 'on main', author: AUTHOR })

    await isoGit.checkout({ fs: nodeFs, dir, ref: 'feature' })
    write('b.txt', 'on feature')
    await stageFile(fs, dir, 'b.txt')
    const featureTip = await createCommit(fs, dir, { message: 'on feature', author: AUTHOR })

    const graph = await getGraphLog(fs, dir)
    const byOid = new Map(graph.map((c) => [c.oid, c]))

    expect(graph).toHaveLength(3)
    expect(byOid.get(base)?.parents).toEqual([])
    expect(byOid.get(mainTip)?.refs).toEqual(['main'])
    // checkout switched HEAD to 'feature' before getGraphLog ran, so HEAD
    // decorates the feature tip, not the main tip.
    expect(byOid.get(featureTip)?.refs).toContain('feature')
    expect(byOid.get(featureTip)?.refs).toContain('HEAD')
    expect(byOid.get(featureTip)?.parents).toEqual([base])
  })
})
