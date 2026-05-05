import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import { AgentLoopBlock } from '../agent-loop/AgentLoopBlock'
import type { AgentLoopSegment } from '../../types'

vi.mock('../../lib/piClient', () => ({
  subscribeAgentLoopIterationStart: vi.fn(async () => () => {}),
  subscribeAgentLoopIterationEnd: vi.fn(async () => () => {}),
  subscribeAgentLoopReviewRequest: vi.fn(async () => () => {}),
  agentLoopAbort: vi.fn(async () => {}),
  agentLoopRespondReview: vi.fn(async () => {}),
}))

const baseSegment: AgentLoopSegment = {
  type: 'agent_loop',
  loopId: 'test-loop',
  status: 'completed',
  totalIterations: 2,
  currentDepth: 0,
  iterations: [
    {
      iteration: 1,
      markerType: 'call',
      status: 'completed',
      delegate: {
        agentId: 'a1',
        agentName: 'Analyzer',
        task: 'analyze data',
        output: 'found 3 issues',
        durationMs: 5000,
      },
    },
    {
      iteration: 2,
      markerType: 'call',
      status: 'completed',
      delegate: {
        agentId: 'a2',
        agentName: 'Writer',
        task: 'write report',
        output: 'report generated',
        durationMs: 3000,
      },
    },
  ],
  startedAt: Date.now() - 8000,
  completedAt: Date.now(),
}

describe('AgentLoopBlock', () => {
  it('renders completed iterations', () => {
    render(<AgentLoopBlock segment={baseSegment} />)
    expect(screen.getByText('#1')).toBeInTheDocument()
    expect(screen.getByText('#2')).toBeInTheDocument()
    expect(screen.getByText('Analyzer')).toBeInTheDocument()
    expect(screen.getByText('Writer')).toBeInTheDocument()
  })

  it('shows completed status with delegate count', () => {
    render(<AgentLoopBlock segment={baseSegment} />)
    expect(screen.getByText(/Agent Loop 完成/)).toBeInTheDocument()
    expect(screen.getByText(/2\/2 次委派/)).toBeInTheDocument()
  })

  it('shows expand button', () => {
    render(<AgentLoopBlock segment={baseSegment} />)
    expect(screen.getByText('展开详情')).toBeInTheDocument()
  })

  it('renders running segment with abort button', () => {
    const runningSegment: AgentLoopSegment = {
      ...baseSegment,
      status: 'running',
      iterations: [
        {
          iteration: 1,
          markerType: 'call',
          status: 'running',
          delegate: {
            agentId: 'a1',
            agentName: 'Worker',
            task: 'processing',
          },
        },
      ],
    }
    render(<AgentLoopBlock segment={runningSegment} />)
    expect(screen.getByText(/Agent Loop 运行中/)).toBeInTheDocument()
    expect(screen.getByText('终止')).toBeInTheDocument()
  })

  it('renders batch iteration', () => {
    const batchSegment: AgentLoopSegment = {
      ...baseSegment,
      iterations: [
        {
          iteration: 1,
          markerType: 'batch',
          status: 'completed',
          batch: {
            delegates: [
              { agentId: 'a1', agentName: 'Agent-A', task: 'task-a', status: 'completed', output: 'ok', durationMs: 1000 },
              { agentId: 'a2', agentName: 'Agent-B', task: 'task-b', status: 'error', output: 'fail', durationMs: 2000 },
            ],
          },
        },
      ],
    }
    render(<AgentLoopBlock segment={batchSegment} />)
    expect(screen.getByText('并发子智能体')).toBeInTheDocument()
    expect(screen.getByText(/2 个 Agent 并发/)).toBeInTheDocument()
    expect(screen.getByText('Agent-A')).toBeInTheDocument()
    expect(screen.getByText('Agent-B')).toBeInTheDocument()
  })
})
