import type { WidgetSegment } from './widgetTypes'

export type PiStreamEventName =
  | 'start'
  | 'delta'
  | 'final_text'
  | 'skill_selection'
  | 'widget_request'
  | 'widget_resolved'
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
  api?: string
  provider?: string
  model?: string
  responseId?: string
  timestamp?: number
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
  strategy?: string
  mountedSkillIds?: string[]
  mounted_skill_ids?: string[]
  reasons?: string[]
  widget?: unknown
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
  api?: string
  provider?: string
  model?: string
  responseId?: string
  response_id?: string
  timestamp?: number
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
/** 团队空间：主智能体提出的委派计划条目。 */
export type DelegatePlanItem = {
  /** 执行该子任务的成员 agentId */
  assignee: string
  /** 任务文案（用户可在下发前编辑） */
  task: string
  /** 主智能体给出的"为什么选他"的理由，可选 */
  reason?: string
  /** 是否启用，默认 true；用户可勾除不下发 */
  enabled?: boolean
}

/** 团队空间：一次具体委派的运行态。 */
export type DelegationTurnEntry = {
  /** 0-based 顺序号 */
  index: number
  /** `thinking` 代表一次 turn_end（中间思考轮），`agent` 代表 agent_end */
  kind: 'thinking' | 'agent'
  /** 可选简介 */
  summary?: string
}

export type DelegationToolCallEntry = {
  /** 0-based 顺序号（按 toolCallId 去重递增） */
  index: number
  /** pi 侧 toolCallId，前端按它聚合 start/end */
  toolCallId: string
  /** 工具名，例如 `web_search` / `stock_price` / `list_dir` */
  toolName: string
  /** 入参摘要（截断到 ~120 字符），供展开时预览 */
  argsDigest?: string
  status: 'running' | 'done' | 'error'
  /** 工具是否报错 */
  isError?: boolean
}

export type DelegationRunSegment = {
  /** 运行 id，前后端共享 */
  runId: string
  assignee: string
  task: string
  status: 'pending' | 'running' | 'done' | 'aborted' | 'error'
  /** 已收集到的输出（flat text） */
  output: string
  /** 开始时间（ms） */
  startedAt?: number
  /** 已用时（ms） */
  elapsedMs?: number
  /** 错误信息 */
  error?: string | null
  /** 实时思考轮（turn_end / agent_end），按 `workspace:delegate:turn` 事件聚合 */
  turns?: DelegationTurnEntry[]
  /** 实时工具调用列表（tool_execution_start/end），按 `workspace:delegate:tool` 事件聚合 */
  toolCalls?: DelegationToolCallEntry[]
}

// ── Agent Loop 类型 ──────────────────────────────────────────

export type BatchFailStrategy = 'failFast' | 'waitAll'

export type AgentLoopConfig = {
  maxIterations: number
  iterationTimeoutMs: number
  enableNested: boolean
  maxDepth: number
  allowExtend: boolean
  maxExtendLimit: number
  maxConcurrent: number
  batchFailStrategy: BatchFailStrategy
}

export type AgentLoopIteration = {
  iteration: number
  markerType: 'call' | 'batch' | 'extend'
  status: 'pending' | 'running' | 'reviewing' | 'completed'
  delegate?: {
    agentId: string
    agentName: string
    task: string
    params?: Record<string, unknown>
    output?: string
    toolCallsCount?: number
    durationMs?: number
  }
  batch?: {
    delegates: Array<{
      agentId: string
      agentName: string
      task: string
      status: 'running' | 'completed' | 'error' | 'cancelled'
      output?: string
      durationMs?: number
    }>
  }
}

export type AgentLoopSegment = {
  type: 'agent_loop'
  loopId: string
  status: 'running' | 'completed' | 'aborted' | 'error'
  reason?: 'max_iterations' | 'natural' | 'abort' | 'error'
  totalIterations: number
  currentDepth: number
  iterations: AgentLoopIteration[]
  startedAt: number
  completedAt?: number
}

export type AgentLoopReviewSegment = {
  type: 'agent_loop_review'
  loopId: string
  reviewType: 'pause_for_review' | 'extend'
  iteration: number
  delegateInfo?: { agentName: string; task: string }
  extendInfo?: { currentMax: number; requestedMax: number; reason: string }
  status: 'pending' | 'approved' | 'rejected'
}

export type ResponseSegment =
  | { type: 'text'; text: string }
  | { type: 'tool'; toolCallId: string }
  | WidgetSegment
  | { type: 'delegate_plan'; planId: string; items: DelegatePlanItem[] }
  | { type: 'delegation_run'; run: DelegationRunSegment }
  | { type: 'agent_loop'; segment: AgentLoopSegment }
  | { type: 'agent_loop_review'; segment: AgentLoopReviewSegment }

export type ChatAttachmentKind = 'image' | 'video' | 'audio' | 'file'

export type ChatAttachmentUpload = {
  fileName: string
  mimeType?: string | null
  dataBase64?: string | null
  sourcePath?: string | null
}

export type PersistedChatAttachment = {
  id: string
  fileName: string
  filePath: string
  mimeType: string
  size: number
  kind: ChatAttachmentKind
}

export type AgentExecutionMode = 'single' | 'supervisor' | 'worker'

export type AgentSkillStrategy = 'static' | 'hybrid' | 'dynamic'

export type AgentToolId =
  | 'bash'
  | 'read_file'
  | 'write_file'
  | 'edit_file'
  | 'grep'
  | 'list_dir'
  | 'glob'
  | 'web_search'
  | 'web_fetch'
  | 'image_generate'
  | 'image_task_query'
  | 'ask_user'
  | 'agent_spawn'
  | 'external_api'
  | 'memory_update'
  | 'memory_search'
  | 'memory_read'
  | 'memory_delete'
  | 'memory_store'
  | 'memory_save'
  | 'memory_get'
  | 'memory_forget'
  | 'memory_list'
  | 'chat_search'
  | 'create_scheduled_task'
  | 'query_scheduled_task'
  | 'query_scheduled_task_info'
  | 'mcp_tool'
  | 'mcp_config'

export type AgentCapabilityPolicy = {
  strategy: AgentSkillStrategy
  requiredSkillIds: string[]
  forbiddenSkillIds: string[]
  maxDynamicSkills: number
}

export type AgentSharedContextPolicy = 'session' | 'summary' | 'none'

export type AgentCollaborationConfig = {
  allowedDelegateAgentIds: string[]
  handoffPrompt: string
  sharedContextPolicy: AgentSharedContextPolicy
}

export type AgentHeartbeatTaskType = 'notify' | 'shell'

export type AgentHeartbeatTask = {
  id: string
  name: string
  description: string
  taskType: AgentHeartbeatTaskType
  enabled: boolean
  messageTemplate: string
  command: string
  workingDirectory: string
  timeoutSec: number
  notifyOnSuccess: boolean
  notifyOnFailure: boolean
}

export type AgentHeartbeatSchedule = {
  id: string
  name: string
  enabled: boolean
  taskId: string
  scheduleType: 'daily'
  times: string[]
  channelId: string
  targetUserId: string
  targetLabel: string
}

export type AgentHeartbeatConfig = {
  timezone: string
  tasks: AgentHeartbeatTask[]
  schedules: AgentHeartbeatSchedule[]
}

/** 可选场景模型；未设置时由运行时回退到默认对话模型 */
export type AgentScenarioLlmSlot = {
  providerId: ProviderId
  model: string
}

export type AgentScenarioLlmConfig = {
  titleGeneration?: AgentScenarioLlmSlot
  memoryExtraction?: AgentScenarioLlmSlot
  /** 定时任务在任务列表 / 系统推送里展示的标题与一句话简介（与 refine 任务元数据同一次调用） */
  taskPushNotificationCopy?: AgentScenarioLlmSlot
}

export type ConversationAgentSnapshot = {
  id: string
  name: string
  summary: string
  description: string
  avatarUri?: string
  triggerCondition?: string
  manualTriggerOnly?: boolean
  systemPrompt: string
  capabilityPolicy: AgentCapabilityPolicy
  skillIds: string[]
  allowedToolIds: AgentToolId[]
  defaultProviderId: ProviderId
  defaultModel: string
  executionMode: AgentExecutionMode
  collaborationConfig?: AgentCollaborationConfig
  accentColor?: string
  scenarioLlmConfig?: AgentScenarioLlmConfig
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
  /** 团队会话中该轮实际发言的智能体 */
  speakerAgentId?: string
  /**
   * 轮类型。`'normal'`（默认）= 常规一来一回；`'note'` = 用户"旁白补充"，
   * 不请求 LLM，仅落库并在下一次调用时作为 `[USER_NOTES]` 注入上下文。
   */
  kind?: 'normal' | 'note'
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
  botTarget?: BotConversationTarget
  /** 本会话单独指定的大模型（与全局默认无关，持久化） */
  sessionLlmProviderId?: ProviderId
  sessionLlmModel?: string
  /** 非空表示本会话属于某团队工作空间（群聊 / 多智能体） */
  workspaceId?: string
  /** 会话创建时自动生成的话题工作区目录。 */
  topicWorkspaceDir?: string
  /** 当前产物写入目录，可切换到外部文件夹。 */
  currentWorkspaceDir?: string
}

/** Session list item returned from the structured chat API (no turns). */
export type ChatSessionListItem = {
  id: string
  title: string
  status: HistoryStatus
  created_at: number
  updated_at: number
  agent_id: string | null
  agent_snapshot_json: string | null
  bot_target_json: string | null
  session_llm_provider_id: string | null
  session_llm_model: string | null
  workspace_id: string | null
  topic_workspace_dir?: string | null
  current_workspace_dir?: string | null
  turn_count: number
}

/** Full session detail with turns from the structured chat API. */
export type ChatSessionDetail = {
  id: string
  title: string
  status: HistoryStatus
  created_at: number
  updated_at: number
  agent_id: string | null
  agent_snapshot_json: string | null
  bot_target_json: string | null
  session_llm_provider_id: string | null
  session_llm_model: string | null
  workspace_id: string | null
  topic_workspace_dir?: string | null
  current_workspace_dir?: string | null
  turns: ChatTurnRow[]
}

/** Database row for a chat turn. */
export type ChatTurnRow = {
  id: string
  session_id: string
  turn_index: number
  prompt: string
  answer: string
  thinking: string
  status: HistoryStatus
  created_at: number
  completed_at: number | null
  usage_json: string | null
  response_segments_json: string | null
  tool_calls_json: string | null
  activity_json: string | null
  speaker_agent_id: string | null
}

export type ViewKey = 'chat' | 'agents' | 'tasks' | 'workspaces'

export type {
  ArtifactsTreeEntry,
  WorkspaceMemoryRecord,
  WorkspaceMemberView,
  WorkspaceRecord,
  WorkspaceResourceRecord,
} from './workspaceTypes'

/** 全局 runtime 参数（「设置 → 参数」）与 `stream_pi_prompt` / 委派 Agent Loop 共享 */
export type RuntimeParameters = {
  maxAgentToolRoundsPerDialogue: number
  streamDisconnectMaxRetries: number
  llmOuterMaxAttempts: number
}

export type SettingsTab =
  | 'general'
  | 'appearance'
  | 'parameters'
  | 'providers'
  | 'mcp'
  | 'usage'
  | 'shortcuts'
  | 'skills'
  | 'resources'
  | 'memory'
  | 'logs'

export type TokenUsageRecord = {
  turnId: string
  sessionId: string
  turnCreatedAt: number
  turnCompletedAt?: number | null
  agentId?: string | null
  agentName?: string | null
  api?: string | null
  provider?: string | null
  model?: string | null
  responseId?: string | null
  usageTimestamp?: number | null
  inputTokens: number
  outputTokens: number
  cacheReadTokens: number
  cacheWriteTokens: number
  totalTokens: number
  recordedAt: number
}

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
  avatarUri?: string
  triggerCondition: string
  manualTriggerOnly: boolean
  systemPrompt: string
  capabilityPolicy: AgentCapabilityPolicy
  skillIds: string[]
  allowedToolIds: AgentToolId[]
  defaultProviderId: ProviderId
  defaultModel: string
  isBuiltin: boolean
  isArchived: boolean
  executionMode: AgentExecutionMode
  collaborationConfig?: AgentCollaborationConfig
  accentColor?: string
  scenarioLlmConfig?: AgentScenarioLlmConfig
  agentLoopConfig?: AgentLoopConfig
  botConfigs: AgentBotBindings
  heartbeatConfig: AgentHeartbeatConfig
  createdAt: number
  updatedAt: number
}

export type AgentPresetSummary = {
  id: string
  name: string
  summary: string
  description: string
  accentColor?: string
  skillIds: string[]
  allowedToolIds: AgentToolId[]
  isDefault: boolean
}

export type AgentInput = {
  id?: string
  name: string
  summary: string
  description: string
  avatarUri?: string
  triggerCondition: string
  manualTriggerOnly: boolean
  systemPrompt: string
  capabilityPolicy?: AgentCapabilityPolicy
  skillIds: string[]
  allowedToolIds: AgentToolId[]
  defaultProviderId: ProviderId
  defaultModel: string
  executionMode: AgentExecutionMode
  collaborationConfig?: AgentCollaborationConfig
  accentColor?: string
  scenarioLlmConfig?: AgentScenarioLlmConfig
  botConfigs: AgentBotBindings
  heartbeatConfig: AgentHeartbeatConfig
  agentLoopConfig?: AgentLoopConfig
}

export type AgentImportResult = {
  agent: AgentRecord
  warnings: string[]
}

export type AgentBuilderDraft = {
  id?: string
  name: string
  summary: string
  description: string
  avatarUri?: string
  triggerCondition?: string
  manualTriggerOnly?: boolean
  systemPrompt: string
  capabilityPolicy?: AgentCapabilityPolicy
  skillIds: string[]
  allowedToolIds?: AgentToolId[]
  defaultProviderId: ProviderId
  defaultModel: string
  executionMode: AgentExecutionMode
  collaborationConfig?: AgentCollaborationConfig
  accentColor?: string
  scenarioLlmConfig?: AgentScenarioLlmConfig
  botConfigs?: AgentBotBindings
  heartbeatConfig?: AgentHeartbeatConfig
  agentLoopConfig?: AgentLoopConfig
  workspaceNotes?: string
}

export type AgentWorkspaceFile = {
  key: string
  scope: 'agent' | 'shared'
  section: 'private' | 'shared' | 'dailyLog' | 'memoryIndex' | 'wiki'
  name: string
  relativePath: string
  absolutePath: string
  readOnly: boolean
  exists: boolean
  content: string
  /** 为 true 时正文未预载，选中时由前端再请求完整内容 */
  lazyFetch?: boolean
}

export type AgentWorkspaceBundle = {
  agentId: string
  workspaceRoot: string
  agentHome: string
  files: AgentWorkspaceFile[]
}

export type BotChannelId = 'dingtalk' | 'lark' | 'peer' | 'wechat_work' | 'wechat_work_bot' | 'wechat'

export type PeerGatewayInfo = {
  enabled: boolean
  /** 已由环境变量 NINECLAW_PEER_BIND 覆盖，应用内端口无效 */
  envOverrideActive: boolean
  listenAddress: string | null
  publicBaseUrl: string | null
  inboundUrl: string | null
  healthUrl: string | null
}

export type PeerGatewaySettings = {
  enabled: boolean
  host: string
  port: number
  publicBase: string
}

export type RuntimeDependencyStatus = {
  platform: string
  nodeAvailable: boolean
  npmAvailable: boolean
  piAvailable: boolean
  bundledPiAvailable: boolean
  bundledPiPath?: string | null
  resolvedPiPath?: string | null
  piSource?: 'bundled' | 'system_path' | null
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
  /** 用户点「断开」后为 true；为 false 时应用重启会自动拉起已绑定凭证的通道 */
  imChannelPaused?: boolean
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
  /** 对等入站专用（优先于下方 clientSecret 映射）；也可仅在「虾 / 对等」里填 clientSecret */
  peerSharedSecret?: string
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
  /** 手动配置的最大上下文窗口（tokens），留空表示自动检测 */
  maxContextTokens?: number
}

export type SubmitShortcut = 'enter' | 'mod_enter'

export type NetworkProxySettings = {
  useSystemProxy: boolean
  customProxyUrl: string
}

export type EmbeddingMode = 'local' | 'remote'

export type LocalModelDownloadState = 'idle' | 'downloading' | 'ready' | 'failed'

export type EmbeddingSettings = {
  mode: EmbeddingMode
  remoteEndpoint: string
  remoteModelName: string
  remoteApiKey: string
  remoteDimension: number
}

export type EmbeddingProviderStatus = {
  activeProviderId?: string | null
  mode: EmbeddingMode
  localModelReady: boolean
  localModelPath: string
  localDownloadState: LocalModelDownloadState
  remoteConfigured: boolean
  vectorCount: number
  providerCount: number
  message: string
}

export type EmbeddingReindexResult = {
  indexed: number
  skipped: number
  providerId?: string | null
  searchModeReady: boolean
  message: string
}

export type GeneralSettings = {
  language: '中文' | 'English'
  launchOnStartup: boolean
  useSystemProxy: boolean
  customProxyUrl: string
  submitShortcut: SubmitShortcut
  /** 额外导出 LLM 调用链 jsonl 的目录，空则仅写入工作区/.debug */
  llmCallLogDir: string
  /** Agent 循环 / 流式 / LLM 外层重试等全局运行时参数 */
  runtimeParameters: RuntimeParameters
  /** 是否开启 Agent 回复完成的系统推送通知 */
  notificationEnabled: boolean
}

export type ThemeMode = 'dark' | 'light' | 'claude' | 'shrimp_tide' | 'paper_ink'

export type AppearanceSettings = {
  themeMode: ThemeMode
  compactSidebar: boolean
  sidebarCollapsed: boolean
  /** 展示模型思考过程（thinking 流） */
  showThinkingProcess: boolean
  /** 展示工具调用卡片与分段中的工具块 */
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

export type ScheduledJobRecord = {
  id: string
  sourceKind: string
  ownerAgentId: string
  sourceScheduleId: string
  sourceTaskId: string
  kind: string
  name: string
  description: string
  enabled: boolean
  timezone: string
  triggerType: string
  triggerSpecJson: string
  payloadJson: string
  deliveryJson: string
  nextRunAt?: number | null
  lastRunAt?: number | null
  lastSyncedAt: number
  createdAt: number
  updatedAt: number
}

export type ScheduledJobRunRecord = {
  id: string
  jobId: string
  scheduledFor: number
  claimedAt: number
  startedAt?: number | null
  finishedAt?: number | null
  status: string
  attempt: number
  workerId?: string | null
  summary?: string | null
  details?: string | null
  error?: string | null
  outputPath?: string | null
  createdAt: number
  updatedAt: number
}

export type SchedulerServiceStatus = {
  installed: boolean
  platform: string
  detail: string
  launcherPath?: string | null
}

export type AgentTaskDeliveryRecord = {
  id: string
  taskId: string
  runId: string
  agentId: string
  sessionId: string
  title: string
  content: string
  createdAt: number
  /** 桌面投递时附带，用于新建会话时恢复智能体上下文 */
  agent?: ConversationAgentSnapshot
}

export type AgentTaskListItem = {
  id: string
  agentId: string
  agentName: string
  sourceSessionId: string
  title: string
  intentSummary: string
  taskType: string
  scheduleType: string
  timezone: string
  goal: string
  intervalMinutes?: number | null
  dailyTimes: string[]
  weeklyDays: number[]
  monthlyDays: number[]
  /** 一次性任务：计划触发时间（UTC 毫秒） */
  runAtMs?: number | null
  /** 是否在独立会话中展示执行结果 */
  resultInNewSession?: boolean
  status: string
  nextRunAt?: number | null
  lastRunAt?: number | null
  deliveryKind: string
  deliveryTarget: string
  createdAt: number
  updatedAt: number
}

export type AgentTaskUpdateInput = {
  title: string
  goal: string
  scheduleType: string
  timezone: string
  intervalMinutes?: number | null
  dailyTimes: string[]
  weeklyDays: number[]
  monthlyDays: number[]
  runAtMs?: number | null
  resultInNewSession?: boolean
}

export type SchedulerRuntimeStatus = {
  service: SchedulerServiceStatus
  daemonActive: boolean
  leaderOwnerId?: string | null
  leaderLeasedUntil?: number | null
  activeRunCount: number
}

export type SchedulerSyncResult = {
  jobCount: number
}

export type BotConversationTarget = {
  channelId: string
  userId: string
}

/** Payload emitted from Rust via `bot://message` for history tracking. */
export type BotMessagePayload = {
  channel_id: string
  user_id: string
  /** "inbound" | "outbound" | "outbound_chunk" | "outbound_done" | "error" */
  direction: string
  content: string
  timestamp: number
  agent?: ConversationAgentSnapshot
}
