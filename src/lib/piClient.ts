import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type {
  AgentInput,
  AgentTaskDeliveryRecord,
  AgentTaskListItem,
  AgentTaskUpdateInput,
  AgentRecord,
  AgentWorkspaceBundle,
  AgentWorkspaceFile,
  ChatSessionDetail,
  ChatSessionListItem,
  ChatTurnRow,
  PeerGatewayInfo,
  PeerGatewaySettings,
  ConversationAgentSnapshot,
  ChatAttachmentUpload,
  InstalledSkillItem,
  PersistedChatAttachment,
  PiStreamPayload,
  ProviderRuntimeConfig,
  RuntimeDependencyStatus,
  ScheduledJobRecord,
  ScheduledJobRunRecord,
  SchedulerRuntimeStatus,
  SchedulerServiceStatus,
  SchedulerSyncResult,
  SystemSkillCatalog,
  TokenUsageRecord,
} from '../types'

export type PiStreamUnsubscribe = () => void

export async function streamPiPrompt(
  prompt: string,
  options?: {
    sessionId?: string | null
    providerConfig?: ProviderRuntimeConfig | null
    agentConfig?: ConversationAgentSnapshot | null
    attachments?: PersistedChatAttachment[]
  },
): Promise<void> {
  await invoke('stream_pi_prompt', {
    prompt,
    sessionId: options?.sessionId ?? null,
    providerConfig: options?.providerConfig ?? null,
    agentConfig: options?.agentConfig ?? null,
    attachments: options?.attachments ?? [],
  })
}

export async function listAgentTaskDeliveries(
  sessionIds: string[],
): Promise<AgentTaskDeliveryRecord[]> {
  return invoke<AgentTaskDeliveryRecord[]>('list_agent_task_deliveries', {
    sessionIds,
  })
}

export async function listAgentTasks(agentId?: string | null): Promise<AgentTaskListItem[]> {
  return invoke<AgentTaskListItem[]>('list_agent_tasks', {
    agentId: agentId ?? null,
  })
}

export async function pauseAgentTask(taskId: string): Promise<void> {
  await invoke('pause_agent_task', { taskId })
}

export async function resumeAgentTask(taskId: string): Promise<void> {
  await invoke('resume_agent_task', { taskId })
}

export async function deleteAgentTask(taskId: string): Promise<void> {
  await invoke('delete_agent_task', { taskId })
}

export async function updateAgentTask(taskId: string, payload: AgentTaskUpdateInput): Promise<void> {
  await invoke('update_agent_task', { taskId, payload })
}

export async function runAgentTaskNow(taskId: string): Promise<void> {
  await invoke('run_agent_task_now', { taskId })
}

export async function abortPiStream(sessionId?: string | null): Promise<void> {
  await invoke('abort_pi_stream', { sessionId: sessionId ?? null })
}

export async function clearPiSession(): Promise<void> {
  await invoke('clear_pi_session')
}

export async function clearPiSessionForId(sessionId: string): Promise<void> {
  await invoke('clear_pi_session_for_id', { sessionId })
}

export async function persistChatAttachments(payload: {
  agentId: string
  sessionId?: string | null
  attachments: ChatAttachmentUpload[]
}): Promise<PersistedChatAttachment[]> {
  return invoke<PersistedChatAttachment[]>('persist_chat_attachments', {
    agentId: payload.agentId,
    sessionId: payload.sessionId ?? null,
    attachments: payload.attachments,
  })
}

export async function openLocalFile(filePath: string): Promise<void> {
  await invoke('open_local_file', { filePath })
}

export async function openExternalUrl(url: string): Promise<void> {
  const trimmed = url.trim()
  if (!trimmed) {
    return
  }

  try {
    await invoke('open_external_url', { url: trimmed })
    return
  } catch {
    window.open(trimmed, '_blank', 'noopener,noreferrer')
  }
}

export async function loadLocalMediaPreview(filePath: string, mimeType?: string | null): Promise<string> {
  return invoke<string>('load_local_media_preview', {
    filePath,
    mimeType: mimeType ?? null,
  })
}

export async function testLlmProviderConnection(payload: {
  apiFormat: 'openai' | 'anthropic'
  baseUrl: string
  apiKey: string
  model: string
}): Promise<string> {
  return invoke<string>('test_llm_provider_connection', {
    apiFormat: payload.apiFormat,
    baseUrl: payload.baseUrl,
    apiKey: payload.apiKey,
    model: payload.model,
  })
}

/** 使用智能体「标题生成」模型（未配置则用默认对话模型）根据首轮问答生成会话标题。 */
export async function generateSessionConversationTitle(
  agentId: string,
  userMessage: string,
  assistantMessage: string,
): Promise<string> {
  return invoke<string>('generate_session_conversation_title', {
    agentId,
    userMessage,
    assistantMessage,
  })
}

export async function loadHistoryState(): Promise<string | null> {
  return invoke<string | null>('load_history_state')
}

export async function saveHistoryState(payload: string): Promise<void> {
  await invoke('save_history_state', { payload })
}

export async function clearHistoryState(): Promise<void> {
  await invoke('clear_history_state')
}

// ── Structured Chat History API ──

export async function chatListSessions(): Promise<ChatSessionListItem[]> {
  return invoke<ChatSessionListItem[]>('chat_list_sessions')
}

export async function chatGetSessionDetail(
  sessionId: string,
): Promise<ChatSessionDetail | null> {
  return invoke<ChatSessionDetail | null>('chat_get_session_detail', { sessionId })
}

export async function chatCreateSession(payload: {
  id: string
  title: string
  status: string
  agentId?: string | null
  agentSnapshotJson?: string | null
  botTargetJson?: string | null
  sessionLlmProviderId?: string | null
  sessionLlmModel?: string | null
}): Promise<ChatSessionDetail> {
  return invoke<ChatSessionDetail>('chat_create_session', {
    id: payload.id,
    title: payload.title,
    status: payload.status,
    agentId: payload.agentId ?? null,
    agentSnapshotJson: payload.agentSnapshotJson ?? null,
    botTargetJson: payload.botTargetJson ?? null,
    sessionLlmProviderId: payload.sessionLlmProviderId ?? null,
    sessionLlmModel: payload.sessionLlmModel ?? null,
  })
}

export async function chatAppendTurn(payload: {
  id: string
  sessionId: string
  turnIndex: number
  prompt: string
  answer?: string
  thinking?: string
  status: string
  usageJson?: string | null
  responseSegmentsJson?: string | null
  toolCallsJson?: string | null
  activityJson?: string | null
}): Promise<ChatTurnRow> {
  return invoke<ChatTurnRow>('chat_append_turn', {
    id: payload.id,
    sessionId: payload.sessionId,
    turnIndex: payload.turnIndex,
    prompt: payload.prompt,
    answer: payload.answer ?? '',
    thinking: payload.thinking ?? '',
    status: payload.status,
    usageJson: payload.usageJson ?? null,
    responseSegmentsJson: payload.responseSegmentsJson ?? null,
    toolCallsJson: payload.toolCallsJson ?? null,
    activityJson: payload.activityJson ?? null,
  })
}

export async function chatUpdateTurn(payload: {
  id: string
  answer?: string | null
  thinking?: string | null
  status?: string | null
  completedAt?: number | null
  usageJson?: string | null
  responseSegmentsJson?: string | null
  toolCallsJson?: string | null
  activityJson?: string | null
}): Promise<ChatTurnRow> {
  return invoke<ChatTurnRow>('chat_update_turn', {
    id: payload.id,
    answer: payload.answer ?? null,
    thinking: payload.thinking ?? null,
    status: payload.status ?? null,
    completedAt: payload.completedAt ?? null,
    usageJson: payload.usageJson ?? null,
    responseSegmentsJson: payload.responseSegmentsJson ?? null,
    toolCallsJson: payload.toolCallsJson ?? null,
    activityJson: payload.activityJson ?? null,
  })
}

export async function chatDeleteSession(sessionId: string): Promise<void> {
  await invoke('chat_delete_session', { sessionId })
}

export async function chatClearAllSessions(): Promise<void> {
  await invoke('chat_clear_all_sessions')
}

export async function chatMigrateHistoryV1(): Promise<string> {
  return invoke<string>('chat_migrate_history_v1')
}

export async function listTokenUsageRecords(): Promise<TokenUsageRecord[]> {
  return invoke<TokenUsageRecord[]>('list_token_usage_records')
}

export type ProviderPreferencesPayload = {
  providerConfigs?: string | null
  customProviderMeta?: string | null
}

export async function loadProviderPreferences(): Promise<ProviderPreferencesPayload> {
  return invoke<ProviderPreferencesPayload>('load_provider_preferences')
}

export async function saveProviderPreferences(payload: {
  providerConfigsPayload: string
  customProviderMetaPayload: string
}): Promise<void> {
  await invoke('save_provider_preferences', payload)
}

export async function ensureRuntimeDependencies(): Promise<RuntimeDependencyStatus> {
  return invoke<RuntimeDependencyStatus>('ensure_runtime_dependencies')
}

export async function listInstalledSkills(): Promise<InstalledSkillItem[]> {
  return invoke<InstalledSkillItem[]>('list_installed_skills')
}

export async function listSystemSkillCatalog(): Promise<SystemSkillCatalog> {
  return invoke<SystemSkillCatalog>('list_system_skill_catalog')
}

export async function installSystemSkill(skillId: string): Promise<InstalledSkillItem> {
  return invoke<InstalledSkillItem>('install_system_skill', { skillId })
}

export async function listAgents(): Promise<AgentRecord[]> {
  return invoke<AgentRecord[]>('list_agents')
}

export async function getDefaultAgent(): Promise<AgentRecord | null> {
  return invoke<AgentRecord | null>('get_default_agent')
}

export async function createAgent(payload: AgentInput): Promise<AgentRecord> {
  return invoke<AgentRecord>('create_agent', { payload })
}

export async function updateAgent(agentId: string, payload: AgentInput): Promise<AgentRecord> {
  return invoke<AgentRecord>('update_agent', { agentId, payload })
}

export async function rotateAgentPeerInboundSecret(agentId: string): Promise<AgentRecord> {
  return invoke<AgentRecord>('rotate_agent_peer_inbound_secret', { agentId })
}

export async function getPeerGatewayInfo(): Promise<PeerGatewayInfo> {
  return invoke<PeerGatewayInfo>('get_peer_gateway_info')
}

export async function loadPeerGatewaySettings(): Promise<PeerGatewaySettings> {
  return invoke<PeerGatewaySettings>('load_peer_gateway_settings')
}

export async function savePeerGatewaySettings(
  settings: PeerGatewaySettings,
): Promise<PeerGatewayInfo> {
  return invoke<PeerGatewayInfo>('save_peer_gateway_settings', { settings })
}

export async function archiveAgent(agentId: string): Promise<void> {
  await invoke('archive_agent', { agentId })
}

export async function deleteAgent(agentId: string): Promise<void> {
  await invoke('delete_agent', { agentId })
}

export async function setDefaultAgent(agentId: string): Promise<AgentRecord | null> {
  return invoke<AgentRecord | null>('set_default_agent', { agentId })
}

export async function readAgentWorkspaceBundle(agentId: string): Promise<AgentWorkspaceBundle> {
  return invoke<AgentWorkspaceBundle>('read_agent_workspace_bundle', { agentId })
}

export async function readAgentWorkspaceFile(payload: {
  agentId: string
  relativePath: string
}): Promise<AgentWorkspaceFile> {
  return invoke<AgentWorkspaceFile>('read_agent_workspace_file', payload)
}

export async function writeAgentWorkspaceFile(payload: {
  agentId: string
  relativePath: string
  content: string
}): Promise<AgentWorkspaceBundle> {
  return invoke<AgentWorkspaceBundle>('write_agent_workspace_file', payload)
}

export async function listScheduledJobs(): Promise<ScheduledJobRecord[]> {
  return invoke<ScheduledJobRecord[]>('list_scheduled_jobs')
}

export async function listScheduledJobRuns(limit?: number): Promise<ScheduledJobRunRecord[]> {
  return invoke<ScheduledJobRunRecord[]>('list_scheduled_job_runs', {
    limit: limit ?? null,
  })
}

export async function syncSchedulerJobs(): Promise<SchedulerSyncResult> {
  return invoke<SchedulerSyncResult>('sync_scheduler_jobs')
}

export async function triggerSchedulerJobNow(jobId: string): Promise<void> {
  await invoke('trigger_scheduler_job_now', { jobId })
}

export async function getSchedulerStatus(): Promise<SchedulerRuntimeStatus> {
  return invoke<SchedulerRuntimeStatus>('get_scheduler_status')
}

export async function installSchedulerService(): Promise<SchedulerServiceStatus> {
  return invoke<SchedulerServiceStatus>('install_scheduler_service')
}

export async function uninstallSchedulerService(): Promise<SchedulerServiceStatus> {
  return invoke<SchedulerServiceStatus>('uninstall_scheduler_service')
}

export async function subscribePiStream(
  onMessage: (payload: PiStreamPayload) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<PiStreamPayload>('pi://stream', (event) => {
    onMessage(event.payload)
  })
}

// ── Bot Channel Commands ──

export type QrCodeEvent = {
  channelId: string
  qrcodeUrl?: string
  status: 'waiting' | 'scanned' | 'confirmed' | 'refreshed'
}

export type WechatLoginResult = {
  connected: boolean
  bot_token?: string
  account_id?: string
  base_url?: string
  user_id?: string
  message: string
}

export type BotStatus = 'Disconnected' | 'Connecting' | 'Connected' | 'Error'

export async function botLoginWechat(channelId: string): Promise<WechatLoginResult> {
  return invoke<WechatLoginResult>('bot_login_wechat', { channelId })
}

export async function botStartWechat(
  channelId: string,
  agentId: string,
  token: string,
  options?: {
    baseUrl?: string
    routeTag?: string
    providerId?: string
    providerApiFormat?: 'openai' | 'anthropic'
    model?: string
    apiKey?: string
    providerBaseUrl?: string
  },
): Promise<void> {
  await invoke('bot_start_wechat', {
    channelId,
    agentId,
    token,
    baseUrl: options?.baseUrl ?? null,
    routeTag: options?.routeTag ?? null,
    providerId: options?.providerId ?? null,
    providerApiFormat: options?.providerApiFormat ?? null,
    model: options?.model ?? null,
    apiKey: options?.apiKey ?? null,
    providerBaseUrl: options?.providerBaseUrl ?? null,
  })
}

export async function botStartLark(
  channelId: string,
  agentId: string,
  appId: string,
  appSecret: string,
  options?: {
    providerId?: string
    providerApiFormat?: 'openai' | 'anthropic'
    model?: string
    apiKey?: string
    providerBaseUrl?: string
  },
): Promise<void> {
  await invoke('bot_start_lark', {
    channelId,
    agentId,
    appId,
    appSecret,
    providerId: options?.providerId ?? null,
    providerApiFormat: options?.providerApiFormat ?? null,
    model: options?.model ?? null,
    apiKey: options?.apiKey ?? null,
    providerBaseUrl: options?.providerBaseUrl ?? null,
  })
}

export async function botStopWechat(channelId: string): Promise<void> {
  await invoke('bot_stop_wechat', { channelId })
}

export async function botStopLark(channelId: string): Promise<void> {
  await invoke('bot_stop_lark', { channelId })
}

export async function botGetStatus(channelId: string): Promise<string> {
  return invoke<string>('bot_get_status', { channelId })
}

export async function botSendMessage(channelId: string, userId: string, content: string): Promise<void> {
  await invoke('bot_send_message', { channelId, userId, content })
}

export async function subscribeQrCode(
  onEvent: (payload: QrCodeEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<QrCodeEvent>('bot://qr-code', (event) => {
    onEvent(event.payload)
  })
}

// ── Bot Message Events (history integration) ──

export type BotMessageEvent = {
  channel_id: string
  user_id: string
  direction: string
  content: string
  timestamp: number
  agent?: ConversationAgentSnapshot
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
}

export async function subscribeBotMessage(
  onMessage: (payload: BotMessageEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<BotMessageEvent>('bot://message', (event) => {
    onMessage(event.payload)
  })
}

export type BotStatusEvent = {
  channelId: string
  userId: string
  level: 'processing' | 'done' | 'warn' | 'error'
  message: string
  timestamp: number
}

export async function subscribeBotStatus(
  onEvent: (payload: BotStatusEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<BotStatusEvent>('bot://status', (event) => {
    onEvent(event.payload)
  })
}

export async function botSendMedia(
  channelId: string,
  userId: string,
  mediaType: 'image' | 'file' | 'video',
  filePath: string,
): Promise<void> {
  await invoke('bot_send_media', {
    channelId,
    userId,
    mediaType,
    filePath,
  })
}

// ── Context Window Guard ──

export type SessionContextStatsPayload = {
  sessionId: string
  usedTokens: number
  contextWindow: number | null
  inputTokens: number
  outputTokens: number
  model: string | null
  source: string
}

export async function getSessionContextStats(
  sessionId: string,
): Promise<SessionContextStatsPayload> {
  return invoke<SessionContextStatsPayload>('get_session_context_stats', { sessionId })
}
