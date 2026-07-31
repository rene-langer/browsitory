import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

vi.mock('react-diff-viewer-continued', () => ({
  default: ({ oldValue, newValue }: { oldValue: string; newValue: string }) => (
    <div data-testid="mock-diff">
      {oldValue}|{newValue}
    </div>
  ),
}))

import DiffViewer from './DiffViewer'

describe('DiffViewer', () => {
  it('shows an empty state when there are no diffs', () => {
    render(<DiffViewer diffs={[]} />)
    expect(screen.getByText(/no changes/i)).toBeInTheDocument()
  })

  it('renders a file header and diff content per file', () => {
    render(
      <DiffViewer
        diffs={[
          { filepath: 'a.txt', oldContent: 'old', newContent: 'new' },
          { filepath: 'b.txt', oldContent: '', newContent: 'added' },
        ]}
      />
    )
    expect(screen.getByText('a.txt')).toBeInTheDocument()
    expect(screen.getByText('b.txt')).toBeInTheDocument()
    expect(screen.getAllByTestId('mock-diff')).toHaveLength(2)
  })
})
