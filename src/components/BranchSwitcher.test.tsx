import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import BranchSwitcher from './BranchSwitcher'

const branches = [
  { name: 'main', oid: 'aaa111', isCurrent: true },
  { name: 'feature', oid: 'bbb222', isCurrent: false },
]

function setup(overrides: Partial<Parameters<typeof BranchSwitcher>[0]> = {}) {
  const onSwitch = vi.fn()
  const onCreate = vi.fn()
  const onDelete = vi.fn()
  const onRename = vi.fn()
  render(
    <BranchSwitcher
      branches={branches}
      currentBranch="main"
      onSwitch={onSwitch}
      onCreate={onCreate}
      onDelete={onDelete}
      onRename={onRename}
      {...overrides}
    />
  )
  return { onSwitch, onCreate, onDelete, onRename }
}

describe('BranchSwitcher', () => {
  it('shows the current branch name and keeps the dropdown closed initially', () => {
    setup()
    expect(screen.getByText(/on main/i)).toBeInTheDocument()
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
  })

  it('opens the dropdown listing all branches, current one marked', async () => {
    setup()
    await userEvent.click(screen.getByText(/on main/i))

    const menu = screen.getByRole('menu')
    expect(menu).toBeInTheDocument()
    expect(screen.getByText(/●\s*main/)).toBeInTheDocument()
    expect(screen.getByText('feature')).toBeInTheDocument()
  })

  it('calls onSwitch when clicking a non-current branch, and closes the menu', async () => {
    const { onSwitch } = setup()
    await userEvent.click(screen.getByText(/on main/i))
    await userEvent.click(screen.getByText('feature'))

    expect(onSwitch).toHaveBeenCalledWith('feature')
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
  })

  it('does not call onSwitch when clicking the current branch', async () => {
    const { onSwitch } = setup()
    await userEvent.click(screen.getByText(/on main/i))
    await userEvent.click(screen.getByText(/●\s*main/))

    expect(onSwitch).not.toHaveBeenCalled()
  })

  it('does not show a Delete action for the current branch', async () => {
    setup()
    await userEvent.click(screen.getByText(/on main/i))

    const deleteButtons = screen.getAllByRole('button', { name: /delete/i })
    expect(deleteButtons).toHaveLength(1) // only for "feature"
  })

  it('calls onDelete for a non-current branch', async () => {
    const { onDelete } = setup()
    await userEvent.click(screen.getByText(/on main/i))
    await userEvent.click(screen.getByRole('button', { name: /delete/i }))

    expect(onDelete).toHaveBeenCalledWith('feature')
  })

  it('calls onCreate with the typed branch name and clears the input', async () => {
    const { onCreate } = setup()
    await userEvent.click(screen.getByText(/on main/i))

    const input = screen.getByLabelText(/new branch name/i)
    await userEvent.type(input, 'my-new-branch')
    await userEvent.click(screen.getByRole('button', { name: /^create$/i }))

    expect(onCreate).toHaveBeenCalledWith('my-new-branch')
    expect(input).toHaveValue('')
  })

  it('renames a branch via the inline rename form', async () => {
    const { onRename } = setup()
    await userEvent.click(screen.getByText(/on main/i))
    await userEvent.click(screen.getAllByRole('button', { name: /rename/i })[1]) // feature row

    const input = screen.getByLabelText(/rename feature/i)
    await userEvent.clear(input)
    await userEvent.type(input, 'renamed-feature')
    await userEvent.click(screen.getByRole('button', { name: /save/i }))

    expect(onRename).toHaveBeenCalledWith('feature', 'renamed-feature')
  })
})
