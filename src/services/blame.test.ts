import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import * as nodeFs from 'fs'
import * as os from 'os'
import * as path from 'path'
import * as isoGit from 'isomorphic-git'
import type { GitFs } from './fsaGitFs'
import { getBlame } from './blame'

// Same real-temp-directory-repo pattern as git.test.ts: exercise the exact
// same code path the browser uses, backed by Node's fs against a real repo
// instead of mocking isomorphic-git.
const fs = nodeFs as unknown as GitFs
const AUTHOR = { name: 'Test User', email: 'test@example.com' }

let dir: string

function write(relPath: string, content: string) {
  const full = path.join(dir, relPath)
  nodeFs.mkdirSync(path.dirname(full), { recursive: true })
  nodeFs.writeFileSync(full, content)
}

async function commitFile(relPath: string, content: string, message: string): Promise<string> {
  write(relPath, content)
  await isoGit.add({ fs: nodeFs, dir, filepath: relPath })
  return isoGit.commit({ fs: nodeFs, dir, message, author: AUTHOR })
}

beforeEach(async () => {
  dir = nodeFs.mkdtempSync(path.join(os.tmpdir(), 'browsitory-blame-'))
  await isoGit.init({ fs: nodeFs, dir, defaultBranch: 'main' })
})

afterEach(() => {
  nodeFs.rmSync(dir, { recursive: true, force: true })
})

describe('getBlame', () => {
  it('attributes every line to the commit that created the file', async () => {
    const oidA = await commitFile('a.txt', 'line1\nline2\nline3\n', 'A: create file')

    const blame = await getBlame(fs, dir, 'a.txt')

    expect(blame).toHaveLength(3)
    expect(blame.map((l) => l.content)).toEqual(['line1', 'line2', 'line3'])
    expect(blame.every((l) => l.commit.oid === oidA)).toBe(true)
    expect(blame.map((l) => l.lineNumber)).toEqual([1, 2, 3])
  })

  it('preserves original provenance for untouched lines when only one line changes', async () => {
    const oidA = await commitFile('a.txt', 'line1\nline2\nline3\n', 'A: create file')
    const oidB = await commitFile('a.txt', 'line1\nCHANGED\nline3\n', 'B: change line 2')

    const blame = await getBlame(fs, dir, 'a.txt')

    expect(blame.map((l) => l.content)).toEqual(['line1', 'CHANGED', 'line3'])
    expect(blame[0].commit.oid).toBe(oidA)
    expect(blame[1].commit.oid).toBe(oidB)
    expect(blame[2].commit.oid).toBe(oidA)
  })

  it('attributes an appended line to the appending commit, leaving earlier lines alone', async () => {
    const oidA = await commitFile('a.txt', 'line1\nline2\nline3\n', 'A: create file')
    const oidB = await commitFile('a.txt', 'line1\nCHANGED\nline3\n', 'B: change line 2')
    const oidC = await commitFile('a.txt', 'line1\nCHANGED\nline3\nline4\n', 'C: append line 4')

    const blame = await getBlame(fs, dir, 'a.txt')

    expect(blame.map((l) => l.content)).toEqual(['line1', 'CHANGED', 'line3', 'line4'])
    expect(blame[0].commit.oid).toBe(oidA)
    expect(blame[1].commit.oid).toBe(oidB)
    expect(blame[2].commit.oid).toBe(oidA)
    expect(blame[3].commit.oid).toBe(oidC)
  })

  it('ignores commits that only touch a different, unrelated file', async () => {
    const oidA = await commitFile('a.txt', 'line1\nline2\nline3\n', 'A: create a.txt')
    await commitFile('other.txt', 'unrelated content', 'D: touch a different file')

    const blame = await getBlame(fs, dir, 'a.txt')

    expect(blame.map((l) => l.content)).toEqual(['line1', 'line2', 'line3'])
    expect(blame.every((l) => l.commit.oid === oidA)).toBe(true)
  })

  it('attributes a same-position line edit as a replace, not a misaligned shift', async () => {
    const oidA = await commitFile(
      'a.txt',
      'alpha\nbeta\ngamma\ndelta\n',
      'A: create four-line file'
    )
    const oidB = await commitFile(
      'a.txt',
      'alpha\nBETA-EDITED\ngamma\ndelta\n',
      'B: edit line 2 in place'
    )

    const blame = await getBlame(fs, dir, 'a.txt')

    expect(blame.map((l) => l.content)).toEqual(['alpha', 'BETA-EDITED', 'gamma', 'delta'])
    expect(blame[0].commit.oid).toBe(oidA)
    expect(blame[1].commit.oid).toBe(oidB)
    expect(blame[2].commit.oid).toBe(oidA)
    expect(blame[3].commit.oid).toBe(oidA)
  })
})
