import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react'
import {
  getPeerGatewayInfo,
  loadNetworkProxySettings,
  loadPeerGatewaySettings,
  saveNetworkProxySettings,
  savePeerGatewaySettings,
  testNetworkProxyConnection,
  testLlmProviderConnection,
} from '../lib/piClient'
import {
  getNotificationPermissionState,
  openSystemNotificationSettings,
  requestNotificationPermission,
  sendTestNotification,
  type NotificationPermissionState,
} from '../lib/taskDeliveryNotification'
import { THEME_OPTIONS } from '../theme/themePresets'
import { describeNetworkProxyMode } from '../app/lib'
import type {
  AgentRecord,
  AppearanceSettings,
  GeneralSettings,
  PeerGatewayInfo,
  PeerGatewaySettings,
  ProviderApiFormat,
  ProviderConfig,
  ProviderDefinition,
  ProviderId,
  SettingsTab,
} from '../types'
import type {
  ImageGenerationSystemConfig,
  ImageProviderConfig,
  ImageProviderDefinition,
} from '../types/imageGeneration'
import type { ResourcesViewProps, SkillsViewProps } from '../app/pages/LibraryAndTasks'
import { AppIcon, type IconName } from './AppIcon'
import { ApplicationLogsPanel } from './settings/ApplicationLogsPanel'
import { LlmLogPreview } from './settings/LlmLogPreview'
import { McpSettingsPanel } from './settings/McpSettingsPanel'
import { NumericDraftField } from './NumericDraftField'
import { ImageGenerationSettingsSection } from './settings/ImageGenerationSettingsSection'
import { ImageVisionSettingsPanel } from './settings/ImageVisionSettingsPanel'
import { UserMemorySettingsPanel } from './settings/UserMemorySettingsPanel'
import { VectorMemoryPanel } from './settings/VectorMemoryPanel'
import { UsageStatsPanel } from './UsageStatsPanel'
import { open } from '@tauri-apps/plugin-dialog'
import { llmLogExportPreview } from '../lib/llmLogExportClient'
import { filterLlmProviderDefinitions } from '../lib/providerListSearch'

const SkillsViewLazy = lazy(async () => {
  const module = await import('../app/pages/LibraryAndTasks')
  return { default: module.SkillsView }
})

const ResourcesViewLazy = lazy(async () => {
  const module = await import('../app/pages/LibraryAndTasks')
  return { default: module.ResourcesView }
})

type SettingsModalProps = {
  activeProviderBadge: string
  allProviderDefinitions: ProviderDefinition[]
  appearanceSettings: AppearanceSettings
  generalSettings: GeneralSettings
  imageGenerationSystem: ImageGenerationSystemConfig
  imageProviderConfigs: Record<string, ImageProviderConfig>
  imageProviderDefinitions: ImageProviderDefinition[]
  onAddCustomProvider: (name: string, description: string, apiFormat: ProviderApiFormat) => void
  onSaveImageGenerationSettings: (
    imageProviderConfigs: Record<string, ImageProviderConfig>,
    imageGenerationSystem: ImageGenerationSystemConfig,
  ) => Promise<void>
  onProviderConfigChange: (providerId: ProviderId, updates: Partial<ProviderConfig>) => void
  onDuplicateProvider: (providerId: ProviderId) => void
  onClose: () => void
  onRemoveCustomProvider: (providerId: ProviderId) => void
  onSelectProvider: (id: ProviderId) => void
  onSelectTab: (tab: SettingsTab) => void
  providerConfigs: Record<string, ProviderConfig>
  selectedProviderConfig: ProviderConfig
  selectedProviderDefinition: ProviderDefinition
  selectedProviderId: ProviderId
  setAppearanceSettings: (value: AppearanceSettings | ((previous: AppearanceSettings) => AppearanceSettings)) => void
  setGeneralSettings: (value: GeneralSettings | ((previous: GeneralSettings) => GeneralSettings)) => void
  tab: SettingsTab
  skillsLibrary: SkillsViewProps
  resourcesLibrary: ResourcesViewProps
  memoryAgents?: AgentRecord[]
  memoryDefaultAgentId?: string
}

function getProviderStatus(config: ProviderConfig, preserveVerifiedStatus = true): ProviderConfig['status'] {
  if (!config.baseUrl.trim() || !config.model.trim()) {
    return '未配置'
  }

  return preserveVerifiedStatus && config.status === '测试通过' ? '测试通过' : '已配置'
}

function providerDisplayName(definition: ProviderDefinition, config: ProviderConfig | undefined): string {
  const label = config?.displayName?.trim()
  if (label) {
    return label
  }
  return definition.name
}

function getSubmitShortcutLabel(shortcut: GeneralSettings['submitShortcut']): string {
  return shortcut === 'enter' ? 'Enter' : 'Ctrl / Cmd + Enter'
}

function isFnLikeKeyboardEvent(
  event: Pick<globalThis.KeyboardEvent, 'code' | 'key' | 'location' | 'getModifierState'>,
): boolean {
  if (event.key === 'Fn' || event.key === 'Function' || event.code === 'Fn') {
    return true
  }

  if (event.getModifierState?.('Fn')) {
    return true
  }

  return event.code === 'NumpadEnter' || event.location === globalThis.KeyboardEvent.DOM_KEY_LOCATION_NUMPAD
}

type SettingsTabButtonProps = {
  active: boolean
  icon: IconName
  label: string
  onClick: () => void
}

function SettingsTabButton({ active, icon, label, onClick }: SettingsTabButtonProps) {
  return (
    <button type="button" className={`settings-tab-button ${active ? 'active' : ''}`} onClick={onClick}>
      <AppIcon name={icon} size={20} />
      <span>{label}</span>
    </button>
  )
}

type ProviderSettingsMode = 'llm' | 'image' | 'vision'

type ProviderSettingsModeButtonProps = {
  active: boolean
  label: string
  onClick: () => void
}

function ProviderSettingsModeButton({ active, label, onClick }: ProviderSettingsModeButtonProps) {
  return (
    <button
      type="button"
      className={`provider-mode-tab ${active ? 'active' : ''}`}
      role="tab"
      aria-selected={active}
      onClick={onClick}
    >
      <span>{label}</span>
    </button>
  )
}

type SettingSwitchProps = {
  checked: boolean
  description: string
  label: string
  onChange: () => void
}

function SettingSwitch({ checked, description, label, onChange }: SettingSwitchProps) {
  return (
    <div className="settings-row switch">
      <div>
        <strong>{label}</strong>
        <p>{description}</p>
      </div>
      <Toggle checked={checked} onChange={onChange} />
    </div>
  )
}

type ToggleProps = {
  checked: boolean
  onChange: () => void
}

function Toggle({ checked, onChange }: ToggleProps) {
  return (
    <button type="button" className={`toggle ${checked ? 'checked' : ''}`} onClick={onChange} aria-pressed={checked}>
      <span />
    </button>
  )
}

/** 通知设置行：应用层开关 + 系统权限状态 + 跳转系统设置按钮 */
function NotificationSettingRow({ appEnabled, onSetAppEnabled }: {
  appEnabled: boolean
  onSetAppEnabled: (enabled: boolean) => void
}) {
  const [systemPerm, setSystemPerm] = useState<NotificationPermissionState>('not_determined')

  useEffect(() => {
    void getNotificationPermissionState().then(setSystemPerm)
  }, [appEnabled])

  const systemGranted = systemPerm === 'granted'
  // 开关状态 = 应用层开启 AND 系统已授权
  const effectiveChecked = appEnabled && systemGranted

  const handleToggle = async () => {
    if (!effectiveChecked) {
      // 用户想开启 → 当场向系统重新查询，避免依赖进入设置页时的旧状态
      const currentPerm = await getNotificationPermissionState()
      setSystemPerm(currentPerm)
      if (currentPerm !== 'granted') {
        // 尝试请求一次（可能触发系统弹窗）
        const result = await requestNotificationPermission()
        setSystemPerm(result)
        if (result !== 'granted') {
          // 系统没给权限 → 打开系统设置引导
          await openSystemNotificationSettings()
          return
        }
      }
      onSetAppEnabled(true)
    } else {
      onSetAppEnabled(false)
    }
  }

  return (
    <div className="settings-row switch">
      <div>
        <strong>回复完成通知</strong>
        <p>
          Agent 回复完成时推送系统通知，点击可直接跳转到对应会话
          {!systemGranted && (
            <span style={{ color: 'var(--text-danger, #f87171)', marginLeft: 6 }}>
              · 系统通知权限未开启
              <button
                type="button"
                style={{
                  background: 'none',
                  border: 'none',
                  color: 'var(--text-link, #60a5fa)',
                  cursor: 'pointer',
                  padding: '0 4px',
                  textDecoration: 'underline',
                  fontSize: 'inherit',
                }}
                onClick={() => void openSystemNotificationSettings()}
              >
                去开启
              </button>
            </span>
          )}
        </p>
        <button type="button" className="outline-button" onClick={() => void sendTestNotification()}>
          发送测试通知
        </button>
      </div>
      <Toggle checked={effectiveChecked} onChange={() => void handleToggle()} />
    </div>
  )
}

function getProviderFormatDefaults(apiFormat: ProviderApiFormat): { baseUrl: string; model: string; label: string } {
  if (apiFormat === 'anthropic') {
    return {
      baseUrl: 'https://api.anthropic.com',
      model: 'claude-sonnet-4-0',
      label: 'Anthropic Messages',
    }
  }

  return {
    baseUrl: 'https://api.openai.com/v1',
    model: 'gpt-4o-mini',
    label: 'OpenAI Chat Completions',
  }
}

export function SettingsModal({
  activeProviderBadge,
  allProviderDefinitions,
  appearanceSettings,
  generalSettings,
  imageGenerationSystem,
  imageProviderConfigs,
  imageProviderDefinitions,
  onAddCustomProvider,
  onSaveImageGenerationSettings,
  onProviderConfigChange,
  onDuplicateProvider,
  onClose,
  onRemoveCustomProvider,
  onSelectProvider,
  onSelectTab,
  providerConfigs,
  selectedProviderConfig,
  selectedProviderDefinition,
  selectedProviderId,
  setAppearanceSettings,
  setGeneralSettings,
  tab,
  skillsLibrary,
  resourcesLibrary,
  memoryAgents = [],
  memoryDefaultAgentId = '',
}: SettingsModalProps) {
  const [providerListSearch, setProviderListSearch] = useState('')
  const [providerAddMode, setProviderAddMode] = useState(false)
  const [customFormOpen, setCustomFormOpen] = useState(false)
  const [customName, setCustomName] = useState('')
  const [customDescription, setCustomDescription] = useState('')
  const [customApiFormat, setCustomApiFormat] = useState<ProviderApiFormat>('openai')
  const [providerTestLoading, setProviderTestLoading] = useState(false)
  const [providerTestNote, setProviderTestNote] = useState<{ kind: 'ok' | 'err'; text: string } | null>(null)
  const [providerDeleteConfirmId, setProviderDeleteConfirmId] = useState<ProviderId | null>(null)
  const [providerSettingsMode, setProviderSettingsMode] = useState<ProviderSettingsMode>('llm')
  const [draftImageProviderConfigs, setDraftImageProviderConfigs] =
    useState<Record<string, ImageProviderConfig>>(imageProviderConfigs)
  const [draftImageGenerationSystem, setDraftImageGenerationSystem] =
    useState<ImageGenerationSystemConfig>(imageGenerationSystem)
  const [imageSaveBusy, setImageSaveBusy] = useState(false)
  const [imageSaveNotice, setImageSaveNotice] = useState('')
  const [imageSaveError, setImageSaveError] = useState('')
  const [showApiKey, setShowApiKey] = useState(false)
  const [proxySaveError, setProxySaveError] = useState('')
  const [proxyTestLoading, setProxyTestLoading] = useState(false)
  const [proxyTestNote, setProxyTestNote] = useState<{ kind: 'ok' | 'err'; text: string } | null>(null)

  const [peerGatewayDraft, setPeerGatewayDraft] = useState<PeerGatewaySettings | null>(null)
  const [peerGatewayInfo, setPeerGatewayInfo] = useState<PeerGatewayInfo | null>(null)
  const [peerGatewayLoadError, setPeerGatewayLoadError] = useState('')
  const [peerGatewaySaveError, setPeerGatewaySaveError] = useState('')
  const [peerGatewaySaveNotice, setPeerGatewaySaveNotice] = useState('')
  const [peerGatewaySaving, setPeerGatewaySaving] = useState(false)
  const [logPreviewFile, setLogPreviewFile] = useState<string | null>(null)
  const [logPreviewTail, setLogPreviewTail] = useState('')
  const [logPreviewBusy, setLogPreviewBusy] = useState(false)

  const refreshPeerGateway = useCallback(() => {
    setPeerGatewayLoadError('')
    void Promise.all([loadPeerGatewaySettings(), getPeerGatewayInfo()])
      .then(([settings, info]) => {
        setPeerGatewayDraft(settings)
        setPeerGatewayInfo(info)
      })
      .catch((error: unknown) => {
        setPeerGatewayLoadError(error instanceof Error ? error.message : String(error))
      })
  }, [])

  useEffect(() => {
    refreshPeerGateway()
  }, [refreshPeerGateway])

  const refreshLogPreview = useCallback(async () => {
    setLogPreviewBusy(true)
    try {
      const p = await llmLogExportPreview()
      setLogPreviewFile(p.file)
      setLogPreviewTail(p.tail)
    } catch (error) {
      setLogPreviewFile(null)
      setLogPreviewTail(error instanceof Error ? error.message : String(error))
    } finally {
      setLogPreviewBusy(false)
    }
  }, [])

  useEffect(() => {
    void loadNetworkProxySettings()
      .then((settings) => {
        setGeneralSettings((previous) => ({
          ...previous,
          useSystemProxy: settings.useSystemProxy,
          customProxyUrl: settings.customProxyUrl,
        }))
      })
      .catch(() => {
        // Fall back to the local state when the backend is temporarily unavailable.
      })
  }, [setGeneralSettings])

  const addedProviders = allProviderDefinitions.filter((p) => providerConfigs[p.id]?.added)
  const availableProviders = allProviderDefinitions.filter((p) => !providerConfigs[p.id]?.added)
  const filteredAddedProviders = useMemo(
    () => filterLlmProviderDefinitions(addedProviders, providerConfigs, providerListSearch),
    [addedProviders, providerConfigs, providerListSearch],
  )
  const filteredAvailableProviders = useMemo(
    () => filterLlmProviderDefinitions(availableProviders, providerConfigs, providerListSearch),
    [availableProviders, providerConfigs, providerListSearch],
  )
  const proxyModeDescription = describeNetworkProxyMode({
    useSystemProxy: generalSettings.useSystemProxy,
    customProxyUrl: generalSettings.customProxyUrl,
  })

  const persistProxySettings = useCallback(
    async (nextSettings: GeneralSettings) => {
      setProxySaveError('')
      try {
        const saved = await saveNetworkProxySettings({
          useSystemProxy: nextSettings.useSystemProxy,
          customProxyUrl: nextSettings.customProxyUrl,
        })
        setGeneralSettings((previous) => {
          if (
            previous.useSystemProxy === saved.useSystemProxy &&
            previous.customProxyUrl === saved.customProxyUrl
          ) {
            return previous
          }
          return {
            ...previous,
            useSystemProxy: saved.useSystemProxy,
            customProxyUrl: saved.customProxyUrl,
          }
        })
      } catch (error) {
        setProxySaveError(error instanceof Error ? error.message : String(error))
      }
    },
    [setGeneralSettings],
  )

  const updateProxySettings = useCallback(
    (updates: Partial<Pick<GeneralSettings, 'useSystemProxy' | 'customProxyUrl'>>) => {
      const nextSettings: GeneralSettings = {
        ...generalSettings,
        ...updates,
      }
      setGeneralSettings((previous) => ({
        ...previous,
        ...updates,
      }))
      setProxyTestNote(null)
      void persistProxySettings(nextSettings)
    },
    [generalSettings, persistProxySettings, setGeneralSettings],
  )

  const handleProviderSelect = (providerId: ProviderId) => {
    if (providerAddMode) {
      onProviderConfigChange(providerId, { added: true })
      onSelectProvider(providerId)
      setProviderAddMode(false)
    } else {
      onSelectProvider(providerId)
    }
  }

  const handleRemoveProvider = (providerId: ProviderId) => {
    if (selectedProviderId === providerId) {
      const nextAdded = addedProviders.find((p) => p.id !== providerId)
      if (nextAdded) {
        onSelectProvider(nextAdded.id)
      }
    }
    if (providerId.startsWith('custom_')) {
      onRemoveCustomProvider(providerId)
    } else {
      onProviderConfigChange(providerId, { added: false, enabled: false })
    }
  }

  useEffect(() => {
    if (!providerDeleteConfirmId) {
      return
    }
    const onKeyDown = (event: Event) => {
      if (event instanceof KeyboardEvent && !isFnLikeKeyboardEvent(event) && event.key === 'Escape') {
        setProviderDeleteConfirmId(null)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [providerDeleteConfirmId])

  useEffect(() => {
    setShowApiKey(false)
    setProviderTestNote(null)
  }, [selectedProviderId])

  useEffect(() => {
    setDraftImageProviderConfigs(imageProviderConfigs)
  }, [imageProviderConfigs])

  useEffect(() => {
    setDraftImageGenerationSystem(imageGenerationSystem)
  }, [imageGenerationSystem])

  const handleSubmitCustomProvider = () => {
    const name = customName.trim()
    if (!name) {
      return
    }
    onAddCustomProvider(name, customDescription.trim(), customApiFormat)
    setCustomFormOpen(false)
    setCustomName('')
    setCustomDescription('')
    setCustomApiFormat('openai')
    setProviderAddMode(false)
  }

  const pendingDeleteLabel =
    providerDeleteConfirmId &&
    (() => {
      const def = allProviderDefinitions.find((p) => p.id === providerDeleteConfirmId)
      const cfg = providerConfigs[providerDeleteConfirmId]
      return def && cfg ? providerDisplayName(def, cfg) : providerDeleteConfirmId
    })()

  const currentTabTitle =
    tab === 'general'
      ? '常规'
      : tab === 'appearance'
        ? '外观与行为'
        : tab === 'parameters'
        ? '参数'
        : tab === 'providers'
          ? '模型提供方'
          : tab === 'mcp'
            ? 'MCP'
            : tab === 'usage'
              ? '用量统计'
              : tab === 'skills'
                ? '探索技能'
                : tab === 'resources'
                  ? '资源库'
                  : tab === 'memory'
                    ? '用户记忆'
                    : tab === 'logs'
                      ? '日志'
                      : '快捷键'

  const currentTabDescription =
    tab === 'general'
      ? '维护应用默认行为、语言偏好和对外接入配置。'
      : tab === 'appearance'
        ? '控制侧栏密度、执行轨迹和页面动态效果。'
        : tab === 'parameters'
        ? '统一管理 Agent 工具调用轮数上限、流式连接重试次数与大模型外层重试。'
        : tab === 'providers'
          ? '统一管理大模型接口、默认模型与连通性校验。'
          : tab === 'mcp'
            ? '集中管理通过 stdio、Streamable HTTP、SSE 接入的 MCP 服务，并供 mcp_tool 统一调用。'
            : tab === 'usage'
              ? '按模型、智能体与日期查看本地累计用量。'
              : tab === 'skills'
                ? '浏览已安装技能与系统技能库，通过链接安装或刷新目录。'
                : tab === 'resources'
                  ? '模板、规范与可复用资产，统一检索与编排入口。'
                  : tab === 'memory'
                    ? '按维度查看与工作区记忆库同步的结构化事实，亦可手动写入（与会话内记忆工具同源）。markdown 形态的长期人设仍在各智能体的 MEMORY / USER_MODEL 文件。'
                    : tab === 'logs'
                      ? '将已完成的 LLM 调用链（与调试面板同源）额外写入你选择的目录，便于外部工具分析。'
                      : '配置发送方式与常用桌面快捷操作。'
  const selectedProviderFormatDefaults = getProviderFormatDefaults(selectedProviderConfig.apiFormat)

  return (
    <>
      <div className="modal-backdrop">
        <div className="settings-dialog" role="dialog" aria-modal="true" aria-label="设置">
          <div className="settings-sidebar">
            <header className="settings-sidebar-head">
              <button type="button" className="settings-back-button" onClick={onClose}>
                <AppIcon name="arrow-left" size={18} />
                <span>返回应用</span>
              </button>
              <div className="settings-sidebar-title">
                <span className="settings-sidebar-kicker">系统偏好设置</span>
                <h2>设置</h2>
              </div>
            </header>

            <div className="settings-tab-list">
              <SettingsTabButton active={tab === 'general'} icon="settings" label="通用" onClick={() => onSelectTab('general')} />
              <SettingsTabButton active={tab === 'appearance'} icon="sparkles" label="个性化" onClick={() => onSelectTab('appearance')} />
              <SettingsTabButton active={tab === 'parameters'} icon="wrench" label="参数" onClick={() => onSelectTab('parameters')} />
              <SettingsTabButton active={tab === 'providers'} icon="provider" label="大模型 Provider" onClick={() => onSelectTab('providers')} />
              <SettingsTabButton active={tab === 'mcp'} icon="network" label="MCP" onClick={() => onSelectTab('mcp')} />
              <SettingsTabButton active={tab === 'usage'} icon="zap" label="用量统计" onClick={() => onSelectTab('usage')} />
              <SettingsTabButton active={tab === 'skills'} icon="puzzle" label="探索技能" onClick={() => onSelectTab('skills')} />
              <SettingsTabButton active={tab === 'resources'} icon="book" label="资源库" onClick={() => onSelectTab('resources')} />
              <SettingsTabButton active={tab === 'memory'} icon="spark" label="用户记忆" onClick={() => onSelectTab('memory')} />
              <SettingsTabButton active={tab === 'logs'} icon="folder" label="日志" onClick={() => onSelectTab('logs')} />
              <SettingsTabButton active={tab === 'shortcuts'} icon="keyboard" label="快捷键" onClick={() => onSelectTab('shortcuts')} />
            </div>
          </div>

          <div className="settings-content">
            <div className="settings-content-head">
              <div className="settings-content-head-copy">
                <h2>{currentTabTitle}</h2>
                <p>{currentTabDescription}</p>
              </div>
              <button type="button" className="icon-button subtle" onClick={onClose} aria-label="关闭设置">
                <AppIcon name="close" size={20} />
              </button>
            </div>

            {tab === 'general' ? (
              <div className="settings-section-stack">
                <div className="settings-row">
                  <div>
                    <strong>语言</strong>
                  </div>
                  <label className="select-field">
                    <select
                      value={generalSettings.language}
                      onChange={(event) =>
                        setGeneralSettings((previous) => ({
                          ...previous,
                          language: event.target.value as GeneralSettings['language'],
                        }))
                      }
                    >
                      <option value="中文">中文</option>
                      <option value="English">English</option>
                    </select>
                  </label>
                </div>

                <SettingSwitch
                  checked={generalSettings.launchOnStartup}
                  description="系统启动时自动运行应用"
                  label="开机自启动"
                  onChange={() =>
                    setGeneralSettings((previous) => ({
                      ...previous,
                      launchOnStartup: !previous.launchOnStartup,
                    }))
                  }
                />

                <NotificationSettingRow
                  appEnabled={generalSettings.notificationEnabled}
                  onSetAppEnabled={(enabled) =>
                    setGeneralSettings((previous) => ({
                      ...previous,
                      notificationEnabled: enabled,
                    }))
                  }
                />

                <SettingSwitch
                  checked={generalSettings.useSystemProxy}
                  description="开启后网络请求将跟随系统代理；若填写下方地址，自定义代理会优先生效"
                  label="使用系统代理"
                  onChange={() => updateProxySettings({ useSystemProxy: !generalSettings.useSystemProxy })}
                />

                <div className="settings-row">
                  <div>
                    <strong>自定义代理地址</strong>
                    <p>支持 `127.0.0.1:7890`、`http://host:port` 或 `socks5://host:port`，填写后优先于系统代理。</p>
                  </div>
                  <label className="input-field settings-peer-field">
                    <input
                      value={generalSettings.customProxyUrl}
                      placeholder="例如 127.0.0.1:7890"
                      onChange={(event) => updateProxySettings({ customProxyUrl: event.target.value })}
                    />
                  </label>
                </div>

                <div className="settings-row stacked">
                  <div>
                    <strong>当前代理策略</strong>
                    <p>{proxyModeDescription}</p>
                  </div>
                  <div className="settings-peer-actions">
                    <button
                      type="button"
                      className="outline-button"
                      disabled={proxyTestLoading}
                      onClick={async () => {
                        setProxyTestLoading(true)
                        setProxyTestNote(null)
                        try {
                          const message = await testNetworkProxyConnection({
                            useSystemProxy: generalSettings.useSystemProxy,
                            customProxyUrl: generalSettings.customProxyUrl,
                          })
                          setProxyTestNote({ kind: 'ok', text: message })
                        } catch (error) {
                          setProxyTestNote({
                            kind: 'err',
                            text: error instanceof Error ? error.message : String(error),
                          })
                        } finally {
                          setProxyTestLoading(false)
                        }
                      }}
                    >
                      <AppIcon name="broadcast" size={18} />
                      <span>{proxyTestLoading ? '测试中…' : '测试代理网络'}</span>
                    </button>
                  </div>
                </div>

                {proxySaveError ? <p className="settings-note error">{proxySaveError}</p> : null}
                {proxyTestNote ? (
                  <p className={`settings-note ${proxyTestNote.kind === 'err' ? 'error' : ''}`}>
                    {proxyTestNote.text}
                  </p>
                ) : null}

                <div className="settings-peer-gateway-block">
                  <div className="settings-peer-gateway-title">
                    <strong>外部智能体 接口对接</strong>
                    <span>
                      供其他智能体或脚本调用；修改后点击「保存并重启监听」生效。要给<strong>别的机器</strong>连，监听主机须为{' '}
                      <code>0.0.0.0</code>（默认），并在下方「对外展示基址」填本机局域网 IP（如{' '}
                      <code>http://192.168.x.x:1052</code>）；对方用该基址访问。仅本机可填{' '}
                      <code>127.0.0.1</code>。
                    </span>
                  </div>
                  {peerGatewayLoadError ? (
                    <div className="skills-feedback error agent-feedback inline">
                      <span>{peerGatewayLoadError}</span>
                    </div>
                  ) : null}
                  {peerGatewaySaveError ? (
                    <div className="skills-feedback error agent-feedback inline">
                      <span>{peerGatewaySaveError}</span>
                    </div>
                  ) : null}
                  {peerGatewaySaveNotice ? (
                    <div className="skills-feedback success agent-feedback inline">
                      <span>{peerGatewaySaveNotice}</span>
                    </div>
                  ) : null}
                  {peerGatewayInfo?.envOverrideActive ? (
                    <div className="agent-workspace-hint">
                      <span>
                        当前由环境变量 <code>NINECLAW_PEER_BIND</code> 指定监听地址，下方端口与主机在应用内<strong>无效</strong>；取消该环境变量后可在此配置。
                      </span>
                    </div>
                  ) : null}
                  {peerGatewayDraft && peerGatewayInfo ? (
                    <>
                      <SettingSwitch
                        checked={peerGatewayDraft.enabled}
                        description="关闭后停止监听，不接收对等入站请求"
                        label="启用对等入站"
                        onChange={() => {
                          setPeerGatewayDraft((previous) =>
                            previous ? { ...previous, enabled: !previous.enabled } : previous,
                          )
                          setPeerGatewaySaveNotice('')
                        }}
                      />
                      <div className="settings-row">
                        <div>
                          <strong>监听主机</strong>
                          <p>
                            <code>0.0.0.0</code>：所有网卡（默认，局域网/other 虾可连）；<code>127.0.0.1</code>
                            ：仅本机进程可连。
                          </p>
                        </div>
                        <label className="input-field settings-peer-field">
                          <input
                            value={peerGatewayDraft.host}
                            disabled={peerGatewayInfo.envOverrideActive}
                            onChange={(event) => {
                              setPeerGatewayDraft((previous) =>
                                previous ? { ...previous, host: event.target.value } : previous,
                              )
                              setPeerGatewaySaveNotice('')
                            }}
                          />
                        </label>
                      </div>
                      <div className="settings-row">
                        <div>
                          <strong>监听端口</strong>
                          <p>默认 1052；勿与系统其他服务冲突。</p>
                        </div>
                        <label className="input-field settings-peer-field">
                          <NumericDraftField
                            aria-label="监听端口"
                            value={peerGatewayDraft.port}
                            min={1}
                            max={65535}
                            fallbackOnBlur={1052}
                            disabled={peerGatewayInfo.envOverrideActive}
                            onCommit={(next) => {
                              setPeerGatewayDraft((previous) =>
                                previous ? { ...previous, port: next } : previous,
                              )
                              setPeerGatewaySaveNotice('')
                            }}
                          />
                        </label>
                      </div>
                      <div className="settings-row">
                        <div>
                          <strong>对外展示基址（可选）</strong>
                          <p>
                            给对接方复制的 API 根 URL。监听为 <code>0.0.0.0</code> 时<strong>务必</strong>填本机局域网
                            IP（如 <code>http://192.168.1.5:1052</code>），留空会误显示为 127.0.0.1。
                          </p>
                        </div>
                        <label className="input-field settings-peer-field">
                          <input
                            placeholder="例如 http://192.168.1.5:1052"
                            value={peerGatewayDraft.publicBase}
                            disabled={peerGatewayInfo.envOverrideActive}
                            onChange={(event) => {
                              setPeerGatewayDraft((previous) =>
                                previous ? { ...previous, publicBase: event.target.value } : previous,
                              )
                              setPeerGatewaySaveNotice('')
                            }}
                          />
                        </label>
                      </div>
                      <div className="settings-row stacked">
                        <div>
                          <strong>当前解析结果</strong>
                          <p className="settings-peer-live-urls">
                            <span>
                              监听：{peerGatewayInfo.listenAddress ?? '—'} · POST{' '}
                              {peerGatewayInfo.inboundUrl ?? '—'}
                            </span>
                          </p>
                        </div>
                      </div>
                      <div className="settings-peer-actions">
                        <button
                          type="button"
                          className="outline-button primary"
                          disabled={peerGatewaySaving || peerGatewayInfo.envOverrideActive}
                          onClick={() => {
                            if (!peerGatewayDraft || peerGatewayInfo.envOverrideActive) {
                              return
                            }
                            setPeerGatewaySaving(true)
                            setPeerGatewaySaveError('')
                            setPeerGatewaySaveNotice('')
                            void savePeerGatewaySettings(peerGatewayDraft)
                              .then((info) => {
                                setPeerGatewayInfo(info)
                                setPeerGatewaySaveNotice('已保存并重启监听。')
                              })
                              .catch((error: unknown) => {
                                setPeerGatewaySaveError(error instanceof Error ? error.message : String(error))
                              })
                              .finally(() => setPeerGatewaySaving(false))
                          }}
                        >
                          {peerGatewaySaving ? '保存中…' : '保存并重启监听'}
                        </button>
                      </div>
                    </>
                  ) : !peerGatewayLoadError ? (
                    <div className="agent-workspace-hint">
                      <span>正在加载对等网关配置…</span>
                    </div>
                  ) : null}
                </div>
              </div>
            ) : null}

            {tab === 'appearance' ? (
              <div className="settings-section-stack">
                <div className="settings-row">
                  <div>
                    <strong>主题</strong>
                    <p>统一切换主工作区、设置页和弹窗主题，后续新增主题也会走同一套 token 配置。</p>
                  </div>
                  <label className="select-field">
                    <select
                      value={appearanceSettings.themeMode}
                      onChange={(event) =>
                        setAppearanceSettings((previous) => ({
                          ...previous,
                          themeMode: event.target.value as AppearanceSettings['themeMode'],
                        }))
                      }
                    >
                      {THEME_OPTIONS.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <SettingSwitch
                  checked={appearanceSettings.compactSidebar}
                  description="压缩左侧导航宽度，适合更小的桌面窗口。"
                  label="紧凑侧栏"
                  onChange={() =>
                    setAppearanceSettings((previous) => ({
                      ...previous,
                      compactSidebar: !previous.compactSidebar,
                    }))
                  }
                />
                <SettingSwitch
                  checked={appearanceSettings.showThinkingProcess}
                  description="在助手回复中展示模型的 thinking 流（若供应商返回）。关闭后仍会在后台记录，仅不显示。"
                  label="展示思考过程"
                  onChange={() =>
                    setAppearanceSettings((previous) => ({
                      ...previous,
                      showThinkingProcess: !previous.showThinkingProcess,
                    }))
                  }
                />
                <SettingSwitch
                  checked={appearanceSettings.showExecutionRail}
                  description="在对话中展示工具调用卡片，以及分段回复里与工具相关的块。"
                  label="展示工具调用"
                  onChange={() =>
                    setAppearanceSettings((previous) => ({
                      ...previous,
                      showExecutionRail: !previous.showExecutionRail,
                    }))
                  }
                />
                <SettingSwitch
                  checked={appearanceSettings.preferReducedMotion}
                  description="减少过渡动画，提升低性能机器上的响应感。"
                  label="减少动画"
                  onChange={() =>
                    setAppearanceSettings((previous) => ({
                      ...previous,
                      preferReducedMotion: !previous.preferReducedMotion,
                    }))
                  }
                />
              </div>
            ) : null}

            {tab === 'parameters' ? (
              <div className="settings-section-stack">
                <div className="settings-peer-gateway-block">
                  <div className="settings-peer-gateway-title">
                    <strong>Agent 循环</strong>
                    <span>
                      控制 Agent 单次对话中工具调用的循环行为。接近上限时系统会弹窗提醒。
                    </span>
                  </div>
                  <div className="settings-row stacked">
                    <div>
                      <strong>最大迭代次数</strong>
                      <p>单次对话中 Agent 执行工具调用的最大轮数。</p>
                    </div>
                    <label className="input-field settings-peer-field" style={{ maxWidth: 200 }}>
                      <NumericDraftField
                        aria-label="最大迭代次数"
                        value={generalSettings.runtimeParameters.maxAgentToolRoundsPerDialogue}
                        min={1}
                        max={500}
                        fallbackOnBlur={80}
                        onCommit={(next) =>
                          setGeneralSettings((previous) => ({
                            ...previous,
                            runtimeParameters: { ...previous.runtimeParameters, maxAgentToolRoundsPerDialogue: next },
                          }))
                        }
                      />
                    </label>
                  </div>
                  <div className="settings-row stacked">
                    <div>
                      <strong>流式传输中断重试</strong>
                      <p>Agent 循环中流式响应断开或长时间无输出时的最大等待/重连重试次数。</p>
                    </div>
                    <label className="input-field settings-peer-field" style={{ maxWidth: 200 }}>
                      <NumericDraftField
                        aria-label="流式中断重试"
                        value={generalSettings.runtimeParameters.streamDisconnectMaxRetries}
                        min={0}
                        max={20}
                        fallbackOnBlur={3}
                        onCommit={(next) =>
                          setGeneralSettings((previous) => ({
                            ...previous,
                            runtimeParameters: { ...previous.runtimeParameters, streamDisconnectMaxRetries: next },
                          }))
                        }
                      />
                    </label>
                  </div>
                </div>

                <div className="settings-peer-gateway-block">
                  <div className="settings-peer-gateway-title">
                    <strong>LLM 请求</strong>
                    <span>控制大模型 API 请求的重试策略（网络错误、限流等）。</span>
                  </div>
                  <div className="settings-row stacked">
                    <div>
                      <strong>最大重试次数</strong>
                      <p>遇到网络错误或 429 限流时的指数退避重试上限（含首次请求）。</p>
                    </div>
                    <label className="input-field settings-peer-field" style={{ maxWidth: 200 }}>
                      <NumericDraftField
                        aria-label="LLM 外层最大重试次数"
                        value={generalSettings.runtimeParameters.llmOuterMaxAttempts}
                        min={1}
                        max={24}
                        fallbackOnBlur={8}
                        onCommit={(next) =>
                          setGeneralSettings((previous) => ({
                            ...previous,
                            runtimeParameters: { ...previous.runtimeParameters, llmOuterMaxAttempts: next },
                          }))
                        }
                      />
                    </label>
                  </div>
                </div>
              </div>
            ) : null}

            {tab === 'providers' ? (
              <div className="provider-settings-shell">
                <div className="provider-mode-tabs" role="tablist" aria-label="Provider 设置类型">
                  <ProviderSettingsModeButton
                    active={providerSettingsMode === 'llm'}
                    label="普通 LLM"
                    onClick={() => setProviderSettingsMode('llm')}
                  />
                  <ProviderSettingsModeButton
                    active={providerSettingsMode === 'image'}
                    label="图片大模型"
                    onClick={() => setProviderSettingsMode('image')}
                  />
                  <ProviderSettingsModeButton
                    active={providerSettingsMode === 'vision'}
                    label="识图模型"
                    onClick={() => setProviderSettingsMode('vision')}
                  />
                </div>

                {providerSettingsMode === 'llm' ? (
                  <div className="bot-settings-layout">
                    <div className="bot-channel-list">
                      <div className="image-provider-list-head provider-list-head">
                        <div>
                          <strong>{providerAddMode ? '添加模型提供方' : '模型提供方'}</strong>
                          <span>共 {providerAddMode ? filteredAvailableProviders.length : addedProviders.length} 个</span>
                        </div>
                        <button
                          type="button"
                          className="image-provider-add-icon"
                          onClick={() => {
                            setProviderAddMode((previous) => !previous)
                            setCustomFormOpen(false)
                            setProviderListSearch('')
                          }}
                          aria-label={providerAddMode ? '返回模型列表' : '添加 Provider'}
                        >
                          <AppIcon name={providerAddMode ? 'close' : 'plus'} size={18} />
                        </button>
                      </div>
                      <div className="provider-list-search">
                        <AppIcon name="search" size={16} />
                        <input
                          type="search"
                          className="provider-list-search-input"
                          value={providerListSearch}
                          onChange={(event) => setProviderListSearch(event.target.value)}
                          placeholder="搜索提供方或模型…"
                          aria-label="搜索提供方或模型"
                        />
                      </div>
                      <div className="provider-list-items">
                      {providerAddMode ? (
                        <>
                          {customFormOpen ? (
                            <div className="provider-custom-form">
                              <label className="input-field">
                                <span>供应商名称</span>
                                <input
                                  value={customName}
                                  onChange={(event) => setCustomName(event.target.value)}
                                  placeholder="例如：公司内网网关"
                                />
                              </label>
                              <label className="input-field">
                                <span>说明（可选）</span>
                                <input
                                  value={customDescription}
                                  onChange={(event) => setCustomDescription(event.target.value)}
                                  placeholder={customApiFormat === 'anthropic' ? 'Anthropic 兼容接口' : 'OpenAI 兼容接口'}
                                />
                              </label>
                              <div className="input-field">
                                <span>接口格式</span>
                                <label className="select-field">
                                  <select
                                    value={customApiFormat}
                                    onChange={(event) => setCustomApiFormat(event.target.value as ProviderApiFormat)}
                                  >
                                    <option value="openai">OpenAI</option>
                                    <option value="anthropic">Anthropic</option>
                                  </select>
                                </label>
                              </div>
                              <div className="provider-actions">
                                <button type="button" className="outline-button" onClick={() => setCustomFormOpen(false)}>
                                  返回
                                </button>
                                <button type="button" className="outline-button primary" onClick={handleSubmitCustomProvider}>
                                  创建
                                </button>
                              </div>
                            </div>
                          ) : (
                            <>
                              <button
                                type="button"
                                className="provider-add-button provider-custom-shortcut"
                                onClick={() => setCustomFormOpen(true)}
                              >
                                <AppIcon name="plus" size={18} />
                                <span>新建自定义 Provider</span>
                              </button>
                              {filteredAvailableProviders.map((provider) => {
                                return (
                                  <button
                                    key={provider.id}
                                    type="button"
                                    className={`bot-channel-card ${selectedProviderId === provider.id ? 'active' : ''}`}
                                    onClick={() => handleProviderSelect(provider.id)}
                                  >
                                    <span className="bot-channel-main">
                                      <span className="bot-channel-icon" aria-hidden="true">
                                        {provider.name.slice(0, 1).toUpperCase()}
                                      </span>
                                      <span className="bot-channel-copy">
                                        <strong>{provider.name}</strong>
                                        <span className="bot-channel-model">{provider.suggestedModel}</span>
                                      </span>
                                    </span>
                                    <span className="image-provider-state">添加</span>
                                  </button>
                                )
                              })}
                              {filteredAvailableProviders.length === 0 && (
                                <p className="settings-note">
                                  {availableProviders.length === 0
                                    ? '预设已全部添加；你仍可使用上方「自定义供应商」。'
                                    : '没有匹配的 Provider，请换个关键词。'}
                                </p>
                              )}
                            </>
                          )}
                        </>
                      ) : (
                        <>
                          {addedProviders.length === 0 ? (
                            <p className="settings-note">暂未添加任何 Provider，请点击上方按钮添加。</p>
                          ) : filteredAddedProviders.length === 0 ? (
                            <p className="settings-note">没有匹配的 Provider，请换个关键词。</p>
                          ) : (
                            filteredAddedProviders.map((provider) => {
                              const config = providerConfigs[provider.id]
                              const modelLabel = config.model.trim() || provider.suggestedModel || '未设置模型'
                              return (
                                <div
                                  key={provider.id}
                                  className={`bot-channel-card ${selectedProviderId === provider.id ? 'active' : ''}`}
                                  role="button"
                                  tabIndex={0}
                                  onClick={() => handleProviderSelect(provider.id)}
                                  onKeyDown={(event) => {
                                    if (event.key === 'Enter' || event.key === ' ') {
                                      event.preventDefault()
                                      handleProviderSelect(provider.id)
                                    }
                                  }}
                                >
                                  <div className="bot-channel-main">
                                    <span className="bot-channel-icon" aria-hidden="true">
                                      {providerDisplayName(provider, config).slice(0, 1).toUpperCase()}
                                    </span>
                                    <span className="bot-channel-copy">
                                      <strong>{providerDisplayName(provider, config)}</strong>
                                      <span className="bot-channel-model">{modelLabel}</span>
                                    </span>
                                  </div>
                                  <div className="provider-card-end">
                                    <button
                                      type="button"
                                      className="provider-card-action-button"
                                      onClick={(event) => {
                                        event.stopPropagation()
                                        onDuplicateProvider(provider.id)
                                      }}
                                      aria-label={`复制 ${providerDisplayName(provider, config)}`}
                                      title="复制模型配置"
                                    >
                                      <AppIcon name="copy" size={14} />
                                    </button>
                                    <button
                                      type="button"
                                      className="provider-card-action-button provider-remove-button"
                                      onClick={(event) => {
                                        event.stopPropagation()
                                        setProviderDeleteConfirmId(provider.id)
                                      }}
                                      aria-label={`移除 ${providerDisplayName(provider, config)}`}
                                      title="移除 Provider"
                                    >
                                      <AppIcon name="close" size={14} />
                                    </button>
                                  </div>
                                </div>
                              )
                            })
                          )}
                        </>
                      )}
                      </div>
                    </div>

                    <div className="bot-detail-panel">
                      {addedProviders.length === 0 ? (
                        <div className="provider-empty-state">
                          <AppIcon name="provider" size={48} />
                          <p>请先添加一个 Provider</p>
                        </div>
                      ) : (
                        <>
                          <div className="bot-detail-head provider-detail-head">
                            <div className="bot-detail-title">
                              <AppIcon name="provider" size={18} />
                              <strong className="bot-detail-heading">
                                {providerDisplayName(selectedProviderDefinition, selectedProviderConfig)} 配置
                              </strong>
                              <span className="bot-status-tag">{selectedProviderConfig.status}</span>
                            </div>
                            <div className="provider-runtime-badge">{activeProviderBadge}</div>
                          </div>

                          <p className="settings-note provider-note">{selectedProviderDefinition.description}</p>
                          <p className="settings-note provider-note">
                            当前接口格式：{selectedProviderFormatDefaults.label}
                          </p>

                          <label className="input-field">
                            <span>接口格式</span>
                            <label className="select-field">
                              <select
                                value={selectedProviderConfig.apiFormat}
                                onChange={(event) =>
                                  onProviderConfigChange(selectedProviderId, {
                                    apiFormat: event.target.value as ProviderApiFormat,
                                  })
                                }
                              >
                                <option value="openai">OpenAI Chat Completions</option>
                                <option value="anthropic">Anthropic Messages</option>
                              </select>
                            </label>
                          </label>

                          <label className="input-field">
                            <span>显示名称</span>
                            <input
                              value={selectedProviderConfig.displayName}
                              onChange={(event) =>
                                onProviderConfigChange(selectedProviderId, { displayName: event.target.value })
                              }
                              placeholder={selectedProviderDefinition.name}
                            />
                          </label>

                          <label className="input-field">
                            <span>Base URL</span>
                            <input
                              value={selectedProviderConfig.baseUrl}
                              onChange={(event) =>
                                onProviderConfigChange(selectedProviderId, { baseUrl: event.target.value })
                              }
                              placeholder={selectedProviderFormatDefaults.baseUrl}
                            />
                          </label>

                          <label className="input-field">
                            <span>API Key</span>
                            <div className="input-action-field">
                              <input
                                type={showApiKey ? 'text' : 'password'}
                                value={selectedProviderConfig.apiKey}
                                onChange={(event) =>
                                  onProviderConfigChange(selectedProviderId, { apiKey: event.target.value })
                                }
                                placeholder="请输入 API Key"
                              />
                              <button
                                type="button"
                                className="input-action-button"
                                onClick={() => setShowApiKey((previous) => !previous)}
                                aria-label={showApiKey ? '隐藏 API Key' : '显示 API Key'}
                                aria-pressed={showApiKey}
                                title={showApiKey ? '隐藏 API Key' : '显示 API Key'}
                              >
                                <AppIcon name={showApiKey ? 'eye-off' : 'eye'} size={16} />
                              </button>
                            </div>
                          </label>

                          <label className="input-field">
                            <span>默认模型</span>
                            <input
                              value={selectedProviderConfig.model}
                              onChange={(event) => onProviderConfigChange(selectedProviderId, { model: event.target.value })}
                              placeholder={selectedProviderFormatDefaults.model}
                            />
                          </label>

                          <label className="input-field">
                            <span>最大上下文窗口 (tokens)</span>
                            <input
                              type="text"
                              inputMode="numeric"
                              autoComplete="off"
                              value={
                                selectedProviderConfig.maxContextTokens === undefined ||
                                selectedProviderConfig.maxContextTokens === null
                                  ? ''
                                  : String(selectedProviderConfig.maxContextTokens)
                              }
                              onChange={(event) => {
                                const raw = event.target.value.trim()
                                if (raw === '') {
                                  onProviderConfigChange(selectedProviderId, { maxContextTokens: undefined })
                                  return
                                }
                                const parsed = parseInt(raw, 10)
                                onProviderConfigChange(selectedProviderId, {
                                  maxContextTokens: Number.isFinite(parsed) && parsed > 0 ? parsed : undefined,
                                })
                              }}
                              placeholder="如 128000，留空表示自动检测"
                              aria-label="最大上下文窗口 (tokens)"
                            />
                          </label>

                          <label className="input-field">
                            <span>备注</span>
                            <input
                              value={selectedProviderConfig.note}
                              onChange={(event) => onProviderConfigChange(selectedProviderId, { note: event.target.value })}
                              placeholder="例如：用于后续替换默认模型路由"
                            />
                          </label>

                          <div className="provider-actions">
                            <button
                              type="button"
                              className="outline-button"
                              onClick={() =>
                                onProviderConfigChange(selectedProviderId, {
                                  status: getProviderStatus(selectedProviderConfig, false),
                                })
                              }
                            >
                              <AppIcon name="refresh" size={18} />
                              <span>校验配置</span>
                            </button>
                            <button
                              type="button"
                              className="outline-button"
                              disabled={providerTestLoading}
                              onClick={async () => {
                                setProviderTestLoading(true)
                                setProviderTestNote(null)
                                try {
                                  const message = await testLlmProviderConnection({
                                    apiFormat: selectedProviderConfig.apiFormat,
                                    baseUrl: selectedProviderConfig.baseUrl,
                                    apiKey: selectedProviderConfig.apiKey,
                                    model: selectedProviderConfig.model,
                                  })
                                  onProviderConfigChange(selectedProviderId, { status: '测试通过' })
                                  setProviderTestNote({ kind: 'ok', text: message })
                                } catch (error) {
                                  onProviderConfigChange(selectedProviderId, { status: '已配置' })
                                  setProviderTestNote({ kind: 'err', text: String(error) })
                                } finally {
                                  setProviderTestLoading(false)
                                }
                              }}
                            >
                              <AppIcon name="broadcast" size={18} />
                              <span>{providerTestLoading ? '测试中…' : '测试连通性'}</span>
                            </button>
                          </div>

                          {providerTestNote ? (
                            <p className={`settings-note ${providerTestNote.kind === 'err' ? 'error' : ''}`}>
                              {providerTestNote.text}
                            </p>
                          ) : null}

                          <p className="settings-note">
                            接口格式会直接影响请求协议；切换后可以立即点「测试连通性」验证当前 Base URL、API Key 和模型是否匹配。
                          </p>
                        </>
                      )}
                    </div>
                  </div>
                ) : providerSettingsMode === 'image' ? (
                  <ImageGenerationSettingsSection
                    imageGenerationSystem={draftImageGenerationSystem}
                    imageProviderConfigs={draftImageProviderConfigs}
                    imageProviderDefinitions={imageProviderDefinitions}
                    onAddImageProvider={(name, adapterType) => {
                      const providerId = `custom_image_${crypto.randomUUID().replace(/-/g, '')}`
                      const defaults =
                        adapterType === 'openai_images'
                          ? { baseUrl: 'https://api.openai.com/v1', model: 'gpt-image-1' }
                          : { baseUrl: 'https://api.example.com/v1', model: 'your-image-model' }
                      setDraftImageProviderConfigs((previous) => ({
                        ...previous,
                        [providerId]: {
                          adapterType,
                          baseUrl: defaults.baseUrl,
                          apiKey: '',
                          model: defaults.model,
                          note: '',
                          displayName: name.trim() || name,
                          status: '未配置',
                        },
                      }))
                      setImageSaveNotice('')
                      setImageSaveError('')
                      return providerId
                    }}
                    onImageGenerationSystemChange={(value) => {
                      setDraftImageGenerationSystem((previous) =>
                        typeof value === 'function' ? value(previous) : value,
                      )
                      setImageSaveNotice(''); setImageSaveError('')
                    }}
                    onImageProviderConfigChange={(providerId, updates) => {
                      setDraftImageProviderConfigs((previous) => {
                        const base = previous[providerId] ?? imageProviderConfigs[providerId]
                        if (!base) {
                          return previous
                        }
                        const merged = { ...base, ...updates }
                        return {
                          ...previous,
                          [providerId]: {
                            ...merged,
                            status:
                              updates.status ??
                              (merged.baseUrl.trim() && merged.apiKey.trim() && merged.model.trim() ? '已配置' : '未配置'),
                          },
                        }
                      })
                      setImageSaveNotice('')
                      setImageSaveError('')
                    }}
                    onRemoveImageProvider={(providerId) => {
                      setDraftImageProviderConfigs((previous) => {
                        const next = { ...previous }
                        delete next[providerId]
                        return next
                      })
                      setDraftImageGenerationSystem((previous) => {
                        if (previous.defaultProviderId !== providerId) {
                          return previous
                        }
                        const fallbackProviderId =
                          imageProviderDefinitions.find((definition) => definition.id !== providerId)?.id || 'openai_image'
                        return {
                          ...previous,
                          defaultProviderId: fallbackProviderId,
                        }
                      })
                      setImageSaveNotice('')
                      setImageSaveError('')
                    }}
                  />
                ) : (
                  <ImageVisionSettingsPanel />
                )}
              </div>
            ) : null}

            {tab === 'mcp' ? (
              <div className="settings-tab-body-scroll">
                <McpSettingsPanel />
              </div>
            ) : null}

            {tab === 'memory' ? (
              <div className="settings-tab-body-scroll user-memory-tab-body">
                <UserMemorySettingsPanel agents={memoryAgents} defaultAgentId={memoryDefaultAgentId} />
                <VectorMemoryPanel />
              </div>
            ) : null}

            {tab === 'logs' ? (
              <div className="settings-tab-body-scroll settings-logs-tab">
                <ApplicationLogsPanel />
                <details
                  className="settings-logs-llm-fold"
                  onToggle={(event) => {
                    const open = (event.currentTarget as HTMLDetailsElement).open
                    if (open && !logPreviewBusy && !logPreviewTail) {
                      void refreshLogPreview()
                    }
                  }}
                >
                  <summary>LLM 调用日志导出（可选）</summary>
                  <div className="settings-logs-llm-body">
                    <div className="settings-row stacked">
                      <div>
                        <p>
                          每条已完成的调用链会额外以 jsonl 追加到该目录下的 <code>llm-trace-日期.jsonl</code>（与团队空间内{' '}
                          <code>.debug</code> 并行，不替代原文件）。与应用运行日志相互独立。
                        </p>
                      </div>
                      <div className="settings-row-actions" style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
                        <button
                          type="button"
                          className="outline-button primary"
                          onClick={async () => {
                            const dir = await open({ directory: true, multiple: false })
                            if (typeof dir === 'string' && dir) {
                              setGeneralSettings((previous) => ({ ...previous, llmCallLogDir: dir }))
                            }
                          }}
                        >
                          选择目录
                        </button>
                        <button
                          type="button"
                          className="outline-button"
                          onClick={() => setGeneralSettings((previous) => ({ ...previous, llmCallLogDir: '' }))}
                        >
                          清除
                        </button>
                        <button
                          type="button"
                          className="outline-button"
                          disabled={logPreviewBusy}
                          onClick={() => void refreshLogPreview()}
                        >
                          {logPreviewBusy ? '刷新中…' : '刷新预览'}
                        </button>
                      </div>
                      <label className="input-field settings-peer-field">
                        <input
                          readOnly
                          value={generalSettings.llmCallLogDir}
                          placeholder="未设置"
                          aria-label="当前 LLM 日志导出目录"
                        />
                      </label>
                    </div>
                    <div className="settings-row stacked">
                      <strong>LLM 导出新文件预览</strong>
                      <LlmLogPreview
                        busy={logPreviewBusy}
                        file={logPreviewFile}
                        tail={logPreviewTail}
                      />
                    </div>
                  </div>
                </details>
              </div>
            ) : null}

            {tab === 'shortcuts' ? (
              <div className="shortcut-list">
                <div className="shortcut-config-card">
                  <strong>发送消息方式</strong>
                  <div className="shortcut-choice-row">
                    <button
                      type="button"
                      className={`shortcut-choice ${generalSettings.submitShortcut === 'enter' ? 'active' : ''}`}
                      onClick={() =>
                        setGeneralSettings((previous) => ({
                          ...previous,
                          submitShortcut: 'enter',
                        }))
                      }
                    >
                      <span>Enter 发送</span>
                      <code>Shift + Enter 换行</code>
                    </button>
                    <button
                      type="button"
                      className={`shortcut-choice ${generalSettings.submitShortcut === 'mod_enter' ? 'active' : ''}`}
                      onClick={() =>
                        setGeneralSettings((previous) => ({
                          ...previous,
                          submitShortcut: 'mod_enter',
                        }))
                      }
                    >
                      <span>Ctrl / Cmd + Enter 发送</span>
                      <code>Enter 换行</code>
                    </button>
                  </div>
                  <p className="settings-note">
                    macOS 桌面版输入栏提供「键盘」图标：打开系统原生文本框，便于 Fn / 豆包等听写；与网页麦克风听写互为补充。NineClaw 使用与 Safari
                    相同的 WebKit
                    网页引擎；「按住 Fn 系统听写」在网页内可能与 Chrome
                    不一致。聊天输入框旁提供「麦克风」网页语音转文字（需麦克风权限）；也可使用菜单「编辑 → 听写」或系统听写快捷键。
                  </p>
                </div>
                <div className="shortcut-row">
                  <span>发送消息</span>
                  <code>{getSubmitShortcutLabel(generalSettings.submitShortcut)}</code>
                </div>
                <div className="shortcut-row">
                  <span>打开设置</span>
                  <code>Cmd + ,</code>
                </div>
                <div className="shortcut-row">
                  <span>探索技能 / 资源库</span>
                  <code>设置侧栏进入</code>
                </div>
                <div className="shortcut-row">
                  <span>停止当前生成</span>
                  <code>Esc</code>
                </div>
              </div>
            ) : null}

            {tab === 'usage' ? (
              <div className="settings-tab-body-scroll">
                <UsageStatsPanel />
              </div>
            ) : null}

            {tab === 'skills' ? (
              <div className="settings-tab-body-scroll">
                <Suspense fallback={<p className="settings-note">正在加载技能…</p>}>
                  <SkillsViewLazy {...skillsLibrary} />
                </Suspense>
              </div>
            ) : null}

            {tab === 'resources' ? (
              <div className="settings-tab-body-scroll">
                <Suspense fallback={<p className="settings-note">正在加载资源库…</p>}>
                  <ResourcesViewLazy {...resourcesLibrary} />
                </Suspense>
              </div>
            ) : null}

            <div className="settings-footer">
              {tab === 'providers' && providerSettingsMode === 'image' ? (
                <div className="settings-footer-copy">
                  {imageSaveError ? <p className="settings-note error">{imageSaveError}</p> : null}
                  {imageSaveNotice ? <p className="settings-note">{imageSaveNotice}</p> : null}
                </div>
              ) : null}
              <div className="settings-footer-actions">
                <button type="button" className="outline-button settings-footer-button" onClick={onClose}>
                  关闭
                </button>
                {tab === 'providers' && providerSettingsMode === 'image' ? (
                  <button
                    type="button"
                    className="primary-dark-button settings-footer-button"
                    disabled={imageSaveBusy}
                    onClick={async () => {
                      setImageSaveBusy(true)
                      setImageSaveError(''); setImageSaveNotice('')
                      try {
                        await onSaveImageGenerationSettings(draftImageProviderConfigs, draftImageGenerationSystem)
                        setImageSaveNotice('图片大模型配置已保存，重启后会自动恢复。')
                      } catch (error) {
                        setImageSaveError(error instanceof Error ? error.message : String(error))
                      } finally {
                        setImageSaveBusy(false)
                      }
                    }}
                  >
                    {imageSaveBusy ? '保存中…' : '保存图片配置'}
                  </button>
                ) : (
                  <button type="button" className="primary-dark-button settings-footer-button" onClick={onClose}>
                    完成
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>

      {providerDeleteConfirmId && pendingDeleteLabel ? (
        <div className="confirm-dialog-overlay" role="presentation" onClick={() => setProviderDeleteConfirmId(null)}>
          <div
            className="confirm-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="provider-delete-confirm-title"
            onClick={(event) => event.stopPropagation()}
          >
            <h3 id="provider-delete-confirm-title">删除 Provider</h3>
            <p>确定要移除「{pendingDeleteLabel}」吗？移除后需重新添加才能再次使用，请确认后再操作。</p>
            <div className="confirm-dialog-actions">
              <button type="button" className="outline-button" onClick={() => setProviderDeleteConfirmId(null)}>
                取消
              </button>
              <button
                type="button"
                className="outline-button confirm-dialog-delete"
                onClick={() => {
                  handleRemoveProvider(providerDeleteConfirmId)
                  setProviderDeleteConfirmId(null)
                }}
              >
                删除
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </>
  )
}
