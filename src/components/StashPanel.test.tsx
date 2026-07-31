import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import StashPanel from './StashPanel'

describe('StashPanel', () => {
  it('shows an empty state when there are no stashes', () => {
    render(<StashPanel stashes={[]} onCreate={vi.fn()} onApply={vi.fn()} onPop={vi.fn()} onDrop={vi.fn()} />)
    expect(screen.getByText(/no stashes/i)).toBeInTheDocument()
    expect(screen.getByText(/stashes \(0\)/i)).toBeInTheDocument()
  })

  it('lists stashes with their index and message', () => {
    render(
      <StashPanel
        stashes={[
          { index: 0, message: 'WIP on main: abc123 latest' },
          { index: 1, message: 'my saved work' },
        ]}
        onCreate={vi.fn()}
        onApply={vi.fn()}
        onPop={vi.fn()}
        onDrop={vi.fn()}
      />
    )
    expect(screen.getByText(/stashes \(2\)/i)).toBeInTheDocument()
    expect(screen.getByText(/WIP on main: abc123 latest/)).toBeInTheDocument()
    expect(screen.getByText(/my saved work/)).toBeInTheDocument()
  })

  it('calls onCreate with a trimmed message and clears the input', async () => {
    const onCreate = vi.fn()
    render(<StashPanel stashes={[]} onCreate={onCreate} onApply={vi.fn()} onPop={vi.fn()} onDrop={vi.fn()} />)

    const input = screen.getByLabelText(/stash message/i)
    await userEvent.type(input, '  work in progress  ')
    await userEvent.click(screen.getByRole('button', { name: /^stash$/i }))

    expect(onCreate).toHaveBeenCalledWith('work in progress')
    expect(input).toHaveValue('')
  })

  it('calls onCreate with undefined when no message is entered', async () => {
    const onCreate = vi.fn()
    render(<StashPanel stashes={[]} onCreate={onCreate} onApply={vi.fn()} onPop={vi.fn()} onDrop={vi.fn()} />)

    await userEvent.click(screen.getByRole('button', { name: /^stash$/i }))

    expect(onCreate).toHaveBeenCalledWith(undefined)
  })

  it('calls onApply/onPop/onDrop with the stash index', async () => {
    const onApply = vi.fn()
    const onPop = vi.fn()
    const onDrop = vi.fn()
    render(
      <StashPanel
        stashes={[{ index: 2, message: 'a stash' }]}
        onCreate={vi.fn()}
        onApply={onApply}
        onPop={onPop}
        onDrop={onDrop}
      />
    )

    await userEvent.click(screen.getByRole('button', { name: /apply/i }))
    await userEvent.click(screen.getByRole('button', { name: /pop/i }))
    await userEvent.click(screen.getByRole('button', { name: /drop/i }))

    expect(onApply).toHaveBeenCalledWith(2)
    expect(onPop).toHaveBeenCalledWith(2)
    expect(onDrop).toHaveBeenCalledWith(2)
  })
})
