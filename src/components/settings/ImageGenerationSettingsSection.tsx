import { useMemo, useState } from 'react'
import { AppIcon } from '../AppIcon'
import type {
  ImageGenerationSystemConfig,
  ImageProviderAdapterType,
  ImageProviderConfig,
  ImageProviderDefinition,
  ImageProviderId,
} from '../../types/imageGeneration'

const IMAGE_PROVIDER_ICON_LABEL: Record<ImageProviderAdapterType, string> = {
  openai_images: '◎',
  openai_compatible: '◇',
  apimart_gpt_image_2: '✦',
}

type ImageGenerationSettingsSectionProps = {
  imageGenerationSystem: ImageGenerationSystemConfig
  imageProviderConfigs: Record<string, ImageProviderConfig>
  imageProviderDefinitions: ImageProviderDefinition[]
  onAddImageProvider: (name: string, adapterType: ImageProviderAdapterType) => ImageProviderId
  onImageGenerationSystemChange: (
    value:
      | ImageGenerationSystemConfig
      | ((previous: ImageGenerationSystemConfig) => ImageGenerationSystemConfig),
  ) => void
  onImageProviderConfigChange: (
    providerId: ImageProviderId,
    updates: Partial<ImageProviderConfig>,
  ) => void
  onRemoveImageProvider: (providerId: ImageProviderId) => void
}

function imageProviderDisplayName(definition: ImageProviderDefinition, config: ImageProviderConfig | undefined): string {
  const label = config?.displayName?.trim()
  return label || definition.name
}

function isImageProviderConfigured(config: ImageProviderConfig | undefined): boolean {
  return Boolean(config?.baseUrl.trim() && config?.apiKey.trim() && config?.model.trim())
}

export function ImageGenerationSettingsSection({
  imageGenerationSystem,
  imageProviderConfigs,
  imageProviderDefinitions,
  onAddImageProvider,
  onImageGenerationSystemChange,
  onImageProviderConfigChange,
  onRemoveImageProvider,
}: ImageGenerationSettingsSectionProps) {
  const [selectedProviderId, setSelectedProviderId] = useState<ImageProviderId>(
    imageGenerationSystem.defaultProviderId || imageProviderDefinitions[0]?.id || 'openai_image',
  )
  const [customFormOpen, setCustomFormOpen] = useState(false)
  const [customProviderName, setCustomProviderName] = useState('')
  const [customAdapterType, setCustomAdapterType] = useState<ImageProviderAdapterType>('openai_compatible')
  const [showApiKey, setShowApiKey] = useState(false)

  const providerOptions = useMemo(() => {
    const options = [...imageProviderDefinitions]
    for (const providerId of Object.keys(imageProviderConfigs)) {
      if (options.some((definition) => definition.id === providerId)) {
        continue
      }
      const config = imageProviderConfigs[providerId]
      options.push({
        id: providerId,
        name: config.displayName.trim() || '自定义图片 Provider',
        defaultBaseUrl: config.baseUrl,
        suggestedModel: config.model,
        description: '自定义图片 Provider，可用于接入额外的图片模型网关。',
        adapterType: config.adapterType,
      })
    }
    return options
  }, [imageProviderConfigs, imageProviderDefinitions])

  const effectiveSelectedProviderId =
    providerOptions.find((item) => item.id === selectedProviderId)?.id ||
    imageGenerationSystem.defaultProviderId ||
    providerOptions[0]?.id ||
    'openai_image'
  const selectedDefinition =
    providerOptions.find((item) => item.id === effectiveSelectedProviderId) ?? providerOptions[0]
  const selectedConfig = imageProviderConfigs[effectiveSelectedProviderId] ?? {
    adapterType: selectedDefinition?.adapterType ?? 'openai_compatible',
    baseUrl: '',
    apiKey: '',
    model: '',
    note: '',
    displayName: '',
    status: '未配置' as const,
  }
  const selectedIsCustom = !imageProviderDefinitions.some((item) => item.id === effectiveSelectedProviderId)
  const availableDefaultProviders = providerOptions.filter((definition) =>
    isImageProviderConfigured(imageProviderConfigs[definition.id]),
  )
  const selectedProviderName = selectedDefinition
    ? imageProviderDisplayName(selectedDefinition, selectedConfig)
    : '图片 Provider'

  const handleCreateProvider = () => {
    const name = customProviderName.trim()
    if (!name) {
      return
    }
    const providerId = onAddImageProvider(name, customAdapterType)
    setSelectedProviderId(providerId)
    setCustomProviderName('')
    setCustomAdapterType('openai_compatible')
    setCustomFormOpen(false)
  }

  const handleRemoveSelectedProvider = () => {
    onRemoveImageProvider(effectiveSelectedProviderId)
    setSelectedProviderId(
      providerOptions.find((item) => item.id !== effectiveSelectedProviderId)?.id ||
        imageProviderDefinitions[0]?.id ||
        'openai_image',
    )
  }

  return (
    <div className="settings-section-stack image-generation-settings">
      <section className="image-settings-summary" aria-label="图片生成网关">
        <div className="image-settings-summary-copy">
          <span className="settings-sidebar-kicker">默认图片提供方</span>
          <label className="select-field image-default-select">
            <select
              value={imageGenerationSystem.defaultProviderId}
              onChange={(event) =>
                onImageGenerationSystemChange((previous) => ({
                  ...previous,
                  defaultProviderId: event.target.value,
                }))
              }
              aria-label="默认图片提供方"
            >
              {providerOptions.map((definition) => (
                <option key={definition.id} value={definition.id}>
                  {imageProviderDisplayName(definition, imageProviderConfigs[definition.id])}
                  {availableDefaultProviders.some((item) => item.id === definition.id) ? '' : '（未配全）'}
                </option>
              ))}
            </select>
          </label>
          <p>用于 image_generate Tool 的默认提供方</p>
        </div>
        <div className="image-settings-summary-card">
          <span>已配置提供方</span>
          <strong>
            {availableDefaultProviders.length} / {providerOptions.length}
          </strong>
        </div>
        <div className="image-settings-summary-card">
          <span>输出默认值</span>
          <strong>已设置</strong>
        </div>
        <button type="button" className="outline-button image-test-button" disabled>
          <AppIcon name="broadcast" size={18} />
          <span>测试当前提供方</span>
        </button>
      </section>

      <div className="image-settings-layout">
        <aside className="image-provider-list" aria-label="图片 Provider 列表">
          <div className="image-provider-list-head">
            <div>
              <strong>图片提供方</strong>
              <span>共 {providerOptions.length} 个</span>
            </div>
            <button
              type="button"
              className="image-provider-add-icon"
              onClick={() => setCustomFormOpen((previous) => !previous)}
              aria-label="添加图片 Provider"
            >
              <AppIcon name="plus" size={18} />
            </button>
          </div>

          {providerOptions.map((definition) => {
            const config = imageProviderConfigs[definition.id]
            const active = definition.id === effectiveSelectedProviderId
            const configured = isImageProviderConfigured(config)
            return (
              <button
                key={definition.id}
                type="button"
                className={`image-provider-card ${active ? 'active' : ''}`}
                onClick={() => setSelectedProviderId(definition.id)}
              >
                <span className="image-provider-icon" aria-hidden="true">
                  {IMAGE_PROVIDER_ICON_LABEL[definition.adapterType]}
                </span>
                <span className="image-provider-card-main">
                  <strong>{imageProviderDisplayName(definition, config)}</strong>
                  <span>{definition.adapterType}</span>
                </span>
                <span className={`image-provider-state ${configured ? 'configured' : ''}`}>
                  {configured ? '已配置' : '未配全'}
                </span>
              </button>
            )
          })}

          {customFormOpen ? (
            <div className="image-provider-create">
              <label className="input-field">
                <span>Provider 名称</span>
                <input
                  value={customProviderName}
                  onChange={(event) => setCustomProviderName(event.target.value)}
                  placeholder="例如：Flux 海报专用"
                />
              </label>
              <div className="input-field">
                <span>适配器类型</span>
                <label className="select-field">
                  <select
                    value={customAdapterType}
                    onChange={(event) => setCustomAdapterType(event.target.value as ImageProviderAdapterType)}
                  >
                    <option value="openai_compatible">OpenAI 兼容图片网关</option>
                    <option value="openai_images">OpenAI Images 原生接口</option>
                  </select>
                </label>
              </div>
              <div className="provider-actions">
                <button type="button" className="outline-button" onClick={() => setCustomFormOpen(false)}>
                  取消
                </button>
                <button type="button" className="outline-button primary" onClick={handleCreateProvider}>
                  创建
                </button>
              </div>
            </div>
          ) : (
            <button type="button" className="provider-add-button subtle" onClick={() => setCustomFormOpen(true)}>
              <AppIcon name="plus" size={18} />
              <span>添加图片提供方</span>
            </button>
          )}
        </aside>

        <section className="image-provider-editor">
          <div className="image-provider-editor-head">
            <div>
              <span className="settings-sidebar-kicker">当前编辑</span>
              <div className="image-provider-title-line">
                <h3>{selectedProviderName}</h3>
                <span className={`image-provider-state ${isImageProviderConfigured(selectedConfig) ? 'configured' : ''}`}>
                  {isImageProviderConfigured(selectedConfig) ? '已配置' : '未配置'}
                </span>
              </div>
            </div>
            <div className="provider-actions">
              <span className="provider-runtime-badge">{selectedConfig.adapterType}</span>
              {selectedIsCustom ? (
                <button type="button" className="outline-button danger image-remove-provider" onClick={handleRemoveSelectedProvider}>
                  <AppIcon name="trash" size={16} />
                  <span>移除提供方</span>
                </button>
              ) : null}
            </div>
          </div>

          <p className="settings-note provider-note">{selectedDefinition?.description}</p>

          <div className="image-settings-form-grid">
            <label className="input-field">
              <span>显示名称</span>
              <input
                value={selectedConfig.displayName}
                onChange={(event) =>
                  onImageProviderConfigChange(effectiveSelectedProviderId, { displayName: event.target.value })
                }
                placeholder={selectedDefinition?.name}
              />
              <small>在列表与选择器中显示的名称</small>
            </label>
            <label className="input-field">
              <span>系统默认模型</span>
              <input
                value={selectedConfig.model}
                onChange={(event) =>
                  onImageProviderConfigChange(effectiveSelectedProviderId, { model: event.target.value })
                }
                placeholder={selectedDefinition?.suggestedModel}
              />
              <small>Agent 未指定时使用的模型</small>
            </label>
            <label className="input-field image-settings-wide">
              <span>Base URL</span>
              <input
                value={selectedConfig.baseUrl}
                onChange={(event) =>
                  onImageProviderConfigChange(effectiveSelectedProviderId, { baseUrl: event.target.value })
                }
                placeholder={selectedDefinition?.defaultBaseUrl}
              />
              <small>接口地址，通常以 /v1 结尾</small>
            </label>
            <label className="input-field image-settings-wide">
              <span>API Key</span>
              <div className="input-action-field">
                <input
                  type={showApiKey ? 'text' : 'password'}
                  value={selectedConfig.apiKey}
                  onChange={(event) =>
                    onImageProviderConfigChange(effectiveSelectedProviderId, { apiKey: event.target.value })
                  }
                  placeholder="请输入图片模型 API Key"
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
            <label className="input-field image-settings-wide">
              <span>备注</span>
              <input
                value={selectedConfig.note}
                onChange={(event) =>
                  onImageProviderConfigChange(effectiveSelectedProviderId, { note: event.target.value })
                }
                placeholder="例如：海报主模型 / 草图快模 / 透明背景专用"
                maxLength={120}
              />
              <small>{selectedConfig.note.length} / 120</small>
            </label>
          </div>

          <div className="image-output-defaults">
            <div className="image-output-defaults-head">
              <strong>输出默认值</strong>
              <span>Agent 未显式指定时使用</span>
            </div>
            <div className="settings-grid">
              <label className="input-field">
                <span>尺寸</span>
                <input
                  value={imageGenerationSystem.size}
                  onChange={(event) =>
                    onImageGenerationSystemChange((previous) => ({
                      ...previous,
                      size: event.target.value,
                    }))
                  }
                  placeholder="1024x1024"
                />
              </label>
              <div className="input-field">
                <span>分辨率</span>
                <label className="select-field">
                  <select
                    value={imageGenerationSystem.resolution}
                    onChange={(event) =>
                      onImageGenerationSystemChange((previous) => ({
                        ...previous,
                        resolution: event.target.value as ImageGenerationSystemConfig['resolution'],
                      }))
                    }
                  >
                    <option value="1k">1k</option>
                    <option value="2k">2k</option>
                    <option value="4k">4k</option>
                  </select>
                </label>
              </div>
              <div className="input-field">
                <span>背景</span>
                <label className="select-field">
                  <select
                    value={imageGenerationSystem.background}
                    onChange={(event) =>
                      onImageGenerationSystemChange((previous) => ({
                        ...previous,
                        background: event.target.value as ImageGenerationSystemConfig['background'],
                      }))
                    }
                  >
                    <option value="auto">auto</option>
                    <option value="transparent">transparent</option>
                    <option value="opaque">opaque</option>
                  </select>
                </label>
              </div>
              <div className="input-field">
                <span>格式</span>
                <label className="select-field">
                  <select
                    value={imageGenerationSystem.outputFormat}
                    onChange={(event) =>
                      onImageGenerationSystemChange((previous) => ({
                        ...previous,
                        outputFormat: event.target.value as ImageGenerationSystemConfig['outputFormat'],
                      }))
                    }
                  >
                    <option value="png">png</option>
                    <option value="jpeg">jpeg</option>
                    <option value="webp">webp</option>
                  </select>
                </label>
              </div>
              <div className="input-field">
                <span>质量</span>
                <label className="select-field">
                  <select
                    value={imageGenerationSystem.quality}
                    onChange={(event) =>
                      onImageGenerationSystemChange((previous) => ({
                        ...previous,
                        quality: event.target.value as ImageGenerationSystemConfig['quality'],
                      }))
                    }
                  >
                    <option value="auto">auto</option>
                    <option value="low">low</option>
                    <option value="medium">medium</option>
                    <option value="high">high</option>
                  </select>
                </label>
              </div>
              <label className="input-field">
                <span>张数</span>
                <input
                  type="number"
                  min={1}
                  max={4}
                  value={imageGenerationSystem.count}
                  onChange={(event) =>
                    onImageGenerationSystemChange((previous) => ({
                      ...previous,
                      count: Math.max(1, Math.min(4, parseInt(event.target.value || '1', 10) || 1)),
                    }))
                  }
                />
              </label>
            </div>
          </div>
        </section>
      </div>
    </div>
  )
}
