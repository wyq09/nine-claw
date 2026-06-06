import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, act } from '@testing-library/react'
import { SettingsModal } from '../SettingsModal'
import type {
  AppearanceSettings,
  GeneralSettings,
  ProviderConfig,
  ProviderDefinition,
} from '../../types'
import type {
  ImageGenerationSystemConfig,
  ImageProviderConfig,
  ImageProviderDefinition,
} from '../../types/imageGeneration'
import {
  getNotificationPermissionState,
  openSystemNotificationSettings,
  requestNotificationPermission,
  sendTestNotification,
} from '../../lib/taskDeliveryNotification'

vi.mock('../../lib/piClient', () => ({
  getPeerGatewayInfo: vi.fn().mockResolvedValue({
    envOverrideActive: false,
    listenAddress: '127.0.0.1:1052',
    inboundUrl: 'http://127.0.0.1:1052/inbound',
  }),
  getEmbeddingStatus: vi.fn().mockResolvedValue({
    activeProviderId: null,
    mode: 'local',
    localModelReady: false,
    localModelPath: '',
    localDownloadState: 'idle',
    remoteConfigured: false,
    vectorCount: 0,
    providerCount: 0,
    message: '',
  }),
  loadEmbeddingSettings: vi.fn().mockResolvedValue({
    mode: 'local',
    remoteEndpoint: '',
    remoteModelName: '',
    remoteApiKey: '',
    remoteDimension: 512,
  }),
  loadNetworkProxySettings: vi.fn().mockResolvedValue({
    useSystemProxy: false,
    customProxyUrl: '',
  }),
  loadPeerGatewaySettings: vi.fn().mockResolvedValue({
    enabled: false,
    host: '127.0.0.1',
    port: 1052,
    publicBase: '',
  }),
  saveNetworkProxySettings: vi.fn(),
  savePeerGatewaySettings: vi.fn(),
  saveEmbeddingSettings: vi.fn(),
  testNetworkProxyConnection: vi.fn(),
  testLlmProviderConnection: vi.fn(),
  triggerEmbeddingReindex: vi.fn().mockResolvedValue({
    indexed: 0,
    skipped: 0,
    providerId: null,
    searchModeReady: false,
    message: '',
  }),
}))

vi.mock('../../lib/mcpClient', () => ({
  loadMcpSettings: vi.fn().mockResolvedValue({ servers: [] }),
  saveMcpSettings: vi.fn(async (settings) => settings),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}))

vi.mock('../../lib/llmLogExportClient', () => ({
  llmLogExportPreview: vi.fn().mockResolvedValue({
    file: null,
    tail: '',
  }),
}))

vi.mock('../../lib/appLogClient', () => ({
  appLogList: vi.fn().mockResolvedValue({
    dir: '/tmp/logs',
    files: [],
    totalBytes: 0,
  }),
  appLogRead: vi.fn().mockResolvedValue(''),
  appLogOpenDir: vi.fn(),
  appLogExportAll: vi.fn().mockResolvedValue(0),
}))

vi.mock('../../lib/taskDeliveryNotification', () => ({
  getNotificationPermissionState: vi.fn().mockResolvedValue('not_determined'),
  openSystemNotificationSettings: vi.fn().mockResolvedValue(undefined),
  requestNotificationPermission: vi.fn().mockResolvedValue('granted'),
  sendTestNotification: vi.fn().mockResolvedValue(undefined),
}))

const generalSettings: GeneralSettings = {
  language: '中文',
  launchOnStartup: false,
  useSystemProxy: false,
  customProxyUrl: '',
  submitShortcut: 'enter',
  llmCallLogDir: '',
  notificationEnabled: true,
  runtimeParameters: {
    maxAgentToolRoundsPerDialogue: 80,
    streamDisconnectMaxRetries: 3,
    llmOuterMaxAttempts: 8,
  },
}

const appearanceSettings: AppearanceSettings = {
  themeMode: 'dark',
  compactSidebar: false,
  sidebarCollapsed: false,
  showThinkingProcess: true,
  showExecutionRail: true,
  preferReducedMotion: false,
}

const llmProviderDefinition: ProviderDefinition = {
  id: 'openai',
  name: 'OpenAI',
  defaultBaseUrl: 'https://api.openai.com/v1',
  suggestedModel: 'gpt-4o-mini',
  description: 'OpenAI 官方接口',
  apiFormat: 'openai',
}

const llmProviderConfig: ProviderConfig = {
  added: true,
  enabled: true,
  apiFormat: 'openai',
  displayName: 'OpenAI',
  baseUrl: 'https://api.openai.com/v1',
  apiKey: '',
  model: 'gpt-4o-mini',
  maxContextTokens: 128000,
  note: '',
  status: '已配置',
}

const imageProviderDefinition: ImageProviderDefinition = {
  id: 'openai_image',
  name: 'OpenAI Images',
  description: 'OpenAI 原生图片生成接口',
  adapterType: 'openai_images',
  defaultBaseUrl: 'https://api.openai.com/v1',
  suggestedModel: 'gpt-image-1',
}

const imageProviderConfig: ImageProviderConfig = {
  adapterType: 'openai_images',
  displayName: '',
  baseUrl: 'https://api.openai.com/v1',
  apiKey: '',
  model: 'gpt-image-1',
  note: '',
  status: '已配置',
}

const imageGenerationSystem: ImageGenerationSystemConfig = {
  defaultProviderId: 'openai_image',
  size: '1024x1024',
  resolution: '1k',
  background: 'auto',
  outputFormat: 'png',
  quality: 'auto',
  count: 1,
}

const baseModalProps = {
  activeProviderBadge: '系统默认：OpenAI',
  allProviderDefinitions: [llmProviderDefinition],
  appearanceSettings,
  generalSettings,
  imageGenerationSystem,
  imageProviderConfigs: { openai_image: imageProviderConfig },
  imageProviderDefinitions: [imageProviderDefinition],
  onAddCustomProvider: vi.fn(),
  onSaveImageGenerationSettings: vi.fn().mockResolvedValue(undefined),
  onProviderConfigChange: vi.fn(),
  onClose: vi.fn(),
  onRemoveCustomProvider: vi.fn(),
  onSelectProvider: vi.fn(),
  onSelectTab: vi.fn(),
  providerConfigs: { openai: llmProviderConfig },
  selectedProviderConfig: llmProviderConfig,
  selectedProviderDefinition: llmProviderDefinition,
  selectedProviderId: 'openai',
  setAppearanceSettings: vi.fn(),
  setGeneralSettings: vi.fn(),
  skillsLibrary: {
    installedSkillCount: 0,
    installedSkills: [],
    onChangeTab: vi.fn(),
    onInstallByLink: vi.fn(),
    onInstallSystemSkill: vi.fn(),
    onRefresh: vi.fn(),
    sessionBusy: false,
    setSearch: vi.fn(),
    skillsError: '',
    skillsLoading: false,
    systemSkillCount: 0,
    systemSkillCatalog: { available: false, skills: [], message: '' },
    systemSkillInstallId: '',
    tab: 'installed' as const,
    skillSearch: '',
    visibleSystemSkills: [],
  },
  resourcesLibrary: {
    onSearch: vi.fn(),
    resourceSearch: '',
    visibleResources: [],
  },
}

describe('SettingsModal provider tabs', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('renders the shrimp tide theme option in the appearance tab', async () => {
    render(
      <SettingsModal
        {...baseModalProps}
        tab="appearance"
      />,
    )

    expect(screen.getByDisplayValue('暖墨深色')).toBeInTheDocument()
    expect(await screen.findByRole('option', { name: '虾游·潮汐间' })).toBeInTheDocument()
  })

  it('separates llm and image provider sections with nested tabs', () => {
    render(
      <SettingsModal
        activeProviderBadge="系统默认：OpenAI"
        allProviderDefinitions={[llmProviderDefinition]}
        appearanceSettings={appearanceSettings}
        generalSettings={generalSettings}
        imageGenerationSystem={imageGenerationSystem}
        imageProviderConfigs={{ openai_image: imageProviderConfig }}
        imageProviderDefinitions={[imageProviderDefinition]}
        onAddCustomProvider={vi.fn()}
        onSaveImageGenerationSettings={vi.fn().mockResolvedValue(undefined)}
        onProviderConfigChange={vi.fn()}
        onClose={vi.fn()}
        onRemoveCustomProvider={vi.fn()}
        onSelectProvider={vi.fn()}
        onSelectTab={vi.fn()}
        providerConfigs={{ openai: llmProviderConfig }}
        selectedProviderConfig={llmProviderConfig}
        selectedProviderDefinition={llmProviderDefinition}
        selectedProviderId="openai"
        setAppearanceSettings={vi.fn()}
        setGeneralSettings={vi.fn()}
        tab="providers"
        skillsLibrary={{
          installedSkillCount: 0,
          installedSkills: [],
          onChangeTab: vi.fn(),
          onInstallByLink: vi.fn(),
          onInstallSystemSkill: vi.fn(),
          onRefresh: vi.fn(),
          sessionBusy: false,
          setSearch: vi.fn(),
          skillsError: '',
          skillsLoading: false,
          systemSkillCount: 0,
          systemSkillCatalog: { available: false, skills: [], message: '' },
          systemSkillInstallId: '',
          tab: 'installed',
          skillSearch: '',
          visibleSystemSkills: [],
        }}
        resourcesLibrary={{
          onSearch: vi.fn(),
          resourceSearch: '',
          visibleResources: [],
        }}
      />,
    )

    expect(screen.getByRole('tab', { name: '普通 LLM' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByText('最大上下文窗口 (tokens)')).toBeInTheDocument()
    expect(screen.queryByText('图片生成网关')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('tab', { name: '图片大模型' }))

    expect(screen.getByRole('tab', { name: '图片大模型' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByText('默认图片提供方')).toBeInTheDocument()
    expect(screen.queryByText('最大上下文窗口 (tokens)')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '保存图片配置' })).toBeInTheDocument()
  })

  it('saves image provider draft only when save button is clicked', async () => {
    const onSaveImageGenerationSettings = vi.fn().mockResolvedValue(undefined)

    render(
      <SettingsModal
        activeProviderBadge="系统默认：OpenAI"
        allProviderDefinitions={[llmProviderDefinition]}
        appearanceSettings={appearanceSettings}
        generalSettings={generalSettings}
        imageGenerationSystem={imageGenerationSystem}
        imageProviderConfigs={{ openai_image: imageProviderConfig }}
        imageProviderDefinitions={[imageProviderDefinition]}
        onAddCustomProvider={vi.fn()}
        onSaveImageGenerationSettings={onSaveImageGenerationSettings}
        onProviderConfigChange={vi.fn()}
        onClose={vi.fn()}
        onRemoveCustomProvider={vi.fn()}
        onSelectProvider={vi.fn()}
        onSelectTab={vi.fn()}
        providerConfigs={{ openai: llmProviderConfig }}
        selectedProviderConfig={llmProviderConfig}
        selectedProviderDefinition={llmProviderDefinition}
        selectedProviderId="openai"
        setAppearanceSettings={vi.fn()}
        setGeneralSettings={vi.fn()}
        tab="providers"
        skillsLibrary={{
          installedSkillCount: 0,
          installedSkills: [],
          onChangeTab: vi.fn(),
          onInstallByLink: vi.fn(),
          onInstallSystemSkill: vi.fn(),
          onRefresh: vi.fn(),
          sessionBusy: false,
          setSearch: vi.fn(),
          skillsError: '',
          skillsLoading: false,
          systemSkillCount: 0,
          systemSkillCatalog: { available: false, skills: [], message: '' },
          systemSkillInstallId: '',
          tab: 'installed',
          skillSearch: '',
          visibleSystemSkills: [],
        }}
        resourcesLibrary={{
          onSearch: vi.fn(),
          resourceSearch: '',
          visibleResources: [],
        }}
      />,
    )

    fireEvent.click(screen.getByRole('tab', { name: '图片大模型' }))
    fireEvent.change(screen.getByDisplayValue('1024x1024'), { target: { value: '1536x1024' } })

    expect(onSaveImageGenerationSettings).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: '保存图片配置' }))

    expect(onSaveImageGenerationSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        openai_image: expect.objectContaining({
          model: 'gpt-image-1',
        }),
      }),
      expect.objectContaining({
        size: '1536x1024',
      }),
    )
  })

  it('renders parameters tab with agent loop and LLM retry fields', () => {
    render(
      <SettingsModal
        activeProviderBadge="系统默认：OpenAI"
        allProviderDefinitions={[llmProviderDefinition]}
        appearanceSettings={appearanceSettings}
        generalSettings={generalSettings}
        imageGenerationSystem={imageGenerationSystem}
        imageProviderConfigs={{ openai_image: imageProviderConfig }}
        imageProviderDefinitions={[imageProviderDefinition]}
        onAddCustomProvider={vi.fn()}
        onSaveImageGenerationSettings={vi.fn().mockResolvedValue(undefined)}
        onProviderConfigChange={vi.fn()}
        onClose={vi.fn()}
        onRemoveCustomProvider={vi.fn()}
        onSelectProvider={vi.fn()}
        onSelectTab={vi.fn()}
        providerConfigs={{ openai: llmProviderConfig }}
        selectedProviderConfig={llmProviderConfig}
        selectedProviderDefinition={llmProviderDefinition}
        selectedProviderId="openai"
        setAppearanceSettings={vi.fn()}
        setGeneralSettings={vi.fn()}
        tab="parameters"
        skillsLibrary={{
          installedSkillCount: 0,
          installedSkills: [],
          onChangeTab: vi.fn(),
          onInstallByLink: vi.fn(),
          onInstallSystemSkill: vi.fn(),
          onRefresh: vi.fn(),
          sessionBusy: false,
          setSearch: vi.fn(),
          skillsError: '',
          skillsLoading: false,
          systemSkillCount: 0,
          systemSkillCatalog: { available: false, skills: [], message: '' },
          systemSkillInstallId: '',
          tab: 'installed',
          skillSearch: '',
          visibleSystemSkills: [],
        }}
        resourcesLibrary={{
          onSearch: vi.fn(),
          resourceSearch: '',
          visibleResources: [],
        }}
      />,
    )

    expect(screen.getByRole('heading', { level: 2, name: '参数' })).toBeInTheDocument()
    expect(screen.getByLabelText('最大迭代次数')).toHaveDisplayValue('80')
    expect(screen.getByLabelText('流式中断重试')).toHaveDisplayValue('3')
    expect(screen.getByLabelText('LLM 外层最大重试次数')).toHaveDisplayValue('8')
  })

  it('renders the MCP tab entry and panel', async () => {
    render(
      <SettingsModal
        {...baseModalProps}
        tab="mcp"
      />,
    )

    expect(screen.getByRole('button', { name: 'MCP' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 2, name: 'MCP' })).toBeInTheDocument()
    expect(await screen.findByText('MCP 接入')).toBeInTheDocument()
  })

  it('requests system notification permission when the user enables notifications', async () => {
    const setGeneralSettings = vi.fn()
    vi.mocked(getNotificationPermissionState).mockResolvedValue('not_determined')
    vi.mocked(requestNotificationPermission).mockResolvedValue('granted')

    render(
      <SettingsModal
        {...baseModalProps}
        generalSettings={{ ...generalSettings, notificationEnabled: false }}
        setGeneralSettings={setGeneralSettings}
        tab="general"
      />,
    )

    const notificationLabel = screen.getByText('回复完成通知')
    const notificationRow = notificationLabel.closest('.settings-row.switch')
    const toggle = notificationRow?.querySelector('.toggle')
    expect(toggle).not.toBeNull()

    await act(async () => {
      fireEvent.click(toggle as HTMLButtonElement)
    })

    expect(getNotificationPermissionState).toHaveBeenCalled()
    expect(requestNotificationPermission).toHaveBeenCalledTimes(1)
    expect(openSystemNotificationSettings).not.toHaveBeenCalled()
    expect(setGeneralSettings).toHaveBeenCalledWith(expect.any(Function))
  })

  it('requests permission when the app setting is enabled but system permission is missing', async () => {
    const setGeneralSettings = vi.fn()
    vi.mocked(getNotificationPermissionState).mockResolvedValue('not_determined')
    vi.mocked(requestNotificationPermission).mockResolvedValue('granted')

    render(
      <SettingsModal
        {...baseModalProps}
        generalSettings={{ ...generalSettings, notificationEnabled: true }}
        setGeneralSettings={setGeneralSettings}
        tab="general"
      />,
    )

    const notificationLabel = screen.getByText('回复完成通知')
    const notificationRow = notificationLabel.closest('.settings-row.switch')
    const toggle = notificationRow?.querySelector('.toggle')
    expect(toggle).not.toBeNull()

    await act(async () => {
      fireEvent.click(toggle as HTMLButtonElement)
    })

    expect(requestNotificationPermission).toHaveBeenCalledTimes(1)
    expect(openSystemNotificationSettings).not.toHaveBeenCalled()
    expect(setGeneralSettings).toHaveBeenCalledWith(expect.any(Function))
  })

  it('can send a test notification from settings', async () => {
    render(
      <SettingsModal
        {...baseModalProps}
        generalSettings={{ ...generalSettings, notificationEnabled: true }}
        tab="general"
      />,
    )

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '发送测试通知' }))
    })

    expect(sendTestNotification).toHaveBeenCalledTimes(1)
  })
})
