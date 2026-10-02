import { useEffect, useState } from 'react'
import { AppIcon } from '../AppIcon'
import { NumericDraftField } from '../NumericDraftField'
import {
  clampToolRouterTimeoutMs,
  DEFAULT_TOOL_ROUTER_SETTINGS,
  loadToolRouterSettings,
  saveToolRouterSettings,
  TOOL_ROUTER_TIMEOUT_MAX_MS,
  TOOL_ROUTER_TIMEOUT_MIN_MS,
  type ToolRouterSettings,
} from '../../lib/toolRouterClient'

type ToolRouterMode = ToolRouterSettings['mode']

const MODE_OPTIONS: Array<{ value: ToolRouterMode; label: string }> = [
  { value: 'legacy', label: '原有模式' },
  { value: 'jev', label: 'Jev 模式' },
]

function mergeDraft(
  previous: ToolRouterSettings,
  updates: Partial<ToolRouterSettings>,
): ToolRouterSettings {
  return { ...previous, ...updates }
}

export function ToolRouterSettingsPanel() {
  const [draft, setDraft] = useState<ToolRouterSettings>(DEFAULT_TOOL_ROUTER_SETTINGS)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const [showApiKey, setShowApiKey] = useState(false)

  useEffect(() => {
    let disposed = false
    setLoading(true)
    void loadToolRouterSettings()
      .then((settings) => {
        if (!disposed) {
          setDraft(settings)
          setError('')
        }
      })
      .catch((loadError) => {
        if (!disposed) {
          setError(`读取工具路由配置失败：${String(loadError)}`)
        }
      })
      .finally(() => {
        if (!disposed) {
          setLoading(false)
        }
      })
    return () => {
      disposed = true
    }
  }, [])

  const update = (updates: Partial<ToolRouterSettings>) => {
    setDraft((previous) => mergeDraft(previous, updates))
    setNotice('')
    setError('')
  }

  const handleSave = async () => {
    setSaving(true)
    setNotice('')
    setError('')
    const normalized: ToolRouterSettings = {
      ...draft,
      timeoutMs: clampToolRouterTimeoutMs(draft.timeoutMs),
    }
    try {
      const saved = await saveToolRouterSettings(normalized)
      setDraft(saved)
      setNotice('工具路由配置已保存，新会话起生效。')
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError))
    } finally {
      setSaving(false)
    }
  }

  const jevMissingKey = draft.mode === 'jev' && draft.apiKey.trim() === ''

  return (
    <div className="settings-section-stack tool-router-settings">
      <section className="image-settings-summary" aria-label="工具路由">
        <div className="image-settings-summary-copy">
          <span className="settings-sidebar-kicker">工具路由（Jev 智能选工具）</span>
          <p>
            每轮对话开始前，由 Jev 模型判断当前任务需要哪些工具，只把选中的工具暴露给主模型；
            路由失败（如超时、密钥无效）时自动回退原有工具白名单，不影响对话。
          </p>
        </div>
        <div className="image-settings-summary-card">
          <span>当前状态</span>
          <strong>{loading ? '读取中…' : draft.mode === 'jev' ? 'Jev 模式' : '原有模式'}</strong>
        </div>
      </section>

      <div className="provider-mode-tabs" role="tablist" aria-label="工具路由模式">
        {MODE_OPTIONS.map((option) => (
          <button
            key={option.value}
            type="button"
            className={`provider-mode-tab ${draft.mode === option.value ? 'active' : ''}`}
            role="tab"
            aria-selected={draft.mode === option.value}
            onClick={() => update({ mode: option.value })}
          >
            <span>{option.label}</span>
          </button>
        ))}
      </div>

      <div className="image-settings-form-grid" style={{ maxWidth: 720 }}>
        <label className="input-field image-settings-wide">
          <span>API Key</span>
          <div className="input-action-field">
            <input
              type={showApiKey ? 'text' : 'password'}
              value={draft.apiKey}
              onChange={(event) => update({ apiKey: event.target.value })}
              placeholder="请输入 Jev 服务 API Key"
              aria-label="Jev API Key"
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
          <small>Jev 模式必填；缺失时后端会静默沿用原有模式</small>
        </label>
        <label className="input-field">
          <span>模型名称</span>
          <input
            value={draft.model}
            onChange={(event) => update({ model: event.target.value })}
            placeholder="jev-1.13.0"
            aria-label="Jev 模型名称"
          />
          <small>固定版本号，避免模型漂移</small>
        </label>
        <label className="input-field">
          <span>路由超时（毫秒）</span>
          <NumericDraftField
            aria-label="路由超时（毫秒）"
            value={draft.timeoutMs}
            min={TOOL_ROUTER_TIMEOUT_MIN_MS}
            max={TOOL_ROUTER_TIMEOUT_MAX_MS}
            fallbackOnBlur={DEFAULT_TOOL_ROUTER_SETTINGS.timeoutMs}
            onCommit={(next) => update({ timeoutMs: next })}
          />
          <small>超出 500–10000 时按边界收敛；超时后回退原有白名单</small>
        </label>
      </div>

      <div className="provider-actions">
        <button
          type="button"
          className="outline-button primary"
          disabled={saving || loading}
          onClick={() => void handleSave()}
        >
          <AppIcon name="check" size={16} />
          <span>{saving ? '保存中…' : '保存工具路由配置'}</span>
        </button>
        <span className={`image-provider-state ${draft.mode === 'jev' ? 'configured' : ''}`}>
          {draft.mode === 'jev' ? 'Jev 模式' : '原有模式'}
        </span>
      </div>

      {jevMissingKey ? (
        <p className="settings-note error">
          尚未填写 API Key：可先保存，但在补齐密钥前，后端会继续按原有模式路由工具。
        </p>
      ) : null}
      {notice ? <p className="settings-note">{notice}</p> : null}
      {error ? <p className="settings-note error">{error}</p> : null}
      <p className="settings-note">保存后无需重启：下一轮对话即按新模式路由工具。</p>
    </div>
  )
}
