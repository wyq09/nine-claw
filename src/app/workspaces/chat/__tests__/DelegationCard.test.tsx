import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { DelegationCard } from '../DelegationCard'
import type { DelegationRunSegment } from '../../../../types'

vi.mock('../../../../lib/piClient', () => ({
  subscribeWorkspaceDelegateChunk: vi.fn(async () => () => {}),
  subscribeWorkspaceDelegateTerminal: vi.fn(async () => () => {}),
  subscribeWorkspaceDelegateTool: vi.fn(async () => () => {}),
  subscribeWorkspaceDelegateTurn: vi.fn(async () => () => {}),
  workspaceAbortDelegate: vi.fn(async () => {}),
  workspaceAugmentDelegate: vi.fn(async () => {}),
}))

const runningRun: DelegationRunSegment = {
  runId: 'run-1',
  assignee: 'ken',
  task: '调研最近 7 天发布的大模型，输出结构化结果。',
  status: 'running',
  output: '',
  turns: [{ index: 0, kind: 'thinking', summary: '需要多源搜索确认发布时间' }],
  toolCalls: [
    {
      index: 0,
      toolCallId: 'tool-1',
      toolName: 'web_search',
      argsDigest: 'latest LLM releases',
      status: 'running',
    },
  ],
}

describe('DelegationCard', () => {
  it('renders a live sub-agent session with identity, task, trace summary, and processing footer', () => {
    render(
      <DelegationCard
        workspaceId="workspace-1"
        run={runningRun}
        resolveSpeaker={() => ({
          name: '陈知远(Ken Chen)',
          role: 'member',
          accentColor: '#4b83aa',
          avatarEmoji: 'K',
        })}
      />,
    )

    expect(screen.getAllByText('陈知远(Ken Chen)')[0]).toBeInTheDocument()
    expect(screen.getByText('工作中')).toBeInTheDocument()
    expect(screen.getByText(runningRun.task)).toBeInTheDocument()
    expect(screen.getByText('1 次工具调用 · 思考 1 轮')).toBeInTheDocument()
    expect(screen.getByText('正在处理')).toBeInTheDocument()
  })

  it('expands live trace from the header chevron', () => {
    render(<DelegationCard workspaceId="workspace-1" run={runningRun} />)

    fireEvent.click(screen.getByLabelText('展开运行细节'))

    expect(screen.getByText('web_search')).toBeInTheDocument()
    expect(screen.getByText('latest LLM releases')).toBeInTheDocument()
    expect(screen.getByText('思考回合 #1')).toBeInTheDocument()
  })
})
