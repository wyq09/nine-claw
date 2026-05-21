import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { NineClawRouteOutletProps } from '../NineClawRouteOutlet'
import type { HistoryItem, PersistedChatAttachment } from '../../../types'

const { mockUsePiAgent, buildUsePiAgentReturn, mockUseComposerAttachments } = vi.hoisted(() => {
  const buildUsePiAgentReturn = () => ({
    error: '',
    loading: false,
    runtimeReady: true,
    runtimeBlockingReason: null,
    runningHistoryIds: [],
    streamingHistoryIds: [],
    history: [] as import('../../../types').HistoryItem[],
    activeHistoryId: '',
    activeHistoryItem: null as import('../../../types').HistoryItem | null,
    submitPrompt: vi.fn(),
    submitPromptInNewSession: vi.fn(),
    submitWidgetResponse: vi.fn(),
    cancelWidgetResponse: vi.fn(),
    abortPrompt: vi.fn(),
    resetSessionDraft: vi.fn(),
    createEmptySession: vi.fn(),
    selectHistoryItem: vi.fn(),
    clearHistory: vi.fn(),
    deleteHistoryItem: vi.fn(),
    updateSessionLlm: vi.fn(),
    sanitizeSessionLlmReferences: vi.fn(),
  })

  return {
    mockUsePiAgent: vi.fn(() => buildUsePiAgentReturn()),
    buildUsePiAgentReturn,
    mockUseComposerAttachments: vi.fn(() => ({
      attachments: [] as import('../../../types').PersistedChatAttachment[],
      uploading: false,
      error: '',
      fileInputRef: { current: null },
      openFilePicker: vi.fn(),
      handleFileInputChange: vi.fn(),
      handleComposerPaste: vi.fn(),
      removeAttachment: vi.fn(),
      clearAttachments: vi.fn(),
      clearError: vi.fn(),
    })),
  }
})

vi.mock('../../../hooks/usePiAgent', () => ({
  usePiAgent: mockUsePiAgent,
}))

vi.mock('../../../hooks/useComposerAttachments', () => ({
  useComposerAttachments: mockUseComposerAttachments,
}))

vi.mock('../../../hooks/useToast', () => ({
  useToast: () => ({
    success: vi.fn(),
    error: vi.fn(),
  }),
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

vi.mock('../../../lib/llmLogExportClient', () => ({
  llmLogExportGet: vi.fn().mockResolvedValue(false),
  llmLogExportSet: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('../../../lib/imageGenerationClient', () => ({
  loadImageGenerationPreferences: vi.fn().mockResolvedValue(null),
  saveImageGenerationPreferences: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('../../../lib/piClient', () => ({
  deleteAgent: vi.fn().mockResolvedValue(undefined),
  botSendMedia: vi.fn().mockResolvedValue(undefined),
  botSendMessage: vi.fn().mockResolvedValue(undefined),
  botLoginWechat: vi.fn().mockResolvedValue(undefined),
  botStartLark: vi.fn().mockResolvedValue(undefined),
  botStartWechat: vi.fn().mockResolvedValue(undefined),
  botStopLark: vi.fn().mockResolvedValue(undefined),
  botStopWechat: vi.fn().mockResolvedValue(undefined),
  createAgent: vi.fn().mockResolvedValue(null),
  getDefaultAgent: vi.fn().mockResolvedValue(null),
  installSystemSkill: vi.fn().mockResolvedValue(undefined),
  listInstalledSkills: vi.fn().mockResolvedValue([]),
  listAgents: vi.fn().mockResolvedValue([]),
  loadNetworkProxySettings: vi.fn().mockResolvedValue({}),
  loadProviderPreferences: vi.fn().mockResolvedValue({
    providerConfigs: null,
    customProviderMeta: null,
  }),
  listSystemSkillCatalog: vi.fn().mockResolvedValue({ skills: [], categories: [] }),
  readAgentWorkspaceBundle: vi.fn().mockResolvedValue(null),
  readAgentWorkspaceFile: vi.fn().mockResolvedValue(''),
  rotateAgentPeerInboundSecret: vi.fn().mockResolvedValue(undefined),
  saveProviderPreferences: vi.fn().mockResolvedValue(undefined),
  setDefaultAgent: vi.fn().mockResolvedValue(undefined),
  subscribeQrCode: vi.fn().mockResolvedValue(() => {}),
  subscribeBotStatus: vi.fn().mockResolvedValue(() => {}),
  updateAgent: vi.fn().mockResolvedValue(null),
  workspaceListMembers: vi.fn().mockResolvedValue([]),
  workspaceRunDelegateTask: vi.fn().mockResolvedValue(undefined),
  writeAgentWorkspaceFile: vi.fn().mockResolvedValue(undefined),
  syncRuntimeParameters: vi.fn().mockResolvedValue({}),
}))

vi.mock('../NineClawAppChrome', () => ({
  NineClawAppChrome: ({ routeOutlet }: { routeOutlet: ReactNode }) => <div data-testid="chrome">{routeOutlet}</div>,
}))

vi.mock('../NineClawRouteOutlet', () => ({
  NineClawRouteOutlet: (props: NineClawRouteOutletProps) => (
    <div data-testid="route-outlet">
      <button type="button" onClick={() => void props.onChatSubmit(' hello from chat ')}>
        submit chat
      </button>
    </div>
  ),
}))

vi.mock('../AgentApprovalDialog', () => ({
  AgentApprovalDialog: () => null,
}))

import { NineClawApp } from '../NineClawApp'
import { botSendMedia, botSendMessage } from '../../../lib/piClient'

const buildHistoryItem = (overrides: Partial<HistoryItem> = {}): HistoryItem => ({
  id: 'session-1',
  title: 'Session',
  status: 'done',
  createdAt: 1,
  updatedAt: 1,
  turns: [],
  ...overrides,
})

const buildAttachment = (overrides: Partial<PersistedChatAttachment> = {}): PersistedChatAttachment => ({
  id: 'attachment-1',
  fileName: 'doc.txt',
  filePath: '/tmp/doc.txt',
  mimeType: 'text/plain',
  size: 4,
  kind: 'file',
  ...overrides,
})

describe('NineClawApp', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('localStorage', {
      getItem: vi.fn((key: string) => {
        if (key === 'nineclaw.provider-configs.v1') {
          return JSON.stringify({
            openai: {
              enabled: true,
              added: true,
              apiFormat: 'openai',
              baseUrl: 'https://api.openai.test/v1',
              apiKey: 'test-key',
              model: 'gpt-test',
              note: '',
              displayName: '',
              status: '已配置',
            },
          })
        }
        return null
      }),
      setItem: vi.fn(),
      removeItem: vi.fn(),
    })
    mockUsePiAgent.mockReturnValue(buildUsePiAgentReturn())
    mockUseComposerAttachments.mockReturnValue({
      attachments: [] as PersistedChatAttachment[],
      uploading: false,
      error: '',
      fileInputRef: { current: null },
      openFilePicker: vi.fn(),
      handleFileInputChange: vi.fn(),
      handleComposerPaste: vi.fn(),
      removeAttachment: vi.fn(),
      clearAttachments: vi.fn(),
      clearError: vi.fn(),
    })
  })

  it('renders and passes a concrete notification setting into usePiAgent', () => {
    expect(() => render(<NineClawApp />)).not.toThrow()
    expect(mockUsePiAgent).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        notificationEnabled: expect.any(Boolean),
      }),
    )
  })

  it('routes bot conversation text through the bot channel instead of desktop Pi', async () => {
    const submitPrompt = vi.fn()
    mockUsePiAgent.mockReturnValue({
      ...buildUsePiAgentReturn(),
      activeHistoryId: 'bot-session',
      activeHistoryItem: buildHistoryItem({
        id: 'bot-session',
        botTarget: { channelId: 'wechat', userId: 'user-1' },
      }),
      submitPrompt,
    })

    render(<NineClawApp />)
    fireEvent.click(screen.getByRole('button', { name: 'submit chat' }))

    await waitFor(() => {
      expect(botSendMessage).toHaveBeenCalledWith('wechat', 'user-1', 'hello from chat')
    })
    expect(submitPrompt).not.toHaveBeenCalled()
    expect(botSendMedia).not.toHaveBeenCalled()
  })

  it('routes bot conversation attachments through the bot channel with text', async () => {
    const submitPrompt = vi.fn()
    const clearAttachments = vi.fn()
    mockUsePiAgent.mockReturnValue({
      ...buildUsePiAgentReturn(),
      activeHistoryId: 'bot-session',
      activeHistoryItem: buildHistoryItem({
        id: 'bot-session',
        botTarget: { channelId: 'wechat', userId: 'user-1' },
      }),
      submitPrompt,
    })
    mockUseComposerAttachments.mockReturnValue({
      attachments: [buildAttachment()],
      uploading: false,
      error: '',
      fileInputRef: { current: null },
      openFilePicker: vi.fn(),
      handleFileInputChange: vi.fn(),
      handleComposerPaste: vi.fn(),
      removeAttachment: vi.fn(),
      clearAttachments,
      clearError: vi.fn(),
    })

    render(<NineClawApp />)
    fireEvent.click(screen.getByRole('button', { name: 'submit chat' }))

    await waitFor(() => {
      expect(botSendMessage).toHaveBeenCalledWith('wechat', 'user-1', 'hello from chat')
      expect(botSendMedia).toHaveBeenCalledWith('wechat', 'user-1', 'file', '/tmp/doc.txt')
    })
    expect(clearAttachments).toHaveBeenCalled()
    expect(submitPrompt).not.toHaveBeenCalled()
  })

  it('keeps standalone chat text on the desktop Pi path', async () => {
    const submitPrompt = vi.fn()
    mockUsePiAgent.mockReturnValue({
      ...buildUsePiAgentReturn(),
      activeHistoryId: 'desktop-session',
      activeHistoryItem: buildHistoryItem({ id: 'desktop-session' }),
      submitPrompt,
    })

    render(<NineClawApp />)
    fireEvent.click(screen.getByRole('button', { name: 'submit chat' }))

    await waitFor(() => {
      expect(submitPrompt).toHaveBeenCalled()
    })
    expect(botSendMessage).not.toHaveBeenCalled()
  })
})
