import { useEffect, useState } from 'react'
import { testLlmProviderConnection } from '../lib/piClient'
import type {
  AppearanceSettings,
  GeneralSettings,
  ProviderApiFormat,
  ProviderConfig,
  ProviderDefinition,
  ProviderId,
  SettingsTab,
} from '../types'
import { AppIcon, type IconName } from './AppIcon'

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

  return (
    <>
      <div className="modal-backdrop">
        <div className="settings-dialog" role="dialog" aria-modal="true" aria-label="设置">
          <div className="settings-sidebar">
            <header>
              <h2>设置</h2>
            </header>

            <div className="settings-tab-list">
              <SettingsTabButton active={tab === 'general'} icon="settings" label="通用" onClick={() => onSelectTab('general')} />
              <SettingsTabButton active={tab === 'appearance'} icon="sparkles" label="个性化" onClick={() => onSelectTab('appearance')} />
              <SettingsTabButton active={tab === 'providers'} icon="provider" label="大模型 Provider" onClick={() => onSelectTab('providers')} />
              <SettingsTabButton active={tab === 'shortcuts'} icon="keyboard" label="快捷键" onClick={() => onSelectTab('shortcuts')} />
            </div>
          </div>

          <div className="settings-content">
            <div className="settings-content-head">
              <h2>
                {tab === 'general'
                  ? '通用'
                  : tab === 'appearance'
                    ? '个性化'
                    : tab === 'providers'
                      ? '大模型 Provider'
                      : '快捷键'}
              </h2>
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
              </div>
            ) : null}

            {tab === 'appearance' ? (
              <div className="settings-section-stack">
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
                  checked={appearanceSettings.showExecutionRail}
                  description="在聊天页显示执行状态卡片，而不是直接暴露模型内部推理。"
                  label="显示执行轨迹"
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
                        当前接口格式：{selectedProviderConfig.apiFormat === 'anthropic' ? 'Anthropic Messages' : 'OpenAI Chat Completions'}
                      </p>

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
                          placeholder={selectedProviderDefinition.defaultBaseUrl}
                        />
                      </label>

                      <label className="input-field">
                        <span>API Key</span>
                        <input
                          type="password"
                          value={selectedProviderConfig.apiKey}
                          onChange={(event) => onProviderConfigChange(selectedProviderId, { apiKey: event.target.value })}
                          placeholder="请输入 API Key"
                        />
                      </label>

                      <label className="input-field">
                        <span>默认模型</span>
                        <input
                          value={selectedProviderConfig.model}
                          onChange={(event) => onProviderConfigChange(selectedProviderId, { model: event.target.value })}
                          placeholder={selectedProviderDefinition.suggestedModel}
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
                        启用后的 Provider 会直接参与后续对话执行。为避免冲突，界面会保持单一启用项。
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
