export type PiStreamEventName =
  | 'start'
  | 'delta'
  | 'done'
  | 'error'
  | 'aborted'
  | 'thinking_start'
  | 'thinking_delta'
  | 'thinking_end'
  | 'tool_execution_start'
  | 'tool_execution_update'
  | 'tool_execution_end'

export type PiAbortSource = 'user' | 'model'

export type TokenUsage = {
  inputTokens: number
  outputTokens: number
  cacheReadTokens: number
  cacheWriteTokens: number
  totalTokens: number
}

export type PiStreamPayload = {
  event: PiStreamEventName
  sessionId?: string
  session_id?: string
  text?: string
  error?: string
  abortedBy?: PiAbortSource
  aborted_by?: PiAbortSource
  toolCallId?: string
  tool_call_id?: string
  toolName?: string
  tool_name?: string
  argsText?: string
  args_text?: string
  /** 若存在则拼接到当前 args 之后（增量片段）；否则用 argsText 整段覆盖 */
  argsDelta?: string
  args_delta?: string
  resultText?: string
  result_text?: string
  /** 若存在则拼接到当前 result 之后（增量片段）；否则用 resultText 整段覆盖 */
  resultDelta?: string
  result_delta?: string
  isError?: boolean
  is_error?: boolean
  reason?: string
  inputTokens?: number
  input_tokens?: number
  outputTokens?: number
  output_tokens?: number
  cacheReadTokens?: number
  cache_read_tokens?: number
  cacheWriteTokens?: number
  cache_write_tokens?: number
  totalTokens?: number
  total_tokens?: number
}

export type HistoryStatus = 'running' | 'done' | 'error' | 'aborted_user' | 'aborted_model'

export type ActivityState = 'running' | 'done' | 'error'

export type ActivityEntry = {
  id: string
  label: string
  detail: string
  state: ActivityState
  createdAt: number
  completedAt?: number
}

export type ToolCallEntry = {
  id: string
  toolCallId: string
  toolName: string
  argsText: string
  resultText: string
  state: ActivityState
  createdAt: number
  completedAt?: number
}

/** 与 pi 流式事件顺序一致：文本块与工具块交错出现。 */
export type ResponseSegment =
  | { type: 'text'; text: string }
  | { type: 'tool'; toolCallId: string }

export type AgentExecutionMode = 'single' | 'supervisor' | 'worker'

export type AgentSharedContextPolicy = 'session' | 'summary' | 'none'

export type AgentCollaborationConfig = {
  allowedDelegateAgentIds: string[]
  handoffPrompt: string
  sharedContextPolicy: AgentSharedContextPolicy
}

export type ConversationAgentSnapshot = {
  id: string
  name: string
  summary: string
  description: string
  systemPrompt: string
  skillIds: string[]
  defaultProviderId: ProviderId
  defaultModel: string
  executionMode: AgentExecutionMode
  collaborationConfig?: AgentCollaborationConfig
  accentColor?: string
}

export type ConversationTurn = {
  id: string
  prompt: string
  answer: string
  status: HistoryStatus
  createdAt: number
  completedAt?: number
  usage?: TokenUsage
  activity: ActivityEntry[]
  thinking: string
  toolCalls: ToolCallEntry[]
  /** 有值时按此顺序渲染正文与工具；缺省为旧版仅 answer + toolCalls */
  responseSegments?: ResponseSegment[]
}

/** 内置 id 为固定字符串；自定义为 custom_ 前缀 */
export type ProviderId = string

export type HistoryItem = {
  id: string
  title: string
  status: HistoryStatus
  createdAt: number
  updatedAt: number
  turns: ConversationTurn[]
  agent?: ConversationAgentSnapshot
  /** 本会话单独指定的大模型（与全局默认无关，持久化） */
  sessionLlmProviderId?: ProviderId
  sessionLlmModel?: string
}

export type ViewKey = 'chat' | 'skills' | 'resources' | 'agents'

export type SettingsTab = 'general' | 'appearance' | 'providers' | 'bots' | 'shortcuts'

export type SkillLibraryTab = 'installed' | 'system'

export type InstalledSkillScope = 'workspace' | 'global'

export type InstalledSkillInstallType = 'directory' | 'symlink'

export type InstalledSkillItem = {
  id: string
  name: string
  description: string
  path: string
  manifestPath: string
  scope: InstalledSkillScope
  installType: InstalledSkillInstallType
  updatedAt: number
  source?: string
  sourceType?: string
}

export type SystemSkillItem = {
  id: string
  name: string
  description: string
  installUrl?: string | null
  installed: boolean
}

export type SystemSkillCatalog = {
  available: boolean
  updatedAt?: number | null
  message: string
  skills: SystemSkillItem[]
}

export type ResourceItem = {
  id: string
  title: string
  description: string
  tag: string
  updatedAt: string
}

export type AgentRecord = {
  id: string
  name: string
  summary: string
  description: string
  systemPrompt: string
  skillIds: string[]
  defaultProviderId: ProviderId
  defaultModel: string
  isBuiltin: boolean
  isArchived: boolean
  executionMode: AgentExecutionMode
  collaborationConfig?: AgentCollaborationConfig
  accentColor?: string
  botConfigs: AgentBotBindings
  createdAt: number
  updatedAt: number
}

export type AgentInput = {
  name: string
  summary: string
  description: string
  systemPrompt: string
  skillIds: string[]
  defaultProviderId: ProviderId
  defaultModel: string
  executionMode: AgentExecutionMode
  collaborationConfig?: AgentCollaborationConfig
  accentColor?: string
  botConfigs: AgentBotBindings
}

export type AgentBuilderDraft = {
  name: string
  summary: string
  description: string
  systemPrompt: string
  skillIds: string[]
  defaultProviderId: ProviderId
  defaultModel: string
  executionMode: AgentExecutionMode
  collaborationConfig?: AgentCollaborationConfig
  accentColor?: string
  botConfigs?: AgentBotBindings
  workspaceNotes?: string
}

export type AgentWorkspaceFile = {
  key: string
  scope: 'agent' | 'shared'
  section: 'private' | 'shared' | 'dailyLog'
  name: string
  relativePath: string
  absolutePath: string
  readOnly: boolean
  exists: boolean
  content: string
}

export type AgentWorkspaceBundle = {
  agentId: string
  workspaceRoot: string
  agentHome: string
  files: AgentWorkspaceFile[]
}

export type BotChannelId = 'dingtalk' | 'lark' | 'wechat_work' | 'wechat_work_bot' | 'wechat'

export type RuntimeDependencyStatus = {
  platform: string
  nodeAvailable: boolean
  npmAvailable: boolean
  piAvailable: boolean
  autoInstallAttempted: boolean
  autoInstallSucceeded: boolean
  messages: string[]
}

export type BotDefinition = {
  id: BotChannelId
  name: string
  guideLabel: string
  keyLabel: string
  keyPlaceholder: string
  secretLabel: string
  secretPlaceholder: string
}

export type BotConfig = {
  enabled: boolean
  clientId: string
  clientSecret: string
  status: '未连接' | '待接入' | '已连接' | '登录中' | '错误'
  /** iLink bot token (returned after QR login) */
  token?: string
  /** iLink server base URL override */
  baseUrl?: string
  /** iLink route tag */
  routeTag?: string
  aiProviderId?: ProviderId
  aiApiFormat?: ProviderApiFormat
  aiBaseUrl?: string
  aiApiKey?: string
  aiModel?: string
  /** Error message when status === '错误' */
  errorMessage?: string
}

export type AgentBotBindings = Record<string, BotConfig>

export type CustomProviderMeta = {
  id: string
  name: string
  description: string
  apiFormat: ProviderApiFormat
}

export type ProviderApiFormat = 'openai' | 'anthropic'

export type ProviderDefinition = {
  id: ProviderId
  name: string
  defaultBaseUrl: string
  suggestedModel: string
  description: string
  apiFormat: ProviderApiFormat
  /** 用户添加的 OpenAI 兼容供应商 */
  isCustom?: boolean
}

export type ProviderConfig = {
  enabled: boolean
  added: boolean
  apiFormat: ProviderApiFormat
  baseUrl: string
  apiKey: string
  model: string
  note: string
  /** 界面展示名，空则用 Provider 预设名称 */
  displayName: string
  status: '未配置' | '已配置' | '测试通过'
}

export type SubmitShortcut = 'enter' | 'mod_enter'

export type GeneralSettings = {
  language: '中文' | 'English'
  launchOnStartup: boolean
  useSystemProxy: boolean
  submitShortcut: SubmitShortcut
}

export type AppearanceSettings = {
  compactSidebar: boolean
  showExecutionRail: boolean
  preferReducedMotion: boolean
}

export type ProviderRuntimeConfig = {
  providerId: ProviderId
  apiFormat: ProviderApiFormat
  baseUrl: string
  apiKey: string
  model: string
}

/** Payload emitted from Rust via `bot://message` for history tracking. */
export type BotMessagePayload = {
  channel_id: string
  user_id: string
  /** "inbound" | "outbound" | "outbound_chunk" | "outbound_done" | "error" */
  direction: string
  content: string
  timestamp: number
}
