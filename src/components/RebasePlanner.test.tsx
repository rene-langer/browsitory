import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import RebasePlanner from './RebasePlanner'
import type { RebaseCommitPlan } from '@services/rebase'

const plan: RebaseCommitPlan[] = [
  { oid: 'aaaaaaaaaaaa', message: 'first commit', author: { name: 'A', email: 'a@x.com', timestamp: 0 }, action: 'pick' },
  { oid: 'bbbbbbbbbbbb', message: 'second commit', author: { name: 'A', email: 'a@x.com', timestamp: 0 }, action: 'pick' },
]

function noop() {}

describe('RebasePlanner', () => {
  it('shows a prompt when no plan is loaded yet', () => {
    render(
      <RebasePlanner
        ontoInput=""
        onOntoInputChange={noop}
        onLoadPlan={noop}
        plan={[]}
        onMove={noop}
        onToggleDrop={noop}
        onStart={noop}
        onCancel={noop}
        loading={false}
        error={null}
      />
    )
    expect(screen.getByText(/enter a target commit/i)).toBeInTheDocument()
  })

  it('disables Load plan until there is input', () => {
    render(
      <RebasePlanner
        ontoInput=""
        onOntoInputChange={noop}
        onLoadPlan={noop}
        plan={[]}
        onMove={noop}
        onToggleDrop={noop}
        onStart={noop}
        onCancel={noop}
        loading={false}
        error={null}
      />
    )
    expect(screen.getByRole('button', { name: /load plan/i })).toBeDisabled()
  })

  it('renders the plan in order with each commit message', () => {
    render(
      <RebasePlanner
        ontoInput="main~2"
        onOntoInputChange={noop}
        onLoadPlan={noop}
        plan={plan}
        onMove={noop}
        onToggleDrop={noop}
        onStart={noop}
        onCancel={noop}
        loading={false}
        error={null}
      />
    )
    expect(screen.getByText('first commit')).toBeInTheDocument()
    expect(screen.getByText('second commit')).toBeInTheDocument()
  })

  it('calls onMove with the right direction', async () => {
    const onMove = vi.fn()
    render(
      <RebasePlanner
        ontoInput="main~2"
        onOntoInputChange={noop}
        onLoadPlan={noop}
        plan={plan}
        onMove={onMove}
        onToggleDrop={noop}
        onStart={noop}
        onCancel={noop}
        loading={false}
        error={null}
      />
    )
    await userEvent.click(screen.getAllByLabelText('Move down')[0])
    expect(onMove).toHaveBeenCalledWith(0, 1)
  })

  it('calls onToggleDrop and reflects the drop state visually', async () => {
    const onToggleDrop = vi.fn()
    render(
      <RebasePlanner
        ontoInput="main~2"
        onOntoInputChange={noop}
        onLoadPlan={noop}
        plan={plan}
        onMove={noop}
        onToggleDrop={onToggleDrop}
        onStart={noop}
        onCancel={noop}
        loading={false}
        error={null}
      />
    )
    await userEvent.click(screen.getAllByRole('button', { name: /drop/i })[0])
    expect(onToggleDrop).toHaveBeenCalledWith(0)
  })

  it('shows an already-dropped entry as struck through with a Pick button', () => {
    const withDrop: RebaseCommitPlan[] = [{ ...plan[0], action: 'drop' }, plan[1]]
    render(
      <RebasePlanner
        ontoInput="main~2"
        onOntoInputChange={noop}
        onLoadPlan={noop}
        plan={withDrop}
        onMove={noop}
        onToggleDrop={noop}
        onStart={noop}
        onCancel={noop}
        loading={false}
        error={null}
      />
    )
    expect(screen.getByRole('button', { name: /^pick$/i })).toBeInTheDocument()
  })

  it('calls onStart when Start rebase is clicked', async () => {
    const onStart = vi.fn()
    render(
      <RebasePlanner
        ontoInput="main~2"
        onOntoInputChange={noop}
        onLoadPlan={noop}
        plan={plan}
        onMove={noop}
        onToggleDrop={noop}
        onStart={onStart}
        onCancel={noop}
        loading={false}
        error={null}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: /start rebase/i }))
    expect(onStart).toHaveBeenCalled()
  })

  it('renders an error message when present', () => {
    render(
      <RebasePlanner
        ontoInput=""
        onOntoInputChange={noop}
        onLoadPlan={noop}
        plan={[]}
        onMove={noop}
        onToggleDrop={noop}
        onStart={noop}
        onCancel={noop}
        loading={false}
        error="not an ancestor"
      />
    )
    expect(screen.getByText('not an ancestor')).toBeInTheDocument()
  })
})
