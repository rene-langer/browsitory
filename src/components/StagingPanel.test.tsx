import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import StagingPanel from './StagingPanel'

describe('StagingPanel', () => {
  it('shows empty state messages when there are no changes', () => {
    render(
      <StagingPanel
        status={{ staged: [], unstaged: [], untracked: [] }}
        onStage={vi.fn()}
        onUnstage={vi.fn()}
        onSelectUnstaged={vi.fn()}
        onSelectStaged={vi.fn()}
        onBlame={vi.fn()}
      />
    )
    expect(screen.getByText(/nothing staged/i)).toBeInTheDocument()
    expect(screen.getByText(/no changes/i)).toBeInTheDocument()
  })

  it('lists staged, unstaged, and untracked files', () => {
    render(
      <StagingPanel
        status={{ staged: ['staged.txt'], unstaged: ['modified.txt'], untracked: ['new.txt'] }}
        onStage={vi.fn()}
        onUnstage={vi.fn()}
        onSelectUnstaged={vi.fn()}
        onSelectStaged={vi.fn()}
        onBlame={vi.fn()}
      />
    )
    expect(screen.getByText('staged.txt')).toBeInTheDocument()
    expect(screen.getByText('modified.txt')).toBeInTheDocument()
    expect(screen.getByText('new.txt')).toBeInTheDocument()
  })

  it('calls onStage when clicking Stage on an unstaged file', async () => {
    const onStage = vi.fn()
    render(
      <StagingPanel
        status={{ staged: [], unstaged: ['modified.txt'], untracked: [] }}
        onStage={onStage}
        onUnstage={vi.fn()}
        onSelectUnstaged={vi.fn()}
        onSelectStaged={vi.fn()}
        onBlame={vi.fn()}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: /stage/i }))
    expect(onStage).toHaveBeenCalledWith('modified.txt')
  })

  it('calls onUnstage when clicking Unstage on a staged file', async () => {
    const onUnstage = vi.fn()
    render(
      <StagingPanel
        status={{ staged: ['staged.txt'], unstaged: [], untracked: [] }}
        onStage={vi.fn()}
        onUnstage={onUnstage}
        onSelectUnstaged={vi.fn()}
        onSelectStaged={vi.fn()}
        onBlame={vi.fn()}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: /unstage/i }))
    expect(onUnstage).toHaveBeenCalledWith('staged.txt')
  })

  it('calls onSelectUnstaged/onSelectStaged when clicking a filename', async () => {
    const onSelectUnstaged = vi.fn()
    const onSelectStaged = vi.fn()
    render(
      <StagingPanel
        status={{ staged: ['staged.txt'], unstaged: ['modified.txt'], untracked: [] }}
        onStage={vi.fn()}
        onUnstage={vi.fn()}
        onSelectUnstaged={onSelectUnstaged}
        onSelectStaged={onSelectStaged}
        onBlame={vi.fn()}
      />
    )
    await userEvent.click(screen.getByText('staged.txt'))
    await userEvent.click(screen.getByText('modified.txt'))
    expect(onSelectStaged).toHaveBeenCalledWith('staged.txt')
    expect(onSelectUnstaged).toHaveBeenCalledWith('modified.txt')
  })

  it('calls onBlame when clicking Blame on a file', async () => {
    const onBlame = vi.fn()
    render(
      <StagingPanel
        status={{ staged: [], unstaged: ['modified.txt'], untracked: [] }}
        onStage={vi.fn()}
        onUnstage={vi.fn()}
        onSelectUnstaged={vi.fn()}
        onSelectStaged={vi.fn()}
        onBlame={onBlame}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: /blame/i }))
    expect(onBlame).toHaveBeenCalledWith('modified.txt')
  })
})
