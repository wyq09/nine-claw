import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { LlmLogPreview, parseLlmLogPreviewTail } from './LlmLogPreview'

describe('parseLlmLogPreviewTail', () => {
  it('splits text and json lines into structured entries', () => {
    const entries = parseLlmLogPreviewTail(
      [
        '[2026-05-01 00:00:21.204] [WARN ] [email/monitor] reconnect in 5000 ms',
        '{"kind":"delegate","callerAgentName":"主智能体","targetAgentName":"PsychMarketer","model":"gpt-5","error":"worker unavailable"}',
      ].join('\n'),
    )

    expect(entries).toHaveLength(2)
    expect(entries[0]).toMatchObject({ type: 'text', lineNumber: 1 })
    expect(entries[1]).toMatchObject({ type: 'json', lineNumber: 2, tone: 'error' })
    if (entries[1].type === 'json') {
      expect(entries[1].summary).toContain('delegate')
      expect(entries[1].summary).toContain('主智能体 -> PsychMarketer')
    }
  })
})

describe('LlmLogPreview', () => {
  it('renders json entries as expandable tree nodes', () => {
    render(
      <LlmLogPreview
        busy={false}
        file="/tmp/llm-trace-2026-05-01.jsonl"
        tail={[
          '{"kind":"delegate","callerAgentName":"主智能体","targetAgentName":"PsychMarketer","toolCalls":[{"name":"agent_delegate","ok":true}]}',
          'plain text line',
        ].join('\n')}
      />,
    )

    expect(screen.getByText('显示 2 / 2 行')).toBeInTheDocument()
    expect(screen.getByText('/tmp/llm-trace-2026-05-01.jsonl')).toBeInTheDocument()
    expect(screen.getByText('JSON')).toBeInTheDocument()
    expect(screen.getByText('TEXT')).toBeInTheDocument()
    expect(screen.getByText('callerAgentName')).toBeInTheDocument()
    expect(screen.getByText('targetAgentName')).toBeInTheDocument()
    expect(screen.getByText('toolCalls')).toBeInTheDocument()
    expect(screen.getByText('plain text line')).toBeInTheDocument()
  })
})
