import { describe, it, expect } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { SessionContextBadge } from '../SessionContextBadge'
import type { SessionContextState } from '../../app/lib/sessionContext'

const baseState: SessionContextState = {
  sessionId: 'test',
  model: 'gpt-4',
  usedTokens: 1000,
  contextWindow: 8000,
  percent: 12.5,
  stage: 'normal',
  source: 'auto',
  inputTokens: 600,
  outputTokens: 400,
  updatedAt: Date.now(),
}

describe('SessionContextBadge', () => {
  it('renders integer ring label without percent sign', () => {
    render(<SessionContextBadge state={baseState} loading={false} />)
    expect(screen.getByText('13')).toBeInTheDocument()
  })

  it('renders fallback when state is null', () => {
    render(<SessionContextBadge state={null} loading={false} />)
    expect(screen.getByText('--')).toBeInTheDocument()
  })

  it('renders loading indicator when loading and no state', () => {
    render(<SessionContextBadge state={null} loading={true} />)
    expect(screen.getByText('...')).toBeInTheDocument()
  })

  it('shows ring label even while loading if state exists', () => {
    render(<SessionContextBadge state={baseState} loading={true} />)
    expect(screen.getByText('13')).toBeInTheDocument()
  })

  it('applies normal class for normal stage', () => {
    const { container } = render(<SessionContextBadge state={baseState} loading={false} />)
    expect(container.querySelector('.context-badge-ring--normal')).toBeTruthy()
  })

  it('applies critical class for auto_compact stage', () => {
    const dangerState = { ...baseState, percent: 96, stage: 'auto_compact' as const }
    const { container } = render(<SessionContextBadge state={dangerState} loading={false} />)
    expect(container.querySelector('.context-badge-ring--critical')).toBeTruthy()
  })

  it('applies danger class for collapse stage', () => {
    const dangerState = { ...baseState, percent: 90, stage: 'collapse' as const }
    const { container } = render(<SessionContextBadge state={dangerState} loading={false} />)
    expect(container.querySelector('.context-badge-ring--danger')).toBeTruthy()
  })

  it('shows popover on mouse enter', () => {
    const { container } = render(<SessionContextBadge state={baseState} loading={false} />)
    const wrap = container.querySelector('.context-badge-wrap')
    expect(wrap).toBeTruthy()
    fireEvent.mouseEnter(wrap!)
    expect(screen.getByText('上下文占用')).toBeInTheDocument()
  })

  it('hides popover on mouse leave', () => {
    const { container } = render(<SessionContextBadge state={baseState} loading={false} />)
    const wrap = container.querySelector('.context-badge-wrap')
    fireEvent.mouseEnter(wrap!)
    expect(screen.getByText('上下文占用')).toBeInTheDocument()
    fireEvent.mouseLeave(wrap!)
    expect(screen.queryByText('上下文占用')).not.toBeInTheDocument()
  })
})
