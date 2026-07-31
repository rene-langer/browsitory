import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import CommitList from './CommitList'
import type { CommitInfo } from '@services/git'

const commits: CommitInfo[] = [
  {
    oid: 'aaaaaaaaaaaa',
    message: 'First commit\n\nbody',
    author: { name: 'Alice', email: 'a@x.com', timestamp: 1700000000000 },
    parents: [],
  },
  {
    oid: 'bbbbbbbbbbbb',
    message: 'Second commit',
    author: { name: 'Bob', email: 'b@x.com', timestamp: 1700000100000 },
    parents: ['aaaaaaaaaaaa'],
  },
]

describe('CommitList', () => {
  it('renders an empty state when there are no commits', () => {
    render(<CommitList commits={[]} onSelect={vi.fn()} />)
    expect(screen.getByText(/no commits yet/i)).toBeInTheDocument()
  })

  it('renders one row per commit with the first line of the message', () => {
    render(<CommitList commits={commits} onSelect={vi.fn()} />)
    expect(screen.getByText('First commit')).toBeInTheDocument()
    expect(screen.getByText('Second commit')).toBeInTheDocument()
    expect(screen.queryByText('body')).not.toBeInTheDocument()
  })

  it('calls onSelect with the oid when a commit is clicked', async () => {
    const onSelect = vi.fn()
    render(<CommitList commits={commits} onSelect={onSelect} />)
    await userEvent.click(screen.getByText('Second commit'))
    expect(onSelect).toHaveBeenCalledWith('bbbbbbbbbbbb')
  })
})
