import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type {
  AgentImportResult,
  AgentInput,
  AgentTaskDeliveryRecord,
  AgentTaskListItem,
  AgentTaskUpdateInput,
  AgentRecord,
  AgentPresetSummary,
  AgentWorkspaceBundle,
  AgentWorkspaceFile,
  ChatSessionDetail,
  ChatSessionListItem,
  ChatTurnRow,
  EmbeddingProviderStatus,
  EmbeddingReindexResult,
  EmbeddingSettings,
  PeerGatewayInfo,
  PeerGatewaySettings,
  ConversationAgentSnapshot,
  ChatAttachmentUpload,
  InstalledSkillItem,
  NetworkProxySettings,
  PersistedChatAttachment,
  PiStreamPayload,
  ProviderRuntimeConfig,
  RuntimeParameters,
  RuntimeDependencyStatus,
  ScheduledJobRecord,
  ScheduledJobRunRecord,
  SchedulerRuntimeStatus,
  SchedulerServiceStatus,
  SchedulerSyncResult,
  SystemSkillCatalog,
  TokenUsageRecord,
  WorkspaceMemoryRecord,
  WorkspaceMemberView,
  ArtifactsTreeEntry,
  WorkspaceRecord,
  WorkspaceResourceRecord,
} from '../types'
import type { AskUserAnswerDraft } from '../widgetTypes'

export type PiStreamUnsubscribe = () => void

export async function streamPiPrompt(
  prompt: string,
  options?: {
    sessionId?: string | null
    providerConfig?: ProviderRuntimeConfig | null
    agentConfig?: ConversationAgentSnapshot | null
    attachments?: PersistedChatAttachment[]
    workspaceId?: string | null
    overrideAgentId?: string | null
    /** 与设置 → 参数一致；不传则服务端使用上次缓存或默认值 */
    runtimeParameters?: RuntimeParameters | null
  },
): Promise<void> {
  await invoke('stream_pi_prompt', {
    prompt,
    sessionId: options?.sessionId ?? null,
    providerConfig: options?.providerConfig ?? null,
    agentConfig: options?.agentConfig ?? null,
    attachments: options?.attachments ?? [],
    workspaceId: options?.workspaceId ?? null,
    overrideAgentId: options?.overrideAgentId ?? null,
    runtimeParameters: options?.runtimeParameters ?? null,
  })
}

/** 写入全局运行时参数缓存（与其它入口共享 `merge_from_payload` 逻辑）。 */
export async function syncRuntimeParameters(payload: RuntimeParameters): Promise<RuntimeParameters> {
  return invoke<RuntimeParameters>('sync_runtime_parameters', { payload })
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

export type DesktopCompressionCommandResult = {
  compressed: boolean
  reason: string
}

export async function compactDesktopSessionBeforeModelSwitch(payload: {
  sessionId: string
  workspaceId?: string | null
  currentModel: string
  nextModel: string
}): Promise<DesktopCompressionCommandResult> {
  return invoke<DesktopCompressionCommandResult>('compact_desktop_session_before_model_switch', {
    sessionId: payload.sessionId,
    workspaceId: payload.workspaceId ?? null,
    currentModel: payload.currentModel,
    nextModel: payload.nextModel,
  })
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
  /** 团队会话：附件写入该工作空间的「项目成果」根目录 */
  workspaceId?: string | null
  attachments: ChatAttachmentUpload[]
}): Promise<PersistedChatAttachment[]> {
  return invoke<PersistedChatAttachment[]>('persist_chat_attachments', {
    agentId: payload.agentId,
    sessionId: payload.sessionId ?? null,
    workspaceId: payload.workspaceId ?? null,
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

export async function loadNetworkProxySettings(): Promise<NetworkProxySettings> {
  return invoke<NetworkProxySettings>('load_network_proxy_settings')
}

export async function saveNetworkProxySettings(
  settings: NetworkProxySettings,
): Promise<NetworkProxySettings> {
  return invoke<NetworkProxySettings>('save_network_proxy_settings', {
    settings,
  })
}

export async function testNetworkProxyConnection(
  settings: NetworkProxySettings,
): Promise<string> {
  return invoke<string>('test_network_proxy_connection', {
    settings,
  })
}

export async function loadEmbeddingSettings(): Promise<EmbeddingSettings> {
  return invoke<EmbeddingSettings>('load_embedding_settings_command')
}

export async function saveEmbeddingSettings(
  settings: EmbeddingSettings,
): Promise<EmbeddingProviderStatus> {
  return invoke<EmbeddingProviderStatus>('save_embedding_settings_command', { settings })
}

export async function getEmbeddingStatus(): Promise<EmbeddingProviderStatus> {
  return invoke<EmbeddingProviderStatus>('embedding_status_command')
}

export async function triggerEmbeddingReindex(): Promise<EmbeddingReindexResult> {
  return invoke<EmbeddingReindexResult>('trigger_embedding_reindex_command')
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
  workspaceId?: string | null
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
    workspaceId: payload.workspaceId ?? null,
  })
}

export async function chatUpdateSessionTitle(payload: {
  sessionId: string
  title: string
}): Promise<ChatSessionDetail> {
  return invoke<ChatSessionDetail>('chat_update_session_title', {
    sessionId: payload.sessionId,
    title: payload.title,
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
  speakerAgentId?: string | null
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
    speakerAgentId: payload.speakerAgentId ?? null,
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

export async function syncHistoryBackupFromStructured(): Promise<void> {
  await invoke('sync_history_backup_from_structured')
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

export async function workspaceList(includeArchived?: boolean): Promise<WorkspaceRecord[]> {
  return invoke<WorkspaceRecord[]>('workspace_list', { includeArchived: includeArchived ?? false })
}

export async function workspaceCreate(payload: {
  name: string
  description: string
  supervisorAgentId: string
}): Promise<WorkspaceRecord> {
  return invoke<WorkspaceRecord>('workspace_create', payload)
}

export async function workspaceUpdate(
  workspaceId: string,
  payload: {
    name?: string | null
    description?: string | null
    artifactsRoot?: string | null
    supervisorOrchestrationPrompt?: string | null
    llmTraceEnabled?: boolean | null
  },
): Promise<WorkspaceRecord> {
  return invoke<WorkspaceRecord>('workspace_update', {
    workspaceId,
    name: payload.name ?? null,
    description: payload.description ?? null,
    artifactsRoot: payload.artifactsRoot === undefined ? null : payload.artifactsRoot,
    supervisorOrchestrationPrompt:
      payload.supervisorOrchestrationPrompt === undefined ? null : payload.supervisorOrchestrationPrompt,
    llmTraceEnabled: payload.llmTraceEnabled === undefined ? null : payload.llmTraceEnabled,
  })
}

export async function workspaceDefaultSupervisorOrchestrationPrompt(workspaceId: string): Promise<string> {
  return invoke<string>('workspace_default_supervisor_orchestration_prompt', { workspaceId })
}

export type LlmTraceSystemPromptSection = { label: string; content: string }

export type LlmTraceToolCall = {
  toolCallId: string
  toolName: string
  argsJson: string
  resultText: string
  status: string
  isError?: boolean | null
  startedAt: number
  finishedAt?: number | null
}

export type LlmTraceUsage = {
  inputTokens?: number | null
  outputTokens?: number | null
  cacheReadTokens?: number | null
  cacheWriteTokens?: number | null
  totalTokens?: number | null
}

export type LlmTraceEntry = {
  id: string
  workspaceId: string
  kind: 'main_pi' | 'delegate' | string
  callerAgentId: string
  callerAgentName: string
  targetAgentId?: string | null
  targetAgentName?: string | null
  sessionId?: string | null
  parentTraceId?: string | null
  provider?: string | null
  model?: string | null
  responseId?: string | null
  status: 'running' | 'done' | 'error' | 'aborted' | string
  error?: string | null
  startedAt: number
  finishedAt?: number | null
  durationMs?: number | null
  usage?: LlmTraceUsage | null
  systemPrompts: LlmTraceSystemPromptSection[]
  userMessage: string
  responseText: string
  thinkingText: string
  toolCalls: LlmTraceToolCall[]
}

export async function workspaceLlmTraceStatus(workspaceId: string): Promise<boolean> {
  return invoke<boolean>('workspace_llm_trace_status', { workspaceId })
}

export async function workspaceLlmTraceSetEnabled(
  workspaceId: string,
  enabled: boolean,
): Promise<WorkspaceRecord> {
  return invoke<WorkspaceRecord>('workspace_llm_trace_set_enabled', {
    workspaceId,
    enabled,
  })
}

export async function workspaceLlmTraceList(
  workspaceId: string,
  options?: { days?: number; limit?: number },
): Promise<LlmTraceEntry[]> {
  return invoke<LlmTraceEntry[]>('workspace_llm_trace_list', {
    workspaceId,
    days: options?.days ?? null,
    limit: options?.limit ?? null,
  })
}

export async function workspaceLlmTraceClear(workspaceId: string): Promise<void> {
  await invoke('workspace_llm_trace_clear', { workspaceId })
}

export type LlmTraceEvent = {
  phase: 'started' | 'updated' | 'finalized'
  entry: LlmTraceEntry
}

export async function onLlmTraceEvent(
  handler: (payload: LlmTraceEvent) => void,
): Promise<PiStreamUnsubscribe> {
  const un = await listen<LlmTraceEvent>('workspace.llm_trace', (event) => {
    handler(event.payload)
  })
  return () => un()
}

export async function workspaceResolveArtifactsRoot(workspaceId: string): Promise<string> {
  return invoke<string>('workspace_resolve_artifacts_root', { workspaceId })
}

export async function workspaceListArtifactsEntries(
  workspaceId: string,
  subPath?: string | null,
): Promise<ArtifactsTreeEntry[]> {
  return invoke<ArtifactsTreeEntry[]>('workspace_list_artifacts_entries', {
    workspaceId,
    subPath: subPath ?? null,
  })
}

export async function workspaceReadArtifactText(workspaceId: string, relPath: string): Promise<string> {
  return invoke<string>('workspace_read_artifact_text', { workspaceId, relPath })
}

export async function workspaceArtifactAbsolutePath(workspaceId: string, relPath: string): Promise<string> {
  return invoke<string>('workspace_artifact_absolute_path', { workspaceId, relPath })
}

export async function workspaceSetArchived(workspaceId: string, archived: boolean): Promise<void> {
  await invoke('workspace_set_archived', { workspaceId, archived })
}

export async function workspaceAddMember(
  workspaceId: string,
  agentId: string,
  role?: string | null,
): Promise<void> {
  await invoke('workspace_add_member', { workspaceId, agentId, role: role ?? null })
}

export async function workspaceRemoveMember(workspaceId: string, agentId: string): Promise<void> {
  await invoke('workspace_remove_member', { workspaceId, agentId })
}

export async function workspaceListMembers(workspaceId: string): Promise<WorkspaceMemberView[]> {
  return invoke<WorkspaceMemberView[]>('workspace_list_members', { workspaceId })
}

export async function workspaceListResources(workspaceId: string): Promise<WorkspaceResourceRecord[]> {
  return invoke<WorkspaceResourceRecord[]>('workspace_list_resources', { workspaceId })
}

export async function workspaceUploadResource(payload: {
  workspaceId: string
  fileName: string
  dataBase64: string
  mime?: string | null
  uploaderAgentId?: string | null
}): Promise<WorkspaceResourceRecord> {
  return invoke<WorkspaceResourceRecord>('workspace_upload_resource', payload)
}

export async function workspaceReadResourceText(workspaceId: string, relPath: string): Promise<string> {
  return invoke<string>('workspace_read_resource_text', { workspaceId, relPath })
}

export async function workspaceResourceAbsolutePath(workspaceId: string, relPath: string): Promise<string> {
  return invoke<string>('workspace_resource_absolute_path', { workspaceId, relPath })
}

export async function workspaceDeleteResource(workspaceId: string, resourceId: string): Promise<void> {
  await invoke('workspace_delete_resource', { workspaceId, resourceId })
}

export async function workspaceListMemories(
  workspaceId: string,
  limit?: number | null,
): Promise<WorkspaceMemoryRecord[]> {
  return invoke<WorkspaceMemoryRecord[]>('workspace_list_memories', {
    workspaceId,
    limit: limit ?? null,
  })
}

export async function workspaceWriteMemory(payload: {
  workspaceId: string
  title: string
  content: string
  authorAgentId?: string | null
  tags?: string[] | null
}): Promise<WorkspaceMemoryRecord> {
  return invoke<WorkspaceMemoryRecord>('workspace_write_memory', payload)
}

export async function workspaceDeleteMemory(workspaceId: string, memoryId: string): Promise<void> {
  await invoke('workspace_delete_memory', { workspaceId, memoryId })
}

export async function workspaceDelegate(payload: {
  workspaceId: string
  targetAgentId: string
  task: string
  providerConfig: ProviderRuntimeConfig
}): Promise<string> {
  return invoke<string>('workspace_delegate', payload)
}

/** 从委派计划卡「下发一项」——后端会发 workspace:delegate:* 事件并返回最终结果。 */
export async function workspaceRunDelegateTask(payload: {
  workspaceId: string
  sessionId?: string | null
  assignee: string
  task: string
  providerConfig: ProviderRuntimeConfig
}): Promise<{ runId: string; output: string; elapsedMs: number; status: 'done' | 'error' }> {
  return invoke('workspace_run_delegate_task', {
    workspaceId: payload.workspaceId,
    sessionId: payload.sessionId ?? null,
    assignee: payload.assignee,
    task: payload.task,
    providerConfig: payload.providerConfig,
  })
}

export async function workspaceAbortDelegate(runId: string): Promise<void> {
  await invoke('workspace_abort_delegate', { runId })
}

// ── Workspace delegate live events ──
// 后端在 `run_delegate_with_provider_events` 中按 runId 发射下列事件：
//   - workspace:delegate:progress  (派发开始) { runId, workspaceId, sessionId?, assignee, task }
//   - workspace:delegate:turn      { runId, workspaceId, turnIndex, kind }
//   - workspace:delegate:tool      { runId, workspaceId, toolIndex, toolCallId, toolName, argsDigest, status, isError? }
//   - workspace:delegate:chunk     { runId, workspaceId, deltaText }
//   - workspace:delegate:done      { runId, workspaceId, assignee, output, elapsedMs, status: 'done' }
//   - workspace:delegate:error     { runId, workspaceId, assignee, error, status: 'error' }

export type WorkspaceDelegateTurnEvent = {
  runId: string
  workspaceId?: string
  turnIndex: number
  kind: 'thinking' | 'agent'
  summary?: string
}

export type WorkspaceDelegateToolEvent = {
  runId: string
  workspaceId?: string
  toolIndex: number
  toolCallId?: string
  toolName: string
  argsDigest?: string
  status: 'running' | 'done' | 'error'
  isError?: boolean
}

export type WorkspaceDelegateChunkEvent = {
  runId: string
  workspaceId?: string
  deltaText: string
}

export type WorkspaceDelegateTerminalEvent = {
  runId: string
  workspaceId?: string
  assignee?: string
  output?: string
  error?: string
  elapsedMs?: number
  status: 'done' | 'error' | 'aborted'
}

export async function subscribeWorkspaceDelegateTurn(
  onEvent: (payload: WorkspaceDelegateTurnEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<WorkspaceDelegateTurnEvent>('workspace:delegate:turn', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeWorkspaceDelegateTool(
  onEvent: (payload: WorkspaceDelegateToolEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<WorkspaceDelegateToolEvent>('workspace:delegate:tool', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeWorkspaceDelegateChunk(
  onEvent: (payload: WorkspaceDelegateChunkEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<WorkspaceDelegateChunkEvent>('workspace:delegate:chunk', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeWorkspaceDelegateTerminal(
  onEvent: (payload: WorkspaceDelegateTerminalEvent) => void,
): Promise<PiStreamUnsubscribe> {
  const unsubs: PiStreamUnsubscribe[] = []
  unsubs.push(
    await listen<WorkspaceDelegateTerminalEvent>('workspace:delegate:done', (event) => {
      onEvent({ ...event.payload, status: 'done' })
    }),
  )
  unsubs.push(
    await listen<WorkspaceDelegateTerminalEvent>('workspace:delegate:error', (event) => {
      onEvent({ ...event.payload, status: 'error' })
    }),
  )
  return () => {
    for (const un of unsubs) un()
  }
}

export async function workspaceAugmentDelegate(payload: {
  workspaceId: string
  runId: string
  note: string
}): Promise<void> {
  await invoke('workspace_augment_delegate', payload)
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

export async function migrateAgentId(oldAgentId: string, newAgentId: string): Promise<string> {
  return invoke<string>('migrate_agent_id', { oldAgentId, newAgentId })
}

export async function setDefaultAgent(agentId: string): Promise<AgentRecord | null> {
  return invoke<AgentRecord | null>('set_default_agent', { agentId })
}

export async function listDefaultAgentPresets(): Promise<AgentPresetSummary[]> {
  return invoke<AgentPresetSummary[]>('list_default_agent_presets')
}

export async function resetAgentToDefaultPreset(agentId: string): Promise<AgentRecord> {
  return invoke<AgentRecord>('reset_agent_to_default_preset', { agentId })
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

export async function exportAgentPackage(payload: {
  agentId: string
  destPath: string
  includeSecrets: boolean
  includeSharedRoot: boolean
}): Promise<void> {
  await invoke('export_agent_package', {
    agentId: payload.agentId,
    destPath: payload.destPath,
    includeSecrets: payload.includeSecrets,
    includeSharedRoot: payload.includeSharedRoot,
  })
}

export async function importAgentPackage(packagePath: string): Promise<AgentImportResult> {
  return invoke<AgentImportResult>('import_agent_package', { packagePath })
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

export async function widgetSubmitResponse(payload: {
  widgetId: string
  kind: 'ask_user'
  answers: AskUserAnswerDraft[]
}): Promise<void> {
  await invoke('widget_submit_response', { payload })
}

export async function widgetCancelResponse(payload: {
  widgetId: string
  kind: 'ask_user'
}): Promise<void> {
  await invoke('widget_cancel_response', { payload })
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

// ── Agent Loop Guard Approval ──

export type ApprovalRequest = {
  loopId: string
  iteration: number
  action: {
    type: 'call' | 'batch'
    agentId: string
    task: string
    riskLevel: string
    reason: string
  }
  timeoutMs: number
}

export async function agentLoopRespondApproval(
  loopId: string,
  approved: boolean,
): Promise<void> {
  await invoke('agent_loop_respond_approval', { loopId, approved })
}

// ── Agent Loop ─────────────────────────────────────────────

export async function agentLoopRespondReview(
  loopId: string,
  approved: boolean,
  extendTo?: number,
): Promise<void> {
  await invoke('agent_loop_respond_review', {
    loopId,
    approved,
    extendTo: extendTo ?? null,
  })
}

export async function agentLoopAbort(loopId: string): Promise<void> {
  await invoke('agent_loop_abort', { loopId })
}

// ── Agent Loop Event Types ──

export type AgentLoopStartedEvent = {
  loopId: string
  maxIterations: number
  depth: number
}

export type AgentLoopIterationStartEvent = {
  loopId: string
  iteration: number
  type: 'call' | 'batch'
  agentId?: string
  task?: string
  agentCount?: number
}

export type AgentLoopIterationEndEvent = {
  loopId: string
  iteration: number
  status: string
  agentId?: string
  type?: string
  resultsCount?: number
  durationMs?: number
}

export type AgentLoopReviewRequestEvent = {
  loopId: string
  iteration: number
  reviewType: 'pause_for_review' | 'extend'
  info: Record<string, unknown>
}

export type AgentLoopCompletedEvent = {
  loopId: string
  reason: string
  totalIterations: number
  durationMs: number
}

export type AgentLoopAbortedEvent = {
  loopId: string
  iterationsCompleted: number
}

export type AgentLoopErrorEvent = {
  loopId: string
  error: string
}

// ── Agent Loop Event Subscriptions ──

export async function subscribeAgentLoopStarted(
  onEvent: (payload: AgentLoopStartedEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopStartedEvent>('agent-loop://started', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopIterationStart(
  onEvent: (payload: AgentLoopIterationStartEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopIterationStartEvent>('agent-loop://iteration/start', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopIterationEnd(
  onEvent: (payload: AgentLoopIterationEndEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopIterationEndEvent>('agent-loop://iteration/end', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopReviewRequest(
  onEvent: (payload: AgentLoopReviewRequestEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopReviewRequestEvent>('agent-loop://review/request', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopCompleted(
  onEvent: (payload: AgentLoopCompletedEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopCompletedEvent>('agent-loop://completed', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopAborted(
  onEvent: (payload: AgentLoopAbortedEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopAbortedEvent>('agent-loop://aborted', (event) => {
    onEvent(event.payload)
  })
}

export async function subscribeAgentLoopError(
  onEvent: (payload: AgentLoopErrorEvent) => void,
): Promise<PiStreamUnsubscribe> {
  return listen<AgentLoopErrorEvent>('agent-loop://error', (event) => {
    onEvent(event.payload)
  })
}
