import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type {
  AgentInput,
  AgentRecord,
  AgentWorkspaceBundle,
  ConversationAgentSnapshot,
  ChatAttachmentUpload,
  InstalledSkillItem,
  PersistedChatAttachment,
  PiStreamPayload,
  ProviderRuntimeConfig,
  RuntimeDependencyStatus,
  SystemSkillCatalog,
} from '../types'

export type PiStreamUnsubscribe = () => void

export async function streamPiPrompt(
  prompt: string,
  options?: {
    sessionId?: string | null
    providerConfig?: ProviderRuntimeConfig | null
    agentConfig?: ConversationAgentSnapshot | null
  },
): Promise<void> {
  await invoke('stream_pi_prompt', {
    prompt,
    sessionId: options?.sessionId ?? null,
    providerConfig: options?.providerConfig ?? null,
    agentConfig: options?.agentConfig ?? null,
  })
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

export async function loadHistoryState(): Promise<string | null> {
  return invoke<string | null>('load_history_state')
}

export async function saveHistoryState(payload: string): Promise<void> {
  await invoke('save_history_state', { payload })
}

export async function clearHistoryState(): Promise<void> {
  await invoke('clear_history_state')
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

export async function writeAgentWorkspaceFile(payload: {
  agentId: string
  relativePath: string
  content: string
}): Promise<AgentWorkspaceBundle> {
  return invoke<AgentWorkspaceBundle>('write_agent_workspace_file', payload)
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
