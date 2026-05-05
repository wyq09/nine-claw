import { fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { ToolCallEntry } from '../../../types'
import { TurnExecutionRail } from '../TurnAndTools'
import {
  DelegateToolResultCard,
  isDelegateToolName,
  parseDelegateToolPayload,
  parseDelegateToolResult,
} from '../delegateToolResult'

vi.mock('../../../components/MarkdownRenderer', () => ({
  default: ({ content }: { content: string }) => <div>{content}</div>,
}))

describe('delegateToolResult helpers', () => {
  it('parses delegate payload and result metadata', () => {
    expect(parseDelegateToolPayload('{"role":"ColorMaster","task":"出来打个招呼"}')).toEqual({
      role: 'ColorMaster',
      task: '出来打个招呼',
    })
    expect(parseDelegateToolResult('[Agent: ColorMaster | Time: 24.3s]\n你好，群哥')).toEqual({
      agentName: 'ColorMaster',
      durationLabel: '24.3s',
      body: '你好，群哥',
    })
  })

  it('recognizes delegate-style tools only', () => {
    expect(isDelegateToolName('agent_delegate')).toBe(true)
    expect(isDelegateToolName('AGENT_SPAWN')).toBe(true)
    expect(isDelegateToolName('web_search')).toBe(false)
  })
})

describe('DelegateToolResultCard', () => {
  it('renders the friendlier delegate card header and content', async () => {
    render(
      <DelegateToolResultCard
        agentName="ColorMaster"
        durationLabel="24.3s"
        task="出来打个招呼"
        body="嘿群哥，我是 ColorMaster。"
      />,
    )

    expect(screen.getByText('ColorMaster')).toBeInTheDocument()
    expect(screen.getByText('已完成')).toBeInTheDocument()
    expect(screen.getByText('结果 >')).toBeInTheDocument()
    expect(screen.getByText('出来打个招呼')).toBeInTheDocument()
    expect(await screen.findByText('嘿群哥，我是 ColorMaster。')).toBeInTheDocument()
  })

  it('collapses result body from the 结果 button', async () => {
    function ControlledCard() {
      const [open, setOpen] = useState(true)
      return (
        <DelegateToolResultCard
          agentName="ColorMaster"
          durationLabel="24.3s"
          task="出来打个招呼"
          body="嘿群哥，我是 ColorMaster。"
          open={open}
          onToggleOpen={() => setOpen((current) => !current)}
        />
      )
    }

    render(
      <ControlledCard />,
    )

    expect(await screen.findByText('嘿群哥，我是 ColorMaster。')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '折叠结果' }))
    expect(screen.queryByText('嘿群哥，我是 ColorMaster。')).not.toBeInTheDocument()
  })
})

describe('TurnExecutionRail', () => {
  it('defaults delegate input closed and result open, then toggles them independently', async () => {
    const toolCall: ToolCallEntry = {
      id: '1',
      toolCallId: 'call-1',
      toolName: 'agent_delegate',
      argsText: '{"role":"ColorMaster","task":"出来打个招呼"}',
      resultText: '[Agent: ColorMaster | Time: 24.3s]\n嘿群哥，我是 ColorMaster。',
      state: 'done',
      createdAt: 1,
      completedAt: 2,
    }

    render(
      <TurnExecutionRail
        toolCalls={[toolCall]}
        thinking=""
        showThinkingProcess={false}
        runningToolCount={0}
      />,
    )

    expect(screen.getByText('1 次工具调用')).toBeInTheDocument()
    expect(screen.getByText('ColorMaster')).toBeInTheDocument()
    expect(await screen.findByText('嘿群哥，我是 ColorMaster。')).toBeInTheDocument()
    expect(screen.queryByText(/"role": "ColorMaster"/)).not.toBeInTheDocument()
    expect(screen.getByText('ColorMaster').closest('.tool-io-panel-body-delegate')).not.toBeNull()

    fireEvent.click(screen.getByRole('button', { name: '展开 INPUT' }))
    expect(screen.getByText(/"role": "ColorMaster"/)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: '折叠结果' }))
    expect(screen.queryByText('嘿群哥，我是 ColorMaster。')).not.toBeInTheDocument()
  })
})
