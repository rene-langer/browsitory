import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

vi.mock('react-diff-viewer-continued', () => ({
  default: ({
    oldValue,
    newValue,
    compareMethod,
  }: {
    oldValue: string
    newValue: string
    compareMethod?: string
  }) => (
    <div data-testid="mock-diff" data-compare-method={compareMethod}>
      {oldValue}|{newValue}
    </div>
  ),
  DiffMethod: { WORDS_WITH_SPACE: 'diffWordsWithSpace' },
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

  it('uses word-level diffing, not the library default of character-level', () => {
    // Character-level diffing (the library default) reads as noisy,
    // mid-word-fragmented highlighting on anything but single-character
    // changes — confirmed against a real multi-edit prose diff. Word-level
    // keeps whitespace attached to each token, matching GitHub/GitLab-style
    // diff rendering.
    render(<DiffViewer diffs={[{ filepath: 'a.txt', oldContent: 'old', newContent: 'new' }]} />)
    expect(screen.getByTestId('mock-diff')).toHaveAttribute(
      'data-compare-method',
      'diffWordsWithSpace'
    )
  })
})
