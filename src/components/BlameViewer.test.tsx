import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import BlameViewer from './BlameViewer'
import type { BlameLine } from '@services/blame'

const commitA = {
  oid: 'aaaaaaaaaaaa',
  message: 'create file',
  author: { name: 'Alice', email: 'a@x.com', timestamp: 1700000000000 },
  parents: [],
}
const commitB = {
  oid: 'bbbbbbbbbbbb',
  message: 'change line 2',
  author: { name: 'Bob', email: 'b@x.com', timestamp: 1700000100000 },
  parents: ['aaaaaaaaaaaa'],
}

const lines: BlameLine[] = [
  { lineNumber: 1, content: 'line1', commit: commitA },
  { lineNumber: 2, content: 'CHANGED', commit: commitB },
  { lineNumber: 3, content: 'line3', commit: commitA },
]

describe('BlameViewer', () => {
  it('shows an empty state when there are no lines', () => {
    render(<BlameViewer filepath="a.txt" lines={[]} />)
    expect(screen.getByText(/no blame information/i)).toBeInTheDocument()
  })

  it('renders the filepath header', () => {
    render(<BlameViewer filepath="src/a.txt" lines={lines} />)
    expect(screen.getByText('src/a.txt')).toBeInTheDocument()
  })

  it('renders every line with its content and line number', () => {
    render(<BlameViewer filepath="a.txt" lines={lines} />)
    expect(screen.getByText('line1')).toBeInTheDocument()
    expect(screen.getByText('CHANGED')).toBeInTheDocument()
    expect(screen.getByText('line3')).toBeInTheDocument()
    expect(screen.getByText('1')).toBeInTheDocument()
    expect(screen.getByText('2')).toBeInTheDocument()
    expect(screen.getByText('3')).toBeInTheDocument()
  })

  it('collapses consecutive lines from the same commit into one info block, but splits on a different commit', () => {
    render(<BlameViewer filepath="a.txt" lines={lines} />)
    // commitA covers lines 1 and 3 (non-consecutive, split by commitB's line 2),
    // so its short oid should appear twice: once per run. The author name is
    // rendered as inline text alongside the oid rather than in its own
    // element, so it's asserted via the run count too rather than a separate
    // isolated-text query.
    expect(screen.getAllByText(commitA.oid.slice(0, 7))).toHaveLength(2)
    expect(screen.getAllByText(commitB.oid.slice(0, 7))).toHaveLength(1)
  })

  it('collapses a run of multiple consecutive same-commit lines into a single info block', () => {
    const consecutive: BlameLine[] = [
      { lineNumber: 1, content: 'a', commit: commitA },
      { lineNumber: 2, content: 'b', commit: commitA },
      { lineNumber: 3, content: 'c', commit: commitA },
    ]
    render(<BlameViewer filepath="a.txt" lines={consecutive} />)
    expect(screen.getAllByText(commitA.oid.slice(0, 7))).toHaveLength(1)
  })
})
