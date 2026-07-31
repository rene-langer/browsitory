import { describe, expect, it, vi } from 'vitest'
import * as git from 'isomorphic-git'
import { FakeDirectoryHandle, createFakeRoot } from '@/test/fakeDirectoryHandle'
import { createFsaGitFs } from './fsaGitFs'
import { createCommit, getLog, getStatus, getUnstagedDiff, openRepository, stageFile } from './git'

function fs() {
  const root = createFakeRoot()
  return createFsaGitFs(root as unknown as FileSystemDirectoryHandle).promises
}

describe('fsaGitFs', () => {
  it('writes and reads a file back as bytes', async () => {
    const { writeFile, readFile } = fs()
    await writeFile('/hello.txt', new TextEncoder().encode('hi there'))
    const result = await readFile('/hello.txt')
    expect(new TextDecoder().decode(result as Uint8Array)).toBe('hi there')
  })

  it('reads a file back as utf8 when requested', async () => {
    const { writeFile, readFile } = fs()
    await writeFile('/hello.txt', 'plain string content')
    const result = await readFile('/hello.txt', { encoding: 'utf8' })
    expect(result).toBe('plain string content')
  })

  it('creates nested directories on write without an explicit mkdir', async () => {
    const { writeFile, readFile } = fs()
    await writeFile('/.git/objects/ab/cdef', new Uint8Array([1, 2, 3]))
    const result = await readFile('/.git/objects/ab/cdef')
    expect(Array.from(result as Uint8Array)).toEqual([1, 2, 3])
  })

  it('mkdir is idempotent', async () => {
    const { mkdir } = fs()
    await mkdir('/.git/refs/heads')
    await expect(mkdir('/.git/refs/heads')).resolves.toBeUndefined()
  })

  it('throws ENOENT reading a file that does not exist', async () => {
    const { readFile } = fs()
    await expect(readFile('/missing.txt')).rejects.toMatchObject({ code: 'ENOENT' })
  })

  it('throws ENOENT statting a path whose parent does not exist', async () => {
    const { stat } = fs()
    await expect(stat('/no/such/dir/file.txt')).rejects.toMatchObject({ code: 'ENOENT' })
  })

  it('lists directory entries', async () => {
    const { writeFile, mkdir, readdir } = fs()
    await writeFile('/a.txt', 'a')
    await writeFile('/b.txt', 'b')
    await mkdir('/subdir')
    const names = (await readdir('/')).sort()
    expect(names).toEqual(['a.txt', 'b.txt', 'subdir'])
  })

  it('stat reports file vs directory correctly', async () => {
    const { writeFile, mkdir, stat } = fs()
    await writeFile('/file.txt', 'content')
    await mkdir('/dir')
    const fileStat = await stat('/file.txt')
    const dirStat = await stat('/dir')
    expect(fileStat.isFile()).toBe(true)
    expect(fileStat.isDirectory()).toBe(false)
    expect(dirStat.isDirectory()).toBe(true)
    expect(dirStat.isFile()).toBe(false)
  })

  it('unlink removes a file', async () => {
    const { writeFile, unlink, readFile } = fs()
    await writeFile('/gone.txt', 'bye')
    await unlink('/gone.txt')
    await expect(readFile('/gone.txt')).rejects.toMatchObject({ code: 'ENOENT' })
  })

  it('rename moves content to the new path and removes the old one', async () => {
    const { writeFile, rename, readFile } = fs()
    await writeFile('/old.txt', 'moved content')
    await rename('/old.txt', '/new.txt')
    await expect(readFile('/old.txt')).rejects.toMatchObject({ code: 'ENOENT' })
    expect(await readFile('/new.txt', 'utf8')).toBe('moved content')
  })

  // Regression test for a real ~20s-to-load-one-small-diff bug: every call
  // used to re-resolve every ancestor directory from root, so reading N
  // files under the same directory did N (not 1) directory lookups for that
  // shared ancestor. Caught via real-browser testing against an actual
  // multi-hundred-file repo — the fake handle here has no real async
  // latency, so this only verifies *call counts*, not wall-clock time.
  it('caches resolved directory handles instead of re-walking from root on every call', async () => {
    // Build the fixture directly against the fake handle (bypassing our
    // adapter's own mkdir/writeFile) so the adapter's cache starts empty for
    // the reads below — using our own writeFile here would pre-warm the
    // cache via its defensive mkdir and defeat the point of the assertion.
    const root = createFakeRoot()
    const dir = await root.getDirectoryHandle('dir', { create: true })
    const sub = await dir.getDirectoryHandle('sub', { create: true })
    async function writeText(handle: FakeDirectoryHandle, name: string, text: string) {
      const fileHandle = await handle.getFileHandle(name, { create: true })
      const writable = await fileHandle.createWritable()
      await writable.write(text)
      await writable.close()
    }
    await writeText(dir, 'a.txt', 'a')
    await writeText(dir, 'b.txt', 'b')
    await writeText(sub, 'c.txt', 'c')

    const { readFile } = createFsaGitFs(root as unknown as FileSystemDirectoryHandle).promises

    const spy = vi.spyOn(FakeDirectoryHandle.prototype, 'getDirectoryHandle')
    spy.mockClear()

    await readFile('/dir/a.txt')
    await readFile('/dir/b.txt')
    await readFile('/dir/sub/c.txt')

    // Without caching this would be 4 (dir, dir, dir, sub) across the three
    // reads. With caching: 'dir' is resolved once on the first read and
    // reused for the second and third; 'sub' is resolved once on the third.
    expect(spy).toHaveBeenCalledTimes(2)

    spy.mockRestore()
  })
})

// Every other test in this file (and in git.ts's own tests) exercises either
// fsaGitFs's functions directly, or isomorphic-git against Node's real fs —
// neither path goes through isomorphic-git's internal `bindFs()`, which
// unconditionally does `fs[command].bind(fs)` for a fixed command list
// (including `readlink`/`symlink`) with no existence check. That gap let a
// real bug ship: omitting readlink/symlink here didn't just disable symlink
// support, it crashed *every* isomorphic-git call against this adapter with
// "Cannot read properties of undefined (reading 'bind')", for any repo, only
// discovered via manual real-browser testing. These tests close that gap by
// actually handing the full GitFs object to isomorphic-git, the same way
// production code does.
describe('fsaGitFs used as a real isomorphic-git fs client', () => {
  function gitFs() {
    const root = createFakeRoot()
    return createFsaGitFs(root as unknown as FileSystemDirectoryHandle)
  }

  it('survives isomorphic-git init + commit + log without throwing', async () => {
    const fs = gitFs()
    await git.init({ fs, dir: '/', defaultBranch: 'main' })
    await fs.promises.writeFile('/a.txt', 'hello')
    await git.add({ fs, dir: '/', filepath: 'a.txt' })
    await git.commit({
      fs,
      dir: '/',
      message: 'initial',
      author: { name: 'Test', email: 'test@example.com' },
    })

    const log = await git.log({ fs, dir: '/' })
    expect(log).toHaveLength(1)
    expect(log[0].commit.message.trim()).toBe('initial')
  })

  it('survives statusMatrix (exercises stat/lstat on every tracked path)', async () => {
    const fs = gitFs()
    await git.init({ fs, dir: '/', defaultBranch: 'main' })
    await fs.promises.writeFile('/a.txt', 'hello')
    await git.add({ fs, dir: '/', filepath: 'a.txt' })
    await git.commit({
      fs,
      dir: '/',
      message: 'initial',
      author: { name: 'Test', email: 'test@example.com' },
    })

    await expect(git.statusMatrix({ fs, dir: '/' })).resolves.not.toThrow()
  })

  it('supports the full app flow through git.ts (open, status, stage, commit, log, diff)', async () => {
    const fs = gitFs()
    await git.init({ fs, dir: '/', defaultBranch: 'main' })

    await openRepository(fs, '/')

    await fs.promises.writeFile('/a.txt', 'v1')
    let status = await getStatus(fs, '/')
    expect(status.untracked).toContain('a.txt')

    await stageFile(fs, '/', 'a.txt')
    status = await getStatus(fs, '/')
    expect(status.staged).toContain('a.txt')

    const oid = await createCommit(fs, '/', {
      message: 'initial',
      author: { name: 'Test', email: 'test@example.com' },
    })

    const log = await getLog(fs, '/')
    expect(log).toHaveLength(1)
    expect(log[0].oid).toBe(oid)

    await fs.promises.writeFile('/a.txt', 'v2, longer')
    const diff = await getUnstagedDiff(fs, '/', 'a.txt')
    expect(diff).toEqual([{ filepath: 'a.txt', oldContent: 'v1', newContent: 'v2, longer' }])
  })
})
