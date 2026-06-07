import { render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { UsageStatsPanel } from '../UsageStatsPanel'
import { listTokenUsageRecords } from '../../lib/piClient'
import type { TokenUsageRecord } from '../../types'

vi.mock('../../lib/piClient', () => ({
  listTokenUsageRecords: vi.fn(),
}))

const sampleRecords: TokenUsageRecord[] = [
  {
    turnId: 't1',
    sessionId: 's1',
    turnCreatedAt: Date.parse('2026-06-05T10:00:00Z'),
    turnCompletedAt: Date.parse('2026-06-05T10:01:00Z'),
    agentId: 'agent-a',
    agentName: '助手 A',
    api: 'chat',
    provider: 'openai',
    model: 'gpt-4o-mini',
    inputTokens: 100,
    outputTokens: 50,
    cacheReadTokens: 0,
    cacheWriteTokens: 0,
    totalTokens: 150,
    usageTimestamp: Date.parse('2026-06-05T10:01:00Z'),
    recordedAt: Date.parse('2026-06-05T10:01:00Z'),
  },
  {
    turnId: 't2',
    sessionId: 's1',
    turnCreatedAt: Date.parse('2026-06-07T12:00:00Z'),
    turnCompletedAt: Date.parse('2026-06-07T12:02:00Z'),
    agentId: 'agent-a',
    agentName: '助手 A',
    api: 'chat',
    provider: 'openai',
    model: 'gpt-4.1',
    inputTokens: 400,
    outputTokens: 200,
    cacheReadTokens: 10,
    cacheWriteTokens: 0,
    totalTokens: 610,
    usageTimestamp: Date.parse('2026-06-07T12:02:00Z'),
    recordedAt: Date.parse('2026-06-07T12:02:00Z'),
  },
]

describe('UsageStatsPanel', () => {
  beforeEach(() => {
    vi.mocked(listTokenUsageRecords).mockResolvedValue(sampleRecords)
  })

  it('places refresh and export actions on the same row', async () => {
    const { container } = render(<UsageStatsPanel />)

    expect(await screen.findByRole('button', { name: '刷新' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '导出 CSV' })).toBeInTheDocument()
    expect(container.querySelector('.usage-dashboard-actions')).toBeTruthy()
  })

  it('highlights the active date preset with a dedicated selected style', async () => {
    const { container } = render(<UsageStatsPanel />)

    await screen.findByRole('button', { name: '刷新' })

    const allPreset = screen.getByRole('button', { name: '全部' })
    expect(allPreset).toHaveAttribute('aria-pressed', 'true')
    expect(allPreset).toHaveClass('active')
    expect(container.querySelector('.usage-date-presets')).toBeTruthy()
  })

  it('renders a scroll-contained daily chart aligned with day labels', async () => {
    const { container } = render(<UsageStatsPanel />)

    await waitFor(() => {
      expect(screen.getByText('按天')).toBeInTheDocument()
    })

    expect(container.querySelector('.usage-daily-chart-wrap')).toBeTruthy()
    expect(container.querySelectorAll('.usage-daily-chart-column')).toHaveLength(2)
    expect(screen.getByText('06-05')).toBeInTheDocument()
    expect(screen.getByText('06-07')).toBeInTheDocument()
  })
})
