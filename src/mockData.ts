import type {
  BotConfig,
  BotDefinition,
  GeneralSettings,
  AppearanceSettings,
  ProviderConfig,
  ProviderDefinition,
  ResourceItem,
} from './types'
import type {
  ImageGenerationSystemConfig,
  ImageProviderConfig,
  ImageProviderDefinition,
} from './types/imageGeneration'

export const resourceSeed: ResourceItem[] = [
  {
    id: 'civil-complaint',
    title: '民事起诉状模板库',
    description: '覆盖物业纠纷、劳动争议、买卖合同、侵权赔偿等常见办案入口文书。',
    tag: '模板',
    updatedAt: '今天 11:32',
  },
  {
    id: 'evidence-checklist',
    title: '证据目录与补强清单',
    description: '适合法院立案前自查，帮助快速梳理证据缺口、举证顺序与附件结构。',
    tag: '交付',
    updatedAt: '今天 09:18',
  },
  {
    id: 'legal-research',
    title: '法规检索指令集',
    description: '沉淀常用法条检索策略，便于将资源库与技能编排成固定工作流。',
    tag: '知识',
    updatedAt: '昨天',
  },
  {
    id: 'client-briefing',
    title: '客户沟通纪要',
    description: '标准化首次咨询纪要、案件推进周报与关键风险提示话术。',
    tag: '客户服务',
    updatedAt: '昨天',
  },
]

export const botDefinitions: BotDefinition[] = [
  {
    id: 'dingtalk',
    name: '钉钉',
    guideLabel: '操作指导',
    keyLabel: 'Client ID (AppKey)',
    keyPlaceholder: 'dingxxxxxx',
    secretLabel: 'Client Secret (AppSecret)',
    secretPlaceholder: '请输入 Client Secret',
  },
  {
    id: 'lark',
    name: '飞书',
    guideLabel: '操作指导',
    keyLabel: 'App ID',
    keyPlaceholder: 'cli_xxxxx',
    secretLabel: 'App Secret',
    secretPlaceholder: '请输入 App Secret',
  },
  {
    id: 'wechat_work',
    name: '企业微信',
    guideLabel: '接入说明',
    keyLabel: 'Corp ID',
    keyPlaceholder: 'wwxxxxxxxx',
    secretLabel: 'Secret',
    secretPlaceholder: '请输入应用 Secret',
  },
  {
    id: 'wechat_work_bot',
    name: '企业微信 Bot',
    guideLabel: 'Webhook 指南',
    keyLabel: 'Webhook Key',
    keyPlaceholder: 'key_xxxxx',
    secretLabel: '签名密钥',
    secretPlaceholder: '请输入签名密钥',
  },
  {
    id: 'wechat',
    name: '微信',
    guideLabel: '接入说明',
    keyLabel: 'Bot Token',
    keyPlaceholder: '扫码后自动填入',
    secretLabel: 'iLink 服务地址',
    secretPlaceholder: 'https://ilinkai.weixin.qq.com',
  },
  {
    id: 'peer',
    name: '虾 / 对等互通',
    guideLabel: 'HTTP 入站',
    keyLabel: '说明',
    keyPlaceholder: '见文档 docs/AGENT_PEER_INTEROP.md',
    secretLabel: '本智能体入站密钥',
    secretPlaceholder: '保存智能体后自动生成，可自填或点重新生成',
  },
]

export const providerDefinitions: ProviderDefinition[] = [
  {
    id: 'openai',
    name: 'OpenAI',
    defaultBaseUrl: 'https://api.openai.com/v1',
    suggestedModel: 'gpt-4.1',
    description: '适合通用推理、代码和工具调用场景。',
    apiFormat: 'openai',
  },
  {
    id: 'anthropic',
    name: 'Anthropic',
    defaultBaseUrl: 'https://api.anthropic.com',
    suggestedModel: 'claude-sonnet-4-0',
    description: '适合长文本、复杂分析和稳健对话。',
    apiFormat: 'anthropic',
  },
  {
    id: 'deepseek',
    name: 'DeepSeek',
    defaultBaseUrl: 'https://api.deepseek.com',
    suggestedModel: 'deepseek-chat',
    description: '适合成本敏感场景和中文任务。',
    apiFormat: 'openai',
  },
  {
    id: 'doubao',
    name: 'Doubao',
    defaultBaseUrl: 'https://ark.cn-beijing.volces.com/api/v3',
    suggestedModel: 'doubao-seed-1-6-thinking',
    description: '适合火山引擎体系内模型与企业接入。',
    apiFormat: 'openai',
  },
  {
    id: 'siliconflow',
    name: 'SiliconFlow',
    defaultBaseUrl: 'https://api.siliconflow.cn/v1',
    suggestedModel: 'deepseek-ai/DeepSeek-V3',
    description: '适合聚合多模型接入和快速试用。',
    apiFormat: 'openai',
  },
]

export const imageProviderDefinitions: ImageProviderDefinition[] = [
  {
    id: 'apimart_gpt_image_2',
    name: 'APIMart GPT-Image-2',
    defaultBaseUrl: 'https://api.apimart.ai/v1',
    suggestedModel: 'gpt-image-2-official',
    description: 'APIMart 官方 GPT-Image-2 异步生图通道，提交任务后通过 task_id 轮询结果。',
    adapterType: 'apimart_gpt_image_2',
  },
  {
    id: 'openai_image',
    name: 'OpenAI Images',
    defaultBaseUrl: 'https://api.openai.com/v1',
    suggestedModel: 'gpt-image-1',
    description: 'OpenAI 原生图片生成接口，优先用于直接接 OpenAI 的场景。',
    adapterType: 'openai_images',
  },
  {
    id: 'siliconflow_image',
    name: 'SiliconFlow Images',
    defaultBaseUrl: 'https://api.siliconflow.cn/v1',
    suggestedModel: 'black-forest-labs/FLUX.1-schnell',
    description: '适合挂 SiliconFlow 这类 OpenAI 兼容图片网关。',
    adapterType: 'openai_compatible',
  },
  {
    id: 'doubao_image',
    name: 'Doubao Images',
    defaultBaseUrl: 'https://ark.cn-beijing.volces.com/api/v3',
    suggestedModel: 'doubao-seedream-3-0-t2i-250415',
    description: '预留给火山 / 豆包图片模型接入，当前走兼容适配层。',
    adapterType: 'openai_compatible',
  },
  {
    id: 'custom_image',
    name: 'Custom Image Gateway',
    defaultBaseUrl: 'https://api.example.com/v1',
    suggestedModel: 'your-image-model',
    description: '自定义 OpenAI 兼容图片网关入口，后续新增供应商时优先落这里。',
    adapterType: 'openai_compatible',
  },
]

export function createInitialBotConfigs(): Record<string, BotConfig> {
  return {
    dingtalk: { enabled: true, imChannelPaused: false, clientId: '', clientSecret: '', status: '未连接' },
    lark: { enabled: false, imChannelPaused: false, clientId: '', clientSecret: '', status: '未连接' },
    wechat_work: { enabled: false, imChannelPaused: false, clientId: '', clientSecret: '', status: '未连接' },
    wechat_work_bot: { enabled: false, imChannelPaused: false, clientId: '', clientSecret: '', status: '未连接' },
    wechat: {
      enabled: false,
      imChannelPaused: false,
      clientId: '',
      clientSecret: 'https://ilinkai.weixin.qq.com',
      status: '未连接',
    },
    peer: {
      enabled: false,
      imChannelPaused: false,
      clientId: '',
      clientSecret: '',
      status: '未连接',
    },
  }
}

export function emptyProviderConfig(): ProviderConfig {
  return {
    enabled: false,
    added: false,
    apiFormat: 'openai',
    baseUrl: '',
    apiKey: '',
    model: '',
    note: '',
    displayName: '',
    status: '未配置',
  }
}

export function createInitialProviderConfigs(): Record<string, ProviderConfig> {
  return {
    openai: {
      enabled: false,
      added: false,
      apiFormat: 'openai',
      baseUrl: 'https://api.openai.com/v1',
      apiKey: '',
      model: 'gpt-4.1',
      note: '保留用于后续多 provider 升级。',
      displayName: '',
      status: '未配置',
    },
    anthropic: {
      enabled: false,
      added: false,
      apiFormat: 'anthropic',
      baseUrl: 'https://api.anthropic.com',
      apiKey: '',
      model: 'claude-sonnet-4-0',
      note: '',
      displayName: '',
      status: '未配置',
    },
    deepseek: {
      enabled: false,
      added: false,
      apiFormat: 'openai',
      baseUrl: 'https://api.deepseek.com',
      apiKey: '',
      model: 'deepseek-chat',
      note: '',
      displayName: '',
      status: '未配置',
    },
    doubao: {
      enabled: false,
      added: false,
      apiFormat: 'openai',
      baseUrl: 'https://ark.cn-beijing.volces.com/api/v3',
      apiKey: '',
      model: 'doubao-seed-1-6-thinking',
      note: '',
      displayName: '',
      status: '未配置',
    },
    siliconflow: {
      enabled: false,
      added: false,
      apiFormat: 'openai',
      baseUrl: 'https://api.siliconflow.cn/v1',
      apiKey: '',
      model: 'deepseek-ai/DeepSeek-V3',
      note: '',
      displayName: '',
      status: '未配置',
    },
  }
}

export function emptyImageProviderConfig(adapterType: ImageProviderDefinition['adapterType']): ImageProviderConfig {
  return {
    adapterType,
    baseUrl: '',
    apiKey: '',
    model: '',
    note: '',
    displayName: '',
    status: '未配置',
  }
}

export function createInitialImageProviderConfigs(): Record<string, ImageProviderConfig> {
  return imageProviderDefinitions.reduce<Record<string, ImageProviderConfig>>((accumulator, definition) => {
    accumulator[definition.id] = {
      adapterType: definition.adapterType,
      baseUrl: definition.defaultBaseUrl,
      apiKey: '',
      model: definition.suggestedModel,
      note: '',
      displayName: '',
      status: '未配置',
    }
    return accumulator
  }, {})
}

export const defaultImageGenerationSystemConfig: ImageGenerationSystemConfig = {
  defaultProviderId: 'openai_image',
  size: '1024x1024',
  resolution: '1k',
  background: 'auto',
  outputFormat: 'png',
  quality: 'auto',
  count: 1,
}

export const defaultGeneralSettings: GeneralSettings = {
  language: '中文',
  launchOnStartup: true,
  useSystemProxy: false,
  customProxyUrl: '',
  submitShortcut: 'mod_enter',
  llmCallLogDir: '',
}

export const defaultAppearanceSettings: AppearanceSettings = {
  themeMode: 'dark',
  compactSidebar: false,
  sidebarCollapsed: false,
  showThinkingProcess: true,
  showExecutionRail: true,
  preferReducedMotion: false,
}
