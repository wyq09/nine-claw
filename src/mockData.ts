import type {
  BotConfig,
  BotDefinition,
  GeneralSettings,
  AppearanceSettings,
  ProviderConfig,
  ProviderDefinition,
  ResourceItem,
} from './types'

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

export function createInitialBotConfigs(): Record<string, BotConfig> {
  return {
    dingtalk: { enabled: true, clientId: '', clientSecret: '', status: '未连接' },
    lark: { enabled: false, clientId: '', clientSecret: '', status: '未连接' },
    wechat_work: { enabled: false, clientId: '', clientSecret: '', status: '未连接' },
    wechat_work_bot: { enabled: false, clientId: '', clientSecret: '', status: '未连接' },
    wechat: { enabled: false, clientId: '', clientSecret: 'https://ilinkai.weixin.qq.com', status: '未连接' },
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

export const defaultGeneralSettings: GeneralSettings = {
  language: '中文',
  launchOnStartup: true,
  useSystemProxy: false,
  submitShortcut: 'mod_enter',
}

export const defaultAppearanceSettings: AppearanceSettings = {
  compactSidebar: false,
  showExecutionRail: true,
  preferReducedMotion: false,
}
