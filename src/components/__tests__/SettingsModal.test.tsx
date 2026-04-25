import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
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

vi.mock('../../lib/piClient', () => ({
  getPeerGatewayInfo: vi.fn().mockResolvedValue({
    envOverrideActive: false,
    listenAddress: '127.0.0.1:1052',
    inboundUrl: 'http://127.0.0.1:1052/inbound',
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
  testNetworkProxyConnection: vi.fn(),
  testLlmProviderConnection: vi.fn(),
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

const generalSettings: GeneralSettings = {
  language: '中文',
  launchOnStartup: false,
  useSystemProxy: false,
  customProxyUrl: '',
  submitShortcut: 'enter',
  llmCallLogDir: '',
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

describe('SettingsModal provider tabs', () => {
  beforeEach(() => {
    vi.clearAllMocks()
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
          systemSkillCatalog: { available: false, skills: [] },
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
          systemSkillCatalog: { available: false, skills: [] },
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
})
