import { describe, it, expect, vi, beforeEach } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import {
  compactDesktopSessionBeforeModelSwitch,
  exportAgentPackage,
  getSessionContextStats,
  listDefaultAgentPresets,
  resetAgentToDefaultPreset,
  streamPiPrompt,
  syncRuntimeParameters,
  widgetCancelResponse,
  widgetSubmitResponse,
} from '../piClient'

describe('getSessionContextStats', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('calls invoke with correct command name and args', async () => {
    const mockResult = {
      sessionId: 'abc',
      usedTokens: 1000,
      contextWindow: 128000,
      inputTokens: 600,
      outputTokens: 400,
      model: 'gpt-4',
      source: 'auto',
    }
    vi.mocked(invoke).mockResolvedValue(mockResult)

    const result = await getSessionContextStats('abc')
    expect(invoke).toHaveBeenCalledWith('get_session_context_stats', { sessionId: 'abc' })
    expect(result).toEqual(mockResult)
  })

  it('propagates errors from invoke', async () => {
    vi.mocked(invoke).mockRejectedValue(new Error('RPC failed'))
    await expect(getSessionContextStats('abc')).rejects.toThrow('RPC failed')
  })
})

describe('streamPiPrompt / syncRuntimeParameters', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('passes runtimeParameters to stream_pi_prompt', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined)
    const rp = {
      maxAgentToolRoundsPerDialogue: 42,
      streamDisconnectMaxRetries: 2,
      llmOuterMaxAttempts: 6,
    }
    await streamPiPrompt('hi', {
      sessionId: 's1',
      runtimeParameters: rp,
    })
    expect(invoke).toHaveBeenCalledWith(
      'stream_pi_prompt',
      expect.objectContaining({
        prompt: 'hi',
        sessionId: 's1',
        runtimeParameters: rp,
      }),
    )
  })

  it('calls sync_runtime_parameters with nested payload', async () => {
    const returned = {
      maxAgentToolRoundsPerDialogue: 80,
      streamDisconnectMaxRetries: 3,
      llmOuterMaxAttempts: 8,
    }
    vi.mocked(invoke).mockResolvedValue(returned)

    const rp = {
      maxAgentToolRoundsPerDialogue: 10,
      streamDisconnectMaxRetries: 1,
      llmOuterMaxAttempts: 5,
    }
    const result = await syncRuntimeParameters(rp)

    expect(invoke).toHaveBeenCalledWith('sync_runtime_parameters', { payload: rp })
    expect(result).toEqual(returned)
  })

  it('calls model-switch compression command before changing session model', async () => {
    vi.mocked(invoke).mockResolvedValue({ compressed: true, reason: 'model_switch_compressed' })

    const result = await compactDesktopSessionBeforeModelSwitch({
      sessionId: 's1',
      workspaceId: 'w1',
      currentModel: 'old-model',
      nextModel: 'new-model',
    })

    expect(invoke).toHaveBeenCalledWith('compact_desktop_session_before_model_switch', {
      sessionId: 's1',
      workspaceId: 'w1',
      currentModel: 'old-model',
      nextModel: 'new-model',
    })
    expect(result.compressed).toBe(true)
  })
})

describe('widget response commands', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('submits widget answers through invoke', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined)

    await widgetSubmitResponse({
      widgetId: 'ask-1',
      kind: 'ask_user',
      answers: [{ questionId: 'format', value: 'doc' }],
    })

    expect(invoke).toHaveBeenCalledWith('widget_submit_response', {
      payload: {
        widgetId: 'ask-1',
        kind: 'ask_user',
        answers: [{ questionId: 'format', value: 'doc' }],
      },
    })
  })

  it('cancels widgets through invoke', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined)

    await widgetCancelResponse({
      widgetId: 'ask-1',
      kind: 'ask_user',
    })

    expect(invoke).toHaveBeenCalledWith('widget_cancel_response', {
      payload: {
        widgetId: 'ask-1',
        kind: 'ask_user',
      },
    })
  })
})

describe('default agent preset commands', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('lists default agent presets through invoke', async () => {
    vi.mocked(invoke).mockResolvedValue([{ id: 'jiujiexia', isDefault: true }])

    const result = await listDefaultAgentPresets()

    expect(invoke).toHaveBeenCalledWith('list_default_agent_presets')
    expect(result).toEqual([{ id: 'jiujiexia', isDefault: true }])
  })

  it('resets an agent to its default preset through invoke', async () => {
    vi.mocked(invoke).mockResolvedValue({ id: 'jiujiexia', name: '九节虾' })

    const result = await resetAgentToDefaultPreset('jiujiexia')

    expect(invoke).toHaveBeenCalledWith('reset_agent_to_default_preset', { agentId: 'jiujiexia' })
    expect(result).toEqual({ id: 'jiujiexia', name: '九节虾' })
  })
})

describe('agent package export defaults to redacted secrets', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('sends includeSecrets=false when the caller omits it', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined)

    await exportAgentPackage({
      agentId: 'agent-1',
      destPath: '/tmp/agent.zip',
      includeSharedRoot: false,
    })

    expect(invoke).toHaveBeenCalledWith('export_agent_package', {
      agentId: 'agent-1',
      destPath: '/tmp/agent.zip',
      includeSecrets: false,
      includeSharedRoot: false,
    })
  })

  it('still honors an explicit includeSecrets=true', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined)

    await exportAgentPackage({
      agentId: 'agent-1',
      destPath: '/tmp/agent.zip',
      includeSecrets: true,
      includeSharedRoot: true,
    })

    expect(invoke).toHaveBeenCalledWith('export_agent_package', {
      agentId: 'agent-1',
      destPath: '/tmp/agent.zip',
      includeSecrets: true,
      includeSharedRoot: true,
    })
  })
})

