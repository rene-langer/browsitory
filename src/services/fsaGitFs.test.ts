import { describe, expect, it } from 'vitest'
import { createFakeRoot } from '@/test/fakeDirectoryHandle'
import { createFsaGitFs } from './fsaGitFs'

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
})
