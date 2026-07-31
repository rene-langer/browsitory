import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import CommitForm from './CommitForm'

beforeEach(() => {
  localStorage.clear()
})

describe('CommitForm', () => {
  it('disables submit until message, name, and email are all filled in', async () => {
    render(<CommitForm disabled={false} onCommit={vi.fn()} />)
    const button = screen.getByRole('button', { name: /commit/i })
    expect(button).toBeDisabled()

    await userEvent.type(screen.getByPlaceholderText(/author name/i), 'Alice')
    await userEvent.type(screen.getByPlaceholderText(/author email/i), 'alice@example.com')
    expect(button).toBeDisabled()

    await userEvent.type(screen.getByPlaceholderText(/commit message/i), 'Fix bug')
    expect(button).toBeEnabled()
  })

  it('calls onCommit with the trimmed message and author, then clears the message', async () => {
    const onCommit = vi.fn()
    render(<CommitForm disabled={false} onCommit={onCommit} />)

    await userEvent.type(screen.getByPlaceholderText(/author name/i), 'Alice')
    await userEvent.type(screen.getByPlaceholderText(/author email/i), 'alice@example.com')
    await userEvent.type(screen.getByPlaceholderText(/commit message/i), '  Fix bug  ')
    await userEvent.click(screen.getByRole('button', { name: /commit/i }))

    expect(onCommit).toHaveBeenCalledWith('Fix bug', { name: 'Alice', email: 'alice@example.com' })
    expect(screen.getByPlaceholderText(/commit message/i)).toHaveValue('')
  })

  it('stays disabled while disabled=true even with valid input', async () => {
    render(<CommitForm disabled={true} onCommit={vi.fn()} />)
    await userEvent.type(screen.getByPlaceholderText(/author name/i), 'Alice')
    await userEvent.type(screen.getByPlaceholderText(/author email/i), 'alice@example.com')
    await userEvent.type(screen.getByPlaceholderText(/commit message/i), 'Fix bug')
    expect(screen.getByRole('button', { name: /commit/i })).toBeDisabled()
  })
})
