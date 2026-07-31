import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import GraphView from './GraphView'
import type { GraphCommit } from '@services/git'

const author = { name: 'Alice', email: 'a@x.com', timestamp: 1700000000000 }

const commits: GraphCommit[] = [
  { oid: 'aaaaaaaaaaaa', parents: [], message: 'initial', author, refs: [] },
  { oid: 'bbbbbbbbbbbb', parents: ['aaaaaaaaaaaa'], message: 'second commit', author, refs: ['main', 'HEAD'] },
]

describe('GraphView', () => {
  it('shows an empty state when there are no commits', () => {
    render(<GraphView commits={[]} onSelect={vi.fn()} />)
    expect(screen.getByText(/no commits yet/i)).toBeInTheDocument()
  })

  it('renders a node per commit with its short oid and message', () => {
    render(<GraphView commits={commits} onSelect={vi.fn()} />)
    expect(screen.getByText('aaaaaaa')).toBeInTheDocument()
    expect(screen.getByText('bbbbbbb')).toBeInTheDocument()
    expect(screen.getByText('initial')).toBeInTheDocument()
    expect(screen.getByText('second commit')).toBeInTheDocument()
  })

  it('renders ref decorations for commits that have them', () => {
    render(<GraphView commits={commits} onSelect={vi.fn()} />)
    expect(screen.getByText('main, HEAD')).toBeInTheDocument()
  })

  it('calls onSelect with the oid when a node is clicked', async () => {
    const onSelect = vi.fn()
    render(<GraphView commits={commits} onSelect={onSelect} />)
    await userEvent.click(screen.getByText('second commit'))
    expect(onSelect).toHaveBeenCalledWith('bbbbbbbbbbbb')
  })

  it('renders an SVG containing an edge between a commit and its parent', () => {
    const { container } = render(<GraphView commits={commits} onSelect={vi.fn()} />)
    expect(container.querySelector('svg')).toBeInTheDocument()
    expect(container.querySelectorAll('polyline')).toHaveLength(1)
  })
})
