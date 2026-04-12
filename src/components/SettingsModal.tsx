import { useCallback, useEffect, useState } from 'react'
import {
  getPeerGatewayInfo,
  loadPeerGatewaySettings,
  savePeerGatewaySettings,
  testLlmProviderConnection,
} from '../lib/piClient'
import { THEME_OPTIONS } from '../theme/themePresets'
import type {
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
import { AppIcon, type IconName } from './AppIcon'
import { UsageStatsPanel } from './UsageStatsPanel'

type SettingsModalProps = {
  activeProviderBadge: string
  allProviderDefinitions: ProviderDefinition[]
  appearanceSettings: AppearanceSettings
  generalSettings: GeneralSettings
  onAddCustomProvider: (name: string, description: string, apiFormat: ProviderApiFormat) => void
  onProviderConfigChange: (providerId: ProviderId, updates: Partial<ProviderConfig>) => void
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
  onAddCustomProvider,
  onProviderConfigChange,
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
}: SettingsModalProps) {
  const [providerAddMode, setProviderAddMode] = useState(false)
  const [customFormOpen, setCustomFormOpen] = useState(false)
  const [customName, setCustomName] = useState('')
  const [customDescription, setCustomDescription] = useState('')
  const [customApiFormat, setCustomApiFormat] = useState<ProviderApiFormat>('openai')
  const [providerTestLoading, setProviderTestLoading] = useState(false)
  const [providerTestNote, setProviderTestNote] = useState<{ kind: 'ok' | 'err'; text: string } | null>(null)
  const [providerDeleteConfirmId, setProviderDeleteConfirmId] = useState<ProviderId | null>(null)
  const [showApiKey, setShowApiKey] = useState(false)

  const [peerGatewayDraft, setPeerGatewayDraft] = useState<PeerGatewaySettings | null>(null)
  const [peerGatewayInfo, setPeerGatewayInfo] = useState<PeerGatewayInfo | null>(null)
  const [peerGatewayLoadError, setPeerGatewayLoadError] = useState('')
  const [peerGatewaySaveError, setPeerGatewaySaveError] = useState('')
  const [peerGatewaySaveNotice, setPeerGatewaySaveNotice] = useState('')
  const [peerGatewaySaving, setPeerGatewaySaving] = useState(false)

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

  const addedProviders = allProviderDefinitions.filter((p) => providerConfigs[p.id]?.added)
  const availableProviders = allProviderDefinitions.filter((p) => !providerConfigs[p.id]?.added)

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
        : tab === 'providers'
          ? '模型提供方'
          : tab === 'usage'
            ? '用量统计'
          : '快捷键'

  const currentTabDescription =
    tab === 'general'
      ? '维护应用默认行为、语言偏好和对外接入配置。'
      : tab === 'appearance'
        ? '控制侧栏密度、执行轨迹和页面动态效果。'
      : tab === 'providers'
        ? '统一管理大模型接口、默认模型与连通性校验。'
        : tab === 'usage'
          ? '按模型、智能体与日期查看本地累计用量。'
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
              <SettingsTabButton active={tab === 'providers'} icon="provider" label="大模型 Provider" onClick={() => onSelectTab('providers')} />
              <SettingsTabButton active={tab === 'usage'} icon="zap" label="用量统计" onClick={() => onSelectTab('usage')} />
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

                <SettingSwitch
                  checked={generalSettings.useSystemProxy}
                  description="开启后网络请求将跟随系统代理（保存后生效）"
                  label="使用系统代理"
                  onChange={() =>
                    setGeneralSettings((previous) => ({
                      ...previous,
                      useSystemProxy: !previous.useSystemProxy,
                    }))
                  }
                />

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
                          <input
                            type="number"
                            min={1}
                            max={65535}
                            value={peerGatewayDraft.port}
                            disabled={peerGatewayInfo.envOverrideActive}
                            onChange={(event) => {
                              const next = Number.parseInt(event.target.value, 10)
                              setPeerGatewayDraft((previous) =>
                                previous
                                  ? { ...previous, port: Number.isFinite(next) ? next : previous.port }
                                  : previous,
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

            {tab === 'providers' ? (
              <div className="bot-settings-layout">
                <div className="bot-channel-list">
                  {providerAddMode ? (
                    <>
                      <div className="provider-add-header">
                        <span>选择要添加的 Provider</span>
                        <button
                          type="button"
                          className="link-button"
                          onClick={() => {
                            setProviderAddMode(false)
                            setCustomFormOpen(false)
                          }}
                        >
                          取消
                        </button>
                      </div>
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
                          <button type="button" className="provider-add-button subtle" onClick={() => setCustomFormOpen(true)}>
                            <AppIcon name="plus" size={18} />
                            <span>添加自定义供应商（OpenAI / Anthropic 兼容）</span>
                          </button>
                          {availableProviders.map((provider) => {
                            return (
                              <button
                                key={provider.id}
                                type="button"
                                className={`bot-channel-card ${selectedProviderId === provider.id ? 'active' : ''}`}
                                onClick={() => handleProviderSelect(provider.id)}
                              >
                                <span className="bot-channel-copy">
                                  <strong>{provider.name}</strong>
                                  <span>{provider.description}</span>
                                </span>
                                <AppIcon name="plus" size={16} />
                              </button>
                            )
                          })}
                          {availableProviders.length === 0 && (
                            <p className="settings-note">预设已全部添加；你仍可使用上方「自定义供应商」。</p>
                          )}
                        </>
                      )}
                    </>
                  ) : (
                    <>
                      <button
                        type="button"
                        className="provider-add-button"
                        onClick={() => {
                          setProviderAddMode(true)
                          setCustomFormOpen(false)
                        }}
                      >
                        <AppIcon name="plus" size={18} />
                        <span>添加 Provider</span>
                      </button>
                      {addedProviders.length === 0 ? (
                        <p className="settings-note">暂未添加任何 Provider，请点击上方按钮添加。</p>
                      ) : (
                        addedProviders.map((provider) => {
                          const config = providerConfigs[provider.id]
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
                              <span className="bot-channel-copy">
                                <strong>{providerDisplayName(provider, config)}</strong>
                                <span>{config.status}</span>
                              </span>
                              <button
                                type="button"
                                className="provider-remove-button"
                                onClick={(event) => {
                                  event.stopPropagation()
                                  setProviderDeleteConfirmId(provider.id)
                                }}
                                aria-label={`移除 ${providerDisplayName(provider, config)}`}
                              >
                                <AppIcon name="close" size={14} />
                              </button>
                            </div>
                          )
                        })
                      )}
                    </>
                  )}
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
                          onChange={(event) => onProviderConfigChange(selectedProviderId, { displayName: event.target.value })}
                          placeholder={selectedProviderDefinition.name}
                        />
                      </label>

                      <label className="input-field">
                        <span>Base URL</span>
                        <input
                          value={selectedProviderConfig.baseUrl}
                          onChange={(event) => onProviderConfigChange(selectedProviderId, { baseUrl: event.target.value })}
                          placeholder={selectedProviderFormatDefaults.baseUrl}
                        />
                      </label>

                      <label className="input-field">
                        <span>API Key</span>
                        <div className="input-action-field">
                          <input
                            type={showApiKey ? 'text' : 'password'}
                            value={selectedProviderConfig.apiKey}
                            onChange={(event) => onProviderConfigChange(selectedProviderId, { apiKey: event.target.value })}
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
                  <span>切换技能页</span>
                  <code>Cmd + 2</code>
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

            <div className="settings-footer">
              <button type="button" className="outline-button settings-footer-button" onClick={onClose}>
                关闭
              </button>
              <button type="button" className="primary-dark-button settings-footer-button" onClick={onClose}>
                完成
              </button>
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
