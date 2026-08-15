import { useEffect, useState } from 'react'
import { AppIcon } from '../AppIcon'
import { NumericDraftField } from '../NumericDraftField'
import { loadImageVisionPreferences, saveImageVisionPreferences } from '../../lib/imageVisionClient'
import type { ImageVisionSystemConfig } from '../../types/imageVision'
import {
  DEFAULT_IMAGE_VISION_SYSTEM_CONFIG,
  isEmptyImageVisionConfig,
  isImageVisionConfigured,
  parseStoredImageVisionSystemConfig,
} from '../../types/imageVision'

function mergeDraft(
  previous: ImageVisionSystemConfig,
  updates: Partial<ImageVisionSystemConfig>,
): ImageVisionSystemConfig {
  return { ...previous, ...updates }
}

export function ImageVisionSettingsPanel() {
  const [draft, setDraft] = useState<ImageVisionSystemConfig>(DEFAULT_IMAGE_VISION_SYSTEM_CONFIG)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const [showApiKey, setShowApiKey] = useState(false)

  useEffect(() => {
    let disposed = false
    setLoading(true)
    void loadImageVisionPreferences()
      .then((raw) => {
        if (disposed) {
          return
        }
        const parsed = parseStoredImageVisionSystemConfig(raw)
        if (parsed && !isEmptyImageVisionConfig(parsed)) {
          setDraft(parsed)
        }
        setError('')
      })
      .catch((loadError) => {
        if (!disposed) {
          setError(`读取识图模型配置失败：${String(loadError)}`)
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

  const configured = isImageVisionConfigured(draft)

  const update = (updates: Partial<ImageVisionSystemConfig>) => {
    setDraft((previous) => mergeDraft(previous, updates))
    setNotice('')
    setError('')
  }

  const handleSave = async () => {
    if (!isImageVisionConfigured(draft)) {
      setError('请完整填写 Base URL、API Key 和模型名称后再保存。')
      return
    }
    setSaving(true)
    setNotice('')
    setError('')
    try {
      await saveImageVisionPreferences(JSON.stringify(draft))
      setNotice('识图模型配置已保存，新会话起生效。')
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="settings-section-stack image-generation-settings image-vision-settings">
      <section className="image-settings-summary" aria-label="识图模型网关">
        <div className="image-settings-summary-copy">
          <span className="settings-sidebar-kicker">系统识图模型</span>
          <p>
            供 <code>image_analyze</code> 工具调用：当主对话模型没有视觉能力时，自动借用这里的视觉模型识别图片 / 视频画面，并把描述文本交回主模型。
          </p>
        </div>
        <div className="image-settings-summary-card">
          <span>当前状态</span>
          <strong>{loading ? '读取中…' : configured ? '已配置' : '未配置'}</strong>
        </div>
      </section>

      <div className="image-settings-form-grid" style={{ maxWidth: 720 }}>
        <div className="input-field">
          <span>接口格式</span>
          <label className="select-field">
            <select
              value={draft.apiFormat}
              onChange={(event) =>
                update({
                  apiFormat: event.target.value as ImageVisionSystemConfig['apiFormat'],
                  baseUrl:
                    event.target.value === 'anthropic'
                      ? 'https://api.anthropic.com'
                      : 'https://api.openai.com/v1',
                })
              }
              aria-label="识图模型接口格式"
            >
              <option value="openai">OpenAI 兼容（chat/completions）</option>
              <option value="anthropic">Anthropic（messages）</option>
            </select>
          </label>
          <small>视觉模型走的协议，按提供方选择</small>
        </div>
        <label className="input-field">
          <span>视觉模型名称</span>
          <input
            value={draft.model}
            onChange={(event) => update({ model: event.target.value })}
            placeholder={draft.apiFormat === 'anthropic' ? 'claude-sonnet-4-5' : 'gpt-4o-mini'}
          />
          <small>例如 qwen-vl-max / gpt-4o / claude-sonnet 等支持图片输入的模型</small>
        </label>
        <label className="input-field image-settings-wide">
          <span>Base URL</span>
          <input
            value={draft.baseUrl}
            onChange={(event) => update({ baseUrl: event.target.value })}
            placeholder={draft.apiFormat === 'anthropic' ? 'https://api.anthropic.com' : 'https://api.example.com/v1'}
          />
          <small>接口地址，OpenAI 兼容格式通常以 /v1 结尾</small>
        </label>
        <label className="input-field image-settings-wide">
          <span>API Key</span>
          <div className="input-action-field">
            <input
              type={showApiKey ? 'text' : 'password'}
              value={draft.apiKey}
              onChange={(event) => update({ apiKey: event.target.value })}
              placeholder="请输入视觉模型 API Key"
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
          <span>单次识别输出上限（tokens）</span>
          <NumericDraftField
            aria-label="单次识别输出上限"
            value={draft.maxOutputTokens}
            min={256}
            max={16384}
            fallbackOnBlur={2048}
            onCommit={(next) => update({ maxOutputTokens: next })}
          />
        </label>
        <label className="input-field image-settings-wide">
          <span>默认识别提问（可选）</span>
          <input
            value={draft.defaultPrompt}
            onChange={(event) => update({ defaultPrompt: event.target.value })}
            placeholder="留空则使用内置的详细描述提示词"
          />
          <small>工具未携带具体问题时使用的识别指令</small>
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
          <span>{saving ? '保存中…' : '保存识图配置'}</span>
        </button>
        <span className={`image-provider-state ${configured ? 'configured' : ''}`}>
          {configured ? '已配置' : '未配置'}
        </span>
      </div>

      {notice ? <p className="settings-note">{notice}</p> : null}
      {error ? <p className="settings-note error">{error}</p> : null}
      <p className="settings-note">
        配置后无需重启：新的会话会自动加载；视频会自动抽取关键帧送识别（需要本机安装 ffmpeg）。
      </p>
    </div>
  )
}