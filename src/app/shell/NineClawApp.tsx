import { startTransition, useCallback, useDeferredValue, useEffect, useMemo, useRef, useState } from 'react'
import type { MouseEvent } from 'react'
import { botDefinitions, createInitialBotConfigs, emptyProviderConfig, providerDefinitions, resourceSeed } from '../../mockData'
import { useComposerAttachments } from '../../hooks/useComposerAttachments'
import { usePiAgent } from '../../hooks/usePiAgent'
import { useToast } from '../../hooks/useToast'
import type {
  AgentBuilderDraft,
  AgentInput,
  AgentRecord,
  AgentWorkspaceBundle,
  AgentWorkspaceFile,
  AppearanceSettings,
  BotChannelId,
  BotConfig,
  ConversationAgentSnapshot,
  GeneralSettings,
  HistoryItem,
  ProviderConfig,
  ProviderDefinition,
  ProviderId,
  CustomProviderMeta,
  ProviderApiFormat,
  SettingsTab,
  InstalledSkillItem,
  SkillLibraryTab,
  SystemSkillCatalog,
  ViewKey,
} from '../../types'
import {
  deleteAgent,
  botSendMedia,
  botSendMessage,
  botLoginWechat,
  botStartLark,
  botStartWechat,
  botStopLark,
  botStopWechat,
  createAgent,
  getDefaultAgent,
  installSystemSkill,
  listInstalledSkills,
  listAgents,
  loadProviderPreferences,
  listSystemSkillCatalog,
  readAgentWorkspaceBundle,
  readAgentWorkspaceFile,
  rotateAgentPeerInboundSecret,
  saveProviderPreferences,
  setDefaultAgent,
  subscribeQrCode,
  subscribeBotStatus,
  updateAgent,
  workspaceListMembers,
  workspaceRunDelegateTask,
  writeAgentWorkspaceFile,
} from '../../lib/piClient'
import type { QrCodeEvent, BotStatusEvent } from '../../lib/piClient'
import { buildPromptWithAttachments } from '../../lib/composerAttachments'
import { THEME_PRESETS, THEME_VARIABLE_KEYS } from '../../theme/themePresets'
import {
  APPEARANCE_SETTINGS_STORAGE_KEY,
  CUSTOM_PROVIDERS_META_KEY,
  GENERAL_SETTINGS_STORAGE_KEY,
  LEGACY_APPEARANCE_SETTINGS_STORAGE_KEYS,
  LEGACY_CUSTOM_PROVIDERS_META_KEYS,
  LEGACY_GENERAL_SETTINGS_STORAGE_KEYS,
  LEGACY_PROVIDER_CONFIGS_STORAGE_KEYS,
  PROVIDER_CONFIGS_STORAGE_KEY,
  buildBotConfigStatusPatch,
  buildBotRuntimeBindingConfig,
  buildConversationAgentSnapshot,
  buildSessionLlmOptionsWithFallback,
  buildSessionLlmSelectOptions,
  buildSkillInstallPrompt,
  createAgentBotConfigState,
  createAgentDraftFromRecord,
  createEmptyAgentDraft,
  createEmptySystemSkillCatalog,
  createInitialAppearanceState,
  createInitialGeneralSettings,
  createInitialProviderState,
  getBotChannelRuntimeId,
  getProviderStatus,
  hasProviderDefinition,
  isProviderAvailable,
  isProviderConfigComplete,
  isValidConfiguredModelReference,
  loadCustomProviderMeta,
  normalizeAgentDraft,
  normalizeHeartbeatConfig,
  parseStoredCustomProviderMeta,
  parseStoredProviderConfigs,
  persistStoredStorageValue,
  pickDefaultWorkspaceFileKey,
  pickFallbackSessionLlm,
  providerDisplayName,
  resolveActiveProviderConfig,
  resolveBotChannelFromRuntimeId,
  resolveEffectiveChatRuntime,
  resolveRuntimeFromAgentSnapshot,
  resolveRuntimeFromSessionFields,
  sanitizeAgentInputModelReference,
  sessionLlmDecode,
  sessionLlmEncode,
  toBotSendMediaType,
  validateAgentDraft,
  validateSkillInstallLink,
} from '../lib'
import { NineClawAppChrome } from './NineClawAppChrome'
import { NineClawRouteOutlet } from './NineClawRouteOutlet'

export function NineClawApp() {
  const toast = useToast()
  const composerClearRef = useRef<(() => void) | null>(null)
  const composerDraftBackupRef = useRef('')
  const {
    error,
    loading,
    runtimeReady,
    runtimeBlockingReason,
    runningHistoryIds,
    streamingHistoryIds,
    history,
    activeHistoryId,
    activeHistoryItem,
    submitPrompt,
    submitPromptInNewSession,
    abortPrompt,
    resetSessionDraft,
    selectHistoryItem,
    clearHistory,
    deleteHistoryItem,
    updateSessionLlm,
    sanitizeSessionLlmReferences,
  } = usePiAgent(composerClearRef)

  const [view, setView] = useState<ViewKey>('chat')
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [settingsTab, setSettingsTab] = useState<SettingsTab>('general')
  const [sidebarOverlayOpen, setSidebarOverlayOpen] = useState(false)
  const [viewportWidth, setViewportWidth] = useState(() => window.innerWidth)
  const [skillLibraryTab, setSkillLibraryTab] = useState<SkillLibraryTab>('installed')
  const [skillSearch, setSkillSearch] = useState('')
  const [installedSkills, setInstalledSkills] = useState<InstalledSkillItem[]>([])
  const [systemSkillCatalog, setSystemSkillCatalog] = useState<SystemSkillCatalog>(createEmptySystemSkillCatalog)
  const [skillsLoading, setSkillsLoading] = useState(false)
  const [skillsError, setSkillsError] = useState('')
  const [skillInstallDialogOpen, setSkillInstallDialogOpen] = useState(false)
  const [skillInstallLink, setSkillInstallLink] = useState('')
  const [skillInstallError, setSkillInstallError] = useState('')
  const [skillInstallLaunching, setSkillInstallLaunching] = useState(false)
  const [systemSkillInstallId, setSystemSkillInstallId] = useState('')
  const [resourceSearch, setResourceSearch] = useState('')
  const [agentSearch, setAgentSearch] = useState('')
  const [historySearch, setHistorySearch] = useState('')
  const [agents, setAgents] = useState<AgentRecord[]>([])
  const [agentsLoading, setAgentsLoading] = useState(false)
  const [agentsError, setAgentsError] = useState('')
  const [defaultAgentId, setDefaultAgentId] = useState('')
  const [managedAgentId, setManagedAgentId] = useState('')
  const [agentEditorMode, setAgentEditorMode] = useState<'create' | 'edit'>('edit')
  const [agentEditorDraft, setAgentEditorDraft] = useState<AgentInput | null>(null)
  const [agentEditorOpen, setAgentEditorOpen] = useState(false)
  const [agentSaving, setAgentSaving] = useState(false)
  const [agentRefreshing, setAgentRefreshing] = useState(false)
  const [agentFormError, setAgentFormError] = useState('')
  const [agentFormNotice, setAgentFormNotice] = useState('')
  const [agentBotBindingDialogOpen, setAgentBotBindingDialogOpen] = useState(false)
  const [agentDeleteConfirmOpen, setAgentDeleteConfirmOpen] = useState(false)
  const [agentDeleteConfirmText, setAgentDeleteConfirmText] = useState('')
  const [agentBuilderActionBusyId, setAgentBuilderActionBusyId] = useState('')
  const [agentBuilderActionTargetId, setAgentBuilderActionTargetId] = useState('')
  const [agentBuilderActionNotice, setAgentBuilderActionNotice] = useState('')
  const [agentBuilderActionError, setAgentBuilderActionError] = useState('')
  const [agentSkillPickerOpen, setAgentSkillPickerOpen] = useState(false)
  const [agentSkillSearch, setAgentSkillSearch] = useState('')
  const [agentWorkspaceDialogOpen, setAgentWorkspaceDialogOpen] = useState(false)
  const [agentWorkspaceDialogLoading, setAgentWorkspaceDialogLoading] = useState(false)
  const [agentWorkspaceFileLoading, setAgentWorkspaceFileLoading] = useState(false)
  const [agentWorkspaceDialogError, setAgentWorkspaceDialogError] = useState('')
  const [agentWorkspaceBundle, setAgentWorkspaceBundle] = useState<AgentWorkspaceBundle | null>(null)
  const [agentWorkspaceSelectedKey, setAgentWorkspaceSelectedKey] = useState('')
  const [agentWorkspaceDraftContent, setAgentWorkspaceDraftContent] = useState('')
  const [agentWorkspaceSaving, setAgentWorkspaceSaving] = useState(false)
  const [agentWorkspaceSaveError, setAgentWorkspaceSaveError] = useState('')
  const [agentWorkspaceSaveNotice, setAgentWorkspaceSaveNotice] = useState('')
  const [newSessionDialogOpen, setNewSessionDialogOpen] = useState(false)
  const [newSessionAgentId, setNewSessionAgentId] = useState('')
  const [newSessionLlm, setNewSessionLlm] = useState<{ providerId: ProviderId; model: string } | null>(null)
  const [historyContextMenu, setHistoryContextMenu] = useState<{
    sessionId: string
    title: string
    x: number
    y: number
    canDelete: boolean
  } | null>(null)
  const [historyDeleteTarget, setHistoryDeleteTarget] = useState<{ sessionId: string; title: string } | null>(null)
  const [historyDeleteBusy, setHistoryDeleteBusy] = useState(false)
  const [generalSettings, setGeneralSettings] = useState<GeneralSettings>(createInitialGeneralSettings)
  const [appearanceSettings, setAppearanceSettings] = useState<AppearanceSettings>(createInitialAppearanceState)
  const [selectedProviderId, setSelectedProviderId] = useState<ProviderId>('openai')
  const [selectedBotId, setSelectedBotId] = useState<BotChannelId>('dingtalk')
  const [providerConfigs, setProviderConfigs] = useState<Record<string, ProviderConfig>>(createInitialProviderState)
  const [customProviderMeta, setCustomProviderMeta] = useState<CustomProviderMeta[]>(() => loadCustomProviderMeta())
  const [qrDialogOpen, setQrDialogOpen] = useState(false)
  const [qrCodeUrl, setQrCodeUrl] = useState<string>('')
  const [qrStatus, setQrStatus] = useState<'waiting' | 'scanned' | 'confirmed' | 'error'>('waiting')
  const [botLoading, setBotLoading] = useState(false)
  const [botStatusLog, setBotStatusLog] = useState<BotStatusEvent[]>([])
  /** 无选中会话时，输入区上方选择的模型（首条消息写入该会话） */
  const [composerSessionLlm, setComposerSessionLlm] = useState<{ providerId: ProviderId; model: string } | null>(null)
  const [composerAgent, setComposerAgent] = useState<ConversationAgentSnapshot | null>(null)
  const [chatGateError, setChatGateError] = useState('')
  const providerCleanupInFlightRef = useRef<Set<string>>(new Set())
  const providerPreferencesHydratedRef = useRef(false)

  const deferredSkillSearch = useDeferredValue(skillSearch.trim().toLowerCase())
  const deferredResourceSearch = useDeferredValue(resourceSearch.trim().toLowerCase())
  const deferredAgentSearch = useDeferredValue(agentSearch.trim().toLowerCase())
  const deferredHistorySearch = useDeferredValue(historySearch.trim().toLowerCase())

  const mergedProviderDefinitions = useMemo((): ProviderDefinition[] => {
    const custom = customProviderMeta.map((meta) => {
      const effectiveApiFormat = providerConfigs[meta.id]?.apiFormat ?? meta.apiFormat
      return {
        id: meta.id,
        name: meta.name,
        defaultBaseUrl: effectiveApiFormat === 'anthropic' ? 'https://api.anthropic.com' : 'https://api.openai.com/v1',
        suggestedModel: effectiveApiFormat === 'anthropic' ? 'claude-sonnet-4-0' : 'gpt-4o-mini',
        description: meta.description || `自定义 ${effectiveApiFormat === 'anthropic' ? 'Anthropic' : 'OpenAI'} 兼容接口。`,
        apiFormat: effectiveApiFormat,
        isCustom: true,
      }
    })
    return [...providerDefinitions, ...custom]
  }, [customProviderMeta, providerConfigs])
  const allProviderIds = useMemo(() => mergedProviderDefinitions.map((p) => p.id), [mergedProviderDefinitions])
  const firstAddedProviderId = useMemo(
    () => mergedProviderDefinitions.find((item) => providerConfigs[item.id]?.added)?.id ?? '',
    [mergedProviderDefinitions, providerConfigs],
  )
  const selectedProviderDefinition =
    mergedProviderDefinitions.find((item) => item.id === selectedProviderId) ?? providerDefinitions[0]
  const selectedProviderConfig = providerConfigs[selectedProviderId] ?? emptyProviderConfig()
  const activeProviderConfig = resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
  const activeProviderDefinition = activeProviderConfig
    ? mergedProviderDefinitions.find((item) => item.id === activeProviderConfig.providerId) ?? null
    : null
  const selectedBotDefinition = botDefinitions.find((item) => item.id === selectedBotId) ?? botDefinitions[0]
  const defaultAgent = useMemo(
    () => agents.find((item) => item.id === defaultAgentId) ?? agents[0] ?? null,
    [agents, defaultAgentId],
  )
  const editableAgents = useMemo(() => agents.filter((item) => !item.isBuiltin), [agents])
  const selectedManagedAgent = useMemo(
    () => editableAgents.find((item) => item.id === managedAgentId) ?? null,
    [editableAgents, managedAgentId],
  )
  const selectedManagedBotConfigs = useMemo(
    () => createAgentBotConfigState(agentEditorDraft?.botConfigs),
    [agentEditorDraft?.botConfigs],
  )
  const selectedManagedBotConfig = selectedManagedBotConfigs[selectedBotId] ?? createInitialBotConfigs()[selectedBotId]
  const preferredComposerAgent = useMemo(
    () => composerAgent ?? (defaultAgent ? buildConversationAgentSnapshot(defaultAgent) : null),
    [composerAgent, defaultAgent],
  )
  const activeChatAgent = activeHistoryItem ? activeHistoryItem.agent ?? null : preferredComposerAgent
  const composerAttachmentScopeKey = `${activeHistoryId || 'composer'}:${activeChatAgent?.id ?? 'no-agent'}:${activeHistoryItem?.workspaceId ?? ''}`
  const {
    attachments: composerAttachments,
    uploading: composerAttachmentUploading,
    error: composerAttachmentError,
    fileInputRef: composerAttachmentInputRef,
    openFilePicker: openComposerAttachmentPicker,
    handleFileInputChange: handleComposerAttachmentInputChange,
    handleComposerPaste,
    removeAttachment: removeComposerAttachment,
    clearAttachments: clearComposerAttachments,
    clearError: clearComposerAttachmentError,
  } = useComposerAttachments({
    agentId: activeChatAgent?.id ?? '',
    sessionId: activeHistoryId || null,
    workspaceId: activeHistoryItem?.workspaceId ?? null,
    scopeKey: composerAttachmentScopeKey,
  })
  const visibleInstalledSkills = installedSkills.filter((skill) => {
    if (!deferredSkillSearch) return true
    return `${skill.name} ${skill.description} ${skill.path}`.toLowerCase().includes(deferredSkillSearch)
  })
  const visibleSystemSkills = systemSkillCatalog.skills.filter((skill) => {
    if (!deferredSkillSearch) return true
    return `${skill.name} ${skill.description}`.toLowerCase().includes(deferredSkillSearch)
  })
  const visibleResources = resourceSeed.filter((resource) => {
    if (!deferredResourceSearch) return true
    return `${resource.title} ${resource.description} ${resource.tag}`.toLowerCase().includes(deferredResourceSearch)
  })
  const builtinAgentSkillOptions = useMemo<InstalledSkillItem[]>(
    () =>
      systemSkillCatalog.skills.map((skill) => ({
        id: skill.id,
        name: skill.name,
        description: skill.description,
        path: `system://${skill.id}`,
        manifestPath: `system://${skill.id}/SKILL.md`,
        scope: 'global',
        installType: 'directory',
        updatedAt: systemSkillCatalog.updatedAt ?? 0,
      })),
    [systemSkillCatalog.skills, systemSkillCatalog.updatedAt],
  )
  const visibleAgents = editableAgents.filter((agent) => {
    if (!deferredAgentSearch) return true
    return `${agent.name} ${agent.summary} ${agent.description}`.toLowerCase().includes(deferredAgentSearch)
  })
  const visibleAgentSkillOptions = useMemo(() => {
    const searchNeedle = agentSkillSearch.trim().toLowerCase()
    const activeSkillIds = new Set(agentEditorDraft?.skillIds ?? [])
    const mergedSkills = [...installedSkills]
    for (const skill of builtinAgentSkillOptions) {
      if (!mergedSkills.some((item) => item.id === skill.id)) {
        mergedSkills.push(skill)
      }
    }

    return mergedSkills
      .filter((skill) => {
        if (!searchNeedle) {
          return true
        }

        return `${skill.name} ${skill.description} ${skill.path} ${skill.source ?? ''}`
          .toLowerCase()
          .includes(searchNeedle)
      })
      .sort((left, right) => {
        const leftSelected = activeSkillIds.has(left.id) ? 1 : 0
        const rightSelected = activeSkillIds.has(right.id) ? 1 : 0
        if (leftSelected !== rightSelected) {
          return rightSelected - leftSelected
        }
        return left.name.localeCompare(right.name, 'zh-CN')
      })
  }, [agentEditorDraft?.skillIds, agentSkillSearch, builtinAgentSkillOptions, installedSkills])
  const visibleHistory = history.filter((item) => {
    if (!deferredHistorySearch) return true

    const searchableText = [
      item.title,
      ...item.turns.flatMap((turn) => [turn.prompt, turn.answer]),
    ]
      .join(' ')
      .toLowerCase()

    return searchableText.includes(deferredHistorySearch)
  })
  const shouldHideSidebar = viewportWidth < 1180
  const effectiveSidebarCollapsed = !shouldHideSidebar && appearanceSettings.sidebarCollapsed
  const effectiveChatRuntime = useMemo(
    () =>
      resolveEffectiveChatRuntime(
        activeHistoryItem,
        preferredComposerAgent,
        composerSessionLlm,
        selectedProviderId,
        providerConfigs,
        allProviderIds,
      ),
    [
      activeHistoryItem,
      preferredComposerAgent,
      composerSessionLlm,
      selectedProviderId,
      providerConfigs,
      allProviderIds,
    ],
  )

  const sessionLlmDisplay = useMemo(() => {
    const globalRuntime = resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
    const fallbackPid = globalRuntime?.providerId ?? selectedProviderId
    const fallbackModel =
      globalRuntime?.model ?? providerConfigs[selectedProviderId]?.model?.trim() ?? ''

    if (activeHistoryItem?.sessionLlmProviderId && activeHistoryItem.sessionLlmModel !== undefined) {
      const resolved = resolveRuntimeFromSessionFields(
        activeHistoryItem.sessionLlmProviderId,
        activeHistoryItem.sessionLlmModel,
        providerConfigs,
      )
      if (resolved) {
        return {
          providerId: resolved.providerId,
          model: resolved.model,
        }
      }
    }
    if (activeHistoryItem) {
      const resolvedFromAgent = resolveRuntimeFromAgentSnapshot(activeHistoryItem.agent ?? null, providerConfigs)
      if (resolvedFromAgent) {
        return {
          providerId: resolvedFromAgent.providerId,
          model: resolvedFromAgent.model,
        }
      }
      const global = resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
      if (global) {
        return { providerId: global.providerId, model: global.model }
      }
      const firstComplete = mergedProviderDefinitions.find((d) => isProviderConfigComplete(providerConfigs[d.id]))
      if (firstComplete) {
        const cfg = providerConfigs[firstComplete.id]
        return { providerId: firstComplete.id, model: cfg!.model.trim() }
      }
      return { providerId: fallbackPid, model: fallbackModel }
    }
    if (composerSessionLlm) {
      const resolved = resolveRuntimeFromSessionFields(composerSessionLlm.providerId, composerSessionLlm.model, providerConfigs)
      if (resolved) {
        return { providerId: resolved.providerId, model: resolved.model }
      }
    }
    if (preferredComposerAgent?.defaultProviderId && preferredComposerAgent.defaultModel.trim()) {
      const resolved = resolveRuntimeFromAgentSnapshot(preferredComposerAgent, providerConfigs)
      if (resolved) {
        return {
          providerId: resolved.providerId,
          model: resolved.model,
        }
      }
    }
    const globalForComposer = resolveActiveProviderConfig(selectedProviderId, providerConfigs, allProviderIds)
    if (globalForComposer) {
      return { providerId: globalForComposer.providerId, model: globalForComposer.model }
    }
    const firstCompleteComposer = mergedProviderDefinitions.find((d) =>
      isProviderConfigComplete(providerConfigs[d.id]),
    )
    if (firstCompleteComposer) {
      const cfg = providerConfigs[firstCompleteComposer.id]
      return { providerId: firstCompleteComposer.id, model: cfg!.model.trim() }
    }
    return { providerId: fallbackPid, model: fallbackModel }
  }, [
    activeHistoryItem,
    preferredComposerAgent,
    composerSessionLlm,
    selectedProviderId,
    providerConfigs,
    allProviderIds,
    mergedProviderDefinitions,
  ])

  const sessionLlmSelectOptions = useMemo(
    () => buildSessionLlmSelectOptions(mergedProviderDefinitions, providerConfigs),
    [mergedProviderDefinitions, providerConfigs],
  )

  const sessionLlmEncodedCurrent = useMemo(
    () => sessionLlmEncode(sessionLlmDisplay.providerId, sessionLlmDisplay.model.trim()),
    [sessionLlmDisplay],
  )

  const sessionLlmSelectOptionsWithFallback = useMemo(() => {
    return buildSessionLlmOptionsWithFallback(
      sessionLlmSelectOptions,
      mergedProviderDefinitions,
      providerConfigs,
      sessionLlmDisplay,
    )
  }, [
    sessionLlmSelectOptions,
    sessionLlmDisplay,
    mergedProviderDefinitions,
    providerConfigs,
  ])

  const chatProviderDefinition = effectiveChatRuntime
    ? mergedProviderDefinitions.find((item) => item.id === effectiveChatRuntime.providerId) ?? null
    : null
  const requiresConfiguredSessionModel = activeHistoryItem
    ? Boolean(
        (activeHistoryItem.sessionLlmProviderId && activeHistoryItem.sessionLlmModel?.trim()) ||
          (activeHistoryItem.agent?.defaultProviderId && activeHistoryItem.agent.defaultModel?.trim()),
      )
    : Boolean(composerSessionLlm || (preferredComposerAgent?.defaultProviderId && preferredComposerAgent.defaultModel.trim()))
  const runtimeResolutionError =
    !effectiveChatRuntime && requiresConfiguredSessionModel
      ? '当前会话绑定的模型尚未在设置里完成配置，请先补全对应供应商的 Base URL、API Key 和模型，或切换到已配置模型。'
      : !effectiveChatRuntime
      ? '当前没有已启用的 Provider。请在设置中启用至少一个 Provider，或为当前会话/智能体绑定一个已配置模型。'
      : ''
  const chatProviderLabel =
    chatProviderDefinition && effectiveChatRuntime
      ? `${providerDisplayName(
          chatProviderDefinition,
          providerConfigs[effectiveChatRuntime.providerId],
        )} · ${effectiveChatRuntime.model}`
      : 'pi 主流程'

  const activeProviderBadge =
    activeProviderDefinition && activeProviderConfig
      ? `当前使用 · ${providerDisplayName(
          activeProviderDefinition,
          providerConfigs[activeProviderConfig.providerId],
        )}`
      : '当前使用 · pi 默认'

  useEffect(() => {
    if (mergedProviderDefinitions.length === 0) {
      return
    }

    if (
      !isProviderAvailable(selectedProviderId, mergedProviderDefinitions, providerConfigs) &&
      firstAddedProviderId &&
      selectedProviderId !== firstAddedProviderId
    ) {
      setSelectedProviderId(firstAddedProviderId)
      return
    }

    if (!hasProviderDefinition(selectedProviderId, mergedProviderDefinitions)) {
      setSelectedProviderId(firstAddedProviderId || mergedProviderDefinitions[0]?.id || 'openai')
    }

    sanitizeSessionLlmReferences((providerId, model) =>
      isValidConfiguredModelReference(providerId, model, mergedProviderDefinitions, providerConfigs),
    )

    setComposerSessionLlm((current) => {
      if (!current) {
        return current
      }
      if (isValidConfiguredModelReference(current.providerId, current.model, mergedProviderDefinitions, providerConfigs)) {
        return current
      }
      return pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, current.providerId)
    })

    setNewSessionLlm((current) => {
      if (!current) {
        return current
      }
      if (isValidConfiguredModelReference(current.providerId, current.model, mergedProviderDefinitions, providerConfigs)) {
        return current
      }
      return pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, current.providerId)
    })

    setAgentEditorDraft((current) => {
      if (!current) {
        return current
      }
      const next = sanitizeAgentInputModelReference(current, mergedProviderDefinitions, providerConfigs)
      return next.defaultProviderId === current.defaultProviderId && next.defaultModel === current.defaultModel
        ? current
        : next
    })

    setAgents((previous) => {
      let changed = false
      const next = previous.map((agent) => {
        const sanitized = sanitizeAgentInputModelReference(
          createAgentDraftFromRecord(agent),
          mergedProviderDefinitions,
          providerConfigs,
        )
        if (
          sanitized.defaultProviderId === agent.defaultProviderId &&
          sanitized.defaultModel === agent.defaultModel
        ) {
          return agent
        }
        changed = true
        return {
          ...agent,
          defaultProviderId: sanitized.defaultProviderId,
          defaultModel: sanitized.defaultModel,
        }
      })
      return changed ? next : previous
    })

    for (const agent of agents) {
      const sanitized = sanitizeAgentInputModelReference(
        createAgentDraftFromRecord(agent),
        mergedProviderDefinitions,
        providerConfigs,
      )
      if (
        sanitized.defaultProviderId === agent.defaultProviderId &&
        sanitized.defaultModel === agent.defaultModel
      ) {
        continue
      }
      if (providerCleanupInFlightRef.current.has(agent.id)) {
        continue
      }

      providerCleanupInFlightRef.current.add(agent.id)
      void updateAgent(agent.id, sanitized)
        .then((savedAgent) => {
          setAgents((previous) => previous.map((item) => (item.id === savedAgent.id ? savedAgent : item)))
          setAgentEditorDraft((current) => {
            if (!current || managedAgentId !== savedAgent.id) {
              return current
            }
            return createAgentDraftFromRecord(savedAgent)
          })
        })
        .catch((cleanupError) => {
          const message = cleanupError instanceof Error ? cleanupError.message : String(cleanupError)
          setAgentFormError((current) =>
            current || `自动清理已删除 Provider 的智能体模型引用失败：${message}`,
          )
        })
        .finally(() => {
          providerCleanupInFlightRef.current.delete(agent.id)
        })
    }
  }, [
    agents,
    managedAgentId,
    firstAddedProviderId,
    mergedProviderDefinitions,
    providerConfigs,
    sanitizeSessionLlmReferences,
    selectedProviderId,
  ])

  useEffect(() => {
    const onResize = () => {
      const nextWidth = window.innerWidth
      setViewportWidth(nextWidth)
      if (nextWidth >= 1180) {
        setSidebarOverlayOpen(false)
      }
    }

    window.addEventListener('resize', onResize, { passive: true })
    return () => window.removeEventListener('resize', onResize)
  }, [])

  useEffect(() => {
    persistStoredStorageValue(
      GENERAL_SETTINGS_STORAGE_KEY,
      JSON.stringify(generalSettings),
      LEGACY_GENERAL_SETTINGS_STORAGE_KEYS,
    )
  }, [generalSettings])

  useEffect(() => {
    persistStoredStorageValue(
      APPEARANCE_SETTINGS_STORAGE_KEY,
      JSON.stringify(appearanceSettings),
      LEGACY_APPEARANCE_SETTINGS_STORAGE_KEYS,
    )
  }, [appearanceSettings])

  useEffect(() => {
    const root = document.documentElement
    const preset = THEME_PRESETS[appearanceSettings.themeMode] ?? THEME_PRESETS.dark

    root.dataset.uiTheme = appearanceSettings.themeMode
    document.body.dataset.uiTheme = appearanceSettings.themeMode
    root.style.setProperty('color-scheme', preset.colorScheme)

    for (const variableName of THEME_VARIABLE_KEYS) {
      const variableValue = preset.variables[variableName]

      if (variableValue) {
        root.style.setProperty(variableName, variableValue)
      } else {
        root.style.removeProperty(variableName)
      }
    }
  }, [appearanceSettings.themeMode])

  useEffect(() => {
    let cancelled = false

    void loadProviderPreferences()
      .then((payload) => {
        if (cancelled) {
          return
        }

        const backendProviderConfigs = parseStoredProviderConfigs(payload.providerConfigs)
        const backendCustomProviderMeta = parseStoredCustomProviderMeta(payload.customProviderMeta)

        if (backendProviderConfigs) {
          setProviderConfigs(backendProviderConfigs)
        }
        if (backendCustomProviderMeta) {
          setCustomProviderMeta(backendCustomProviderMeta)
        }

        const hasBackendState = Boolean(payload.providerConfigs || payload.customProviderMeta)
        providerPreferencesHydratedRef.current = true

        if (!hasBackendState) {
          void saveProviderPreferences({
            providerConfigsPayload: JSON.stringify(providerConfigs),
            customProviderMetaPayload: JSON.stringify(customProviderMeta),
          }).catch((error) => {
            console.warn('NineClaw: migrate provider preferences failed', error)
          })
        }
      })
      .catch((error) => {
        providerPreferencesHydratedRef.current = true
        console.warn('NineClaw: load provider preferences failed', error)
      })

    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => {
    persistStoredStorageValue(
      PROVIDER_CONFIGS_STORAGE_KEY,
      JSON.stringify(providerConfigs),
      LEGACY_PROVIDER_CONFIGS_STORAGE_KEYS,
    )
    if (!providerPreferencesHydratedRef.current) {
      return
    }
    void saveProviderPreferences({
      providerConfigsPayload: JSON.stringify(providerConfigs),
      customProviderMetaPayload: JSON.stringify(customProviderMeta),
    }).catch((error) => {
      console.warn('NineClaw: save provider preferences failed', error)
    })
  }, [customProviderMeta, providerConfigs])

  useEffect(() => {
    persistStoredStorageValue(
      CUSTOM_PROVIDERS_META_KEY,
      JSON.stringify(customProviderMeta),
      LEGACY_CUSTOM_PROVIDERS_META_KEYS,
    )
  }, [customProviderMeta])

  // ── Bot status log (diagnostics) ──
  useEffect(() => {
    let unsub: (() => void) | undefined
    void subscribeBotStatus((event) => {
      setBotStatusLog((prev) => [event, ...prev].slice(0, 20))
      if (!selectedManagedAgent) {
        return
      }

      const channelId = resolveBotChannelFromRuntimeId(selectedManagedAgent.id, event.channelId)
      const statusPatch = channelId ? buildBotConfigStatusPatch(event) : null
      if (!channelId || !statusPatch) {
        return
      }

      setAgentEditorDraft((current) => {
        if (!current) {
          return current
        }
        const currentConfigs = createAgentBotConfigState(current.botConfigs)
        return {
          ...current,
          botConfigs: {
            ...currentConfigs,
            [channelId]: {
              ...currentConfigs[channelId],
              ...statusPatch,
            },
          },
        }
      })
    }).then((unlisten) => {
      unsub = unlisten
    })
    return () => unsub?.()
  }, [selectedManagedAgent])

  const refreshSkillLibrary = useCallback(async () => {
    setSkillsLoading(true)
    setSkillsError('')

    try {
      const [nextInstalledSkills, nextSystemSkillCatalog] = await Promise.all([
        listInstalledSkills(),
        listSystemSkillCatalog(),
      ])
      setInstalledSkills(nextInstalledSkills)
      setSystemSkillCatalog(nextSystemSkillCatalog)
    } catch (loadError) {
      const message = loadError instanceof Error ? loadError.message : String(loadError)
      setSkillsError(message)
    } finally {
      setSkillsLoading(false)
    }
  }, [])

  useEffect(() => {
    void refreshSkillLibrary()
  }, [refreshSkillLibrary])

  const handleInstallSystemSkill = useCallback(async (skillId: string) => {
    const trimmedId = skillId.trim()
    if (!trimmedId) {
      return
    }

    setSystemSkillInstallId(trimmedId)
    setSkillsError('')
    try {
      await installSystemSkill(trimmedId)
      await refreshSkillLibrary()
    } catch (installError) {
      const message = installError instanceof Error ? installError.message : String(installError)
      setSkillsError(message)
    } finally {
      setSystemSkillInstallId('')
    }
  }, [refreshSkillLibrary])

  const settingsSkillsLibrary = useMemo(
    (): import('../pages/LibraryAndTasks').SkillsViewProps => ({
      variant: 'embedded',
      installedSkillCount: installedSkills.length,
      installedSkills: visibleInstalledSkills,
      onChangeTab: setSkillLibraryTab,
      onInstallByLink: () => {
        setSkillInstallDialogOpen(true)
        setSkillInstallError('')
      },
      onInstallSystemSkill: handleInstallSystemSkill,
      onRefresh: refreshSkillLibrary,
      sessionBusy: skillInstallLaunching,
      setSearch: setSkillSearch,
      skillsError,
      skillsLoading,
      systemSkillCount: systemSkillCatalog.skills.length,
      systemSkillCatalog,
      systemSkillInstallId,
      tab: skillLibraryTab,
      skillSearch,
      visibleSystemSkills,
    }),
    [
      installedSkills.length,
      visibleInstalledSkills,
      handleInstallSystemSkill,
      refreshSkillLibrary,
      skillInstallLaunching,
      skillsError,
      skillsLoading,
      systemSkillCatalog,
      systemSkillCatalog.skills.length,
      systemSkillInstallId,
      skillLibraryTab,
      skillSearch,
      visibleSystemSkills,
    ],
  )

  const settingsResourcesLibrary = useMemo(
    (): import('../pages/LibraryAndTasks').ResourcesViewProps => ({
      variant: 'embedded',
      onSearch: setResourceSearch,
      resourceSearch,
      visibleResources,
    }),
    [resourceSearch, visibleResources],
  )

  const refreshAgents = useCallback(async (preferredAgentId?: string) => {
    setAgentsLoading(true)
    setAgentsError('')

    try {
      const [nextAgents, nextDefaultAgent] = await Promise.all([listAgents(), getDefaultAgent()])
      const nextEditableAgents = nextAgents.filter((item) => !item.isBuiltin)
      setAgents(nextAgents)
      setDefaultAgentId(nextDefaultAgent?.id ?? '')

      let nextManagedId = preferredAgentId ?? ''
      setManagedAgentId((current) => {
        nextManagedId =
          (preferredAgentId &&
            nextEditableAgents.some((item) => item.id === preferredAgentId) &&
            preferredAgentId) ||
          (current && nextEditableAgents.some((item) => item.id === current) && current) ||
          (nextDefaultAgent &&
            !nextDefaultAgent.isBuiltin &&
            nextEditableAgents.some((item) => item.id === nextDefaultAgent.id) &&
            nextDefaultAgent.id) ||
          nextEditableAgents[0]?.id ||
          ''
        return nextManagedId
      })

      if (nextManagedId) {
        const targetAgent = nextEditableAgents.find((item) => item.id === nextManagedId) ?? null
        if (targetAgent) {
          setAgentEditorMode('edit')
          setAgentEditorDraft(createAgentDraftFromRecord(targetAgent))
        }
      } else {
        setAgentEditorDraft(null)
      }
      return true
    } catch (loadError) {
      const message = loadError instanceof Error ? loadError.message : String(loadError)
      setAgentsError(message)
      return false
    } finally {
      setAgentsLoading(false)
    }
  }, [])

  useEffect(() => {
    void refreshAgents()
  }, [refreshAgents])

  const handleCreateAgentFromDraft = async (draft: AgentBuilderDraft, actionId: string) => {
    const fallbackProviderId = draft.defaultProviderId.trim() || effectiveChatRuntime?.providerId?.trim() || ''
    const fallbackModel = draft.defaultModel.trim() || effectiveChatRuntime?.model?.trim() || ''
    const availableSkillIds = new Set(
      [...installedSkills.map((skill) => skill.id), ...systemSkillCatalog.skills.map((skill) => skill.id)],
    )
    const normalizedSkillIds = draft.skillIds.filter((skillId) => availableSkillIds.has(skillId))
    const omittedSkillIds = draft.skillIds.filter((skillId) => !availableSkillIds.has(skillId))

    const payload: AgentInput = normalizeAgentDraft({
      name: draft.name,
      summary: draft.summary,
      description: draft.description,
      systemPrompt: draft.systemPrompt,
      skillIds: normalizedSkillIds,
      defaultProviderId: fallbackProviderId,
      defaultModel: fallbackModel,
      executionMode: draft.executionMode,
      botConfigs: createAgentBotConfigState(draft.botConfigs),
      heartbeatConfig: normalizeHeartbeatConfig(draft.heartbeatConfig),
      ...(draft.collaborationConfig ? { collaborationConfig: draft.collaborationConfig } : {}),
      ...(draft.accentColor ? { accentColor: draft.accentColor } : {}),
    })

    const validationError = validateAgentDraft(payload)
    if (validationError) {
      setAgentBuilderActionTargetId(actionId)
      setAgentBuilderActionError(validationError)
      setAgentBuilderActionNotice('')
      return
    }

    setAgentBuilderActionBusyId(actionId)
    setAgentBuilderActionTargetId(actionId)
    setAgentBuilderActionError('')
    setAgentBuilderActionNotice('')

    try {
      const created = await createAgent(payload)
      await refreshAgents(created.id)
      setView('agents')
      setManagedAgentId(created.id)
      setAgentEditorMode('edit')
      setAgentEditorDraft(createAgentDraftFromRecord(created))
      setAgentEditorOpen(true)
      setAgentBuilderActionNotice(
        omittedSkillIds.length > 0
          ? `已创建智能体，未挂载未安装技能：${omittedSkillIds.join('、')}`
          : '已创建智能体，并同步到智能体管理列表。',
      )
    } catch (createError) {
      const message = createError instanceof Error ? createError.message : String(createError)
      setAgentBuilderActionError(message)
    } finally {
      setAgentBuilderActionBusyId('')
    }
  }

  const handleViewChange = (nextView: ViewKey) => {
    startTransition(() => {
      setView(nextView)
      setSidebarOverlayOpen(false)
      setSkillInstallDialogOpen(false)
      setAgentSkillPickerOpen(false)
      setAgentSkillSearch('')
      setHistoryContextMenu(null)
      if (nextView !== 'agents') {
        setAgentEditorOpen(false)
      }
      if (nextView !== 'chat') {
        setNewSessionDialogOpen(false)
      }
      if (nextView !== 'workspaces') {
        setActiveWorkspaceId(null)
      }
    })
  }

  const handleSelectWorkspace = (id: string) => {
    startTransition(() => {
      setActiveWorkspaceId(id)
      resetSessionDraft()
    })
  }

  const handleBackToWorkspaces = () => {
    startTransition(() => {
      setActiveWorkspaceId(null)
      resetSessionDraft()
    })
  }

  const handleStartNewWorkspaceSession = () => {
    startTransition(() => {
      resetSessionDraft()
    })
  }

  const handleWorkspaceSelectSession = (sessionId: string) => {
    if (chatGateError) {
      setChatGateError('')
    }
    setHistoryContextMenu(null)
    selectHistoryItem(sessionId)
  }

  /** 委派计划卡"全部下发"：逐项调用 workspace_run_delegate_task，结果以后端事件为准由各卡片自渲染。 */
  const handleDispatchDelegatePlan = async (payload: {
    workspaceId: string
    planId: string
    items: Array<{ assignee: string; task: string }>
  }) => {
    if (!effectiveChatRuntime) {
      setChatGateError('缺少可用的聊天模型配置，无法下发委派。')
      return
    }
    for (const item of payload.items) {
      try {
        await workspaceRunDelegateTask({
          workspaceId: payload.workspaceId,
          sessionId: activeHistoryId ?? null,
          assignee: item.assignee,
          task: item.task,
          providerConfig: effectiveChatRuntime,
        })
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        setChatGateError(`委派 ${item.assignee} 失败：${message}`)
      }
    }
  }

  const openSettings = (tab: SettingsTab) => {
    startTransition(() => {
      setSettingsOpen(true)
      setSettingsTab(tab)
    })
  }

  const openNewSessionDialog = () => {
    const seedAgent =
      (activeChatAgent && agents.find((item) => item.id === activeChatAgent.id)) ||
      defaultAgent ||
      agents[0] ||
      null

    if (seedAgent) {
      setNewSessionAgentId(seedAgent.id)
      setNewSessionLlm(
        isValidConfiguredModelReference(
          seedAgent.defaultProviderId,
          seedAgent.defaultModel,
          mergedProviderDefinitions,
          providerConfigs,
        )
          ? {
              providerId: seedAgent.defaultProviderId,
              model: seedAgent.defaultModel,
            }
          : pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, seedAgent.defaultProviderId),
      )
    } else {
      setNewSessionAgentId('')
      setNewSessionLlm(null)
    }

    setNewSessionDialogOpen(true)
    setSidebarOverlayOpen(false)
  }

  const handleNewSession = () => {
    openNewSessionDialog()
  }

  const handleSessionLlmChange = (nextProviderId: ProviderId, nextModel: string) => {
    if (chatGateError) {
      setChatGateError('')
    }
    if (activeHistoryId) {
      updateSessionLlm(activeHistoryId, nextProviderId, nextModel)
    } else {
      setComposerSessionLlm({ providerId: nextProviderId, model: nextModel })
    }
  }

  const handleSessionLlmSelectChange = (value: string) => {
    const parsed = sessionLlmDecode(value)
    if (!parsed) {
      return
    }
    handleSessionLlmChange(parsed.providerId, parsed.model)
  }

  const handleSubmit = async (
    composerText: string,
    extras?: { overrideAgentId?: string | null },
  ) => {
    if (composerAttachmentUploading) {
      setChatGateError('附件仍在导入中，请稍等片刻再发送。')
      return
    }

    const botTarget = activeHistoryItem?.botTarget ?? null
    const hasDirectBotAttachments = Boolean(botTarget && composerAttachments.length > 0)

    if (hasDirectBotAttachments && botTarget) {
      if (chatGateError) {
        setChatGateError('')
      }

      try {
        const trimmedDraft = composerText.trim()
        if (trimmedDraft) {
          await botSendMessage(botTarget.channelId, botTarget.userId, trimmedDraft)
        }

        for (const attachment of composerAttachments) {
          await botSendMedia(
            botTarget.channelId,
            botTarget.userId,
            toBotSendMediaType(attachment.kind),
            attachment.filePath,
          )
        }

        composerClearRef.current?.()
        clearComposerAttachments()
      } catch (sendError) {
        const message = sendError instanceof Error ? sendError.message : String(sendError)
        setChatGateError(message)
      }
      return
    }

    if (runtimeResolutionError) {
      setChatGateError(runtimeResolutionError)
      return
    }
    if (chatGateError) {
      setChatGateError('')
    }
    const promptWithAttachments = buildPromptWithAttachments(composerText, composerAttachments)
    clearComposerAttachments()
    const effectiveWorkspaceId =
      activeHistoryItem?.workspaceId ??
      (view === 'workspaces' ? activeWorkspaceId : null) ??
      null
    const overrideAgentId = extras?.overrideAgentId ?? null
    /** 团队空间 @ 点名：用该成员的 agent 快照覆盖默认 agent，让 PI 以其身份回话。 */
    const overrideAgent = overrideAgentId
      ? agents.find((a) => a.id === overrideAgentId) ?? null
      : null

    // 团队边界双保险：若处于团队会话，且指定了发言 agent，则先在前端校验它属于成员名单。
    // 非法 agent 直接提示，避免在后端被 `WORKSPACE_MEMBER_ONLY` 拒绝才看到错误。
    if (effectiveWorkspaceId && overrideAgentId) {
      try {
        const members = await workspaceListMembers(effectiveWorkspaceId)
        const ok = members.some((m) => m.agentId === overrideAgentId)
        if (!ok) {
          setChatGateError('该智能体不在团队内，无法在本团队会话中发言。')
          return
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        setChatGateError(`校验团队成员失败：${message}`)
        return
      }
    }

    const effectiveAgent: ConversationAgentSnapshot | null = overrideAgent
      ? buildConversationAgentSnapshot(overrideAgent)
      : activeHistoryItem
        ? activeHistoryItem.agent ?? null
        : preferredComposerAgent
    void submitPrompt(promptWithAttachments, {
      providerConfig: effectiveChatRuntime,
      agent: effectiveAgent,
      sessionLlm: sessionLlmDisplay,
      attachments: composerAttachments,
      workspaceId: effectiveWorkspaceId,
      overrideAgentId,
    })
  }

  const handleSkillInstallConversation = async () => {
    const trimmedLink = skillInstallLink.trim()
    const validationError = validateSkillInstallLink(trimmedLink)
    if (validationError) {
      setSkillInstallError(validationError)
      return
    }

    setSkillInstallLaunching(true)
    setSkillInstallError('')
    setSkillInstallDialogOpen(false)
    handleViewChange('chat')

    try {
      await submitPromptInNewSession(buildSkillInstallPrompt(trimmedLink), {
        providerConfig: effectiveChatRuntime,
        sessionLlm: sessionLlmDisplay,
      })
      setSkillInstallLink('')
    } finally {
      setSkillInstallLaunching(false)
    }
  }

  const handleHistorySelect = (id: string) => {
    if (chatGateError) {
      setChatGateError('')
    }
    setHistoryContextMenu(null)
    handleViewChange('chat')
    selectHistoryItem(id)
  }

  const handleHistoryContextMenu = (event: MouseEvent<HTMLButtonElement>, item: HistoryItem) => {
    event.preventDefault()

    const menuWidth = 196
    const menuHeight = 56
    const maxX = Math.max(12, window.innerWidth - menuWidth - 12)
    const maxY = Math.max(12, window.innerHeight - menuHeight - 12)
    const canDelete = !runningHistoryIds.includes(item.id)

    setHistoryContextMenu({
      sessionId: item.id,
      title: item.title,
      x: Math.min(event.clientX, maxX),
      y: Math.min(event.clientY, maxY),
      canDelete,
    })
  }

  const handleRequestDeleteHistoryItem = (sessionId: string) => {
    const target = history.find((item) => item.id === sessionId)
    setHistoryContextMenu(null)
    if (!target || runningHistoryIds.includes(sessionId)) {
      return
    }

    setHistoryDeleteTarget({
      sessionId: target.id,
      title: target.title,
    })
  }

  const handleConfirmDeleteHistoryItem = async () => {
    if (!historyDeleteTarget || historyDeleteBusy) {
      return
    }

    setHistoryDeleteBusy(true)
    try {
      await deleteHistoryItem(historyDeleteTarget.sessionId)
      setHistoryDeleteTarget(null)
    } finally {
      setHistoryDeleteBusy(false)
    }
  }

  const handleManagedAgentSelect = (id: string) => {
    const targetAgent = editableAgents.find((item) => item.id === id)
    if (!targetAgent) {
      return
    }

    setAgentWorkspaceDialogOpen(false)
    setAgentWorkspaceDialogError('')
    setAgentWorkspaceBundle(null)
    setAgentWorkspaceSelectedKey('')
    setAgentWorkspaceDraftContent('')
    setAgentWorkspaceSaving(false)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')
    setManagedAgentId(id)
    setAgentEditorMode('edit')
    setAgentEditorDraft(createAgentDraftFromRecord(targetAgent))
    setAgentFormError('')
    setAgentFormNotice('')
    setAgentBotBindingDialogOpen(false)
    setAgentDeleteConfirmOpen(false)
    setAgentDeleteConfirmText('')
    handleCloseAgentSkillPicker()
  }

  const handleOpenAgentEditor = (agentId: string) => {
    const targetAgent = editableAgents.find((item) => item.id === agentId)
    if (!targetAgent) {
      return
    }
    handleManagedAgentSelect(agentId)
    setAgentEditorOpen(true)
  }

  const handleOpenAgentBotBinding = (agentId: string) => {
    const targetAgent = editableAgents.find((item) => item.id === agentId)
    if (!targetAgent) {
      return
    }
    handleManagedAgentSelect(agentId)
    setAgentEditorOpen(false)
    setAgentBotBindingDialogOpen(true)
  }

  const handleCreateAgentDraft = () => {
    setAgentWorkspaceDialogOpen(false)
    setAgentWorkspaceDialogError('')
    setAgentWorkspaceBundle(null)
    setAgentWorkspaceSelectedKey('')
    setAgentWorkspaceDraftContent('')
    setAgentWorkspaceSaving(false)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')
    setManagedAgentId('')
    setAgentEditorMode('create')
    setAgentFormError('')
    setAgentFormNotice('')
    setAgentDeleteConfirmOpen(false)
    setAgentDeleteConfirmText('')
    setAgentSkillSearch('')
    setAgentSkillPickerOpen(false)
    setAgentEditorOpen(true)
    setAgentEditorDraft(
      createEmptyAgentDraft(
        defaultAgent?.defaultProviderId ?? sessionLlmDisplay.providerId,
        defaultAgent?.defaultModel || sessionLlmDisplay.model,
        defaultAgent?.accentColor,
      ),
    )
    handleViewChange('agents')
  }

  const handleAgentDraftChange = (updates: Partial<AgentInput>) => {
    setAgentEditorDraft((current) => (current ? { ...current, ...updates } : current))
    if (agentFormError) {
      setAgentFormError('')
    }
    if (agentFormNotice) {
      setAgentFormNotice('')
    }
  }

  const handleAgentSkillToggle = (skillId: string) => {
    setAgentEditorDraft((current) => {
      if (!current) {
        return current
      }

      const nextSkillIds = current.skillIds.includes(skillId)
        ? current.skillIds.filter((item) => item !== skillId)
        : [...current.skillIds, skillId]

      return { ...current, skillIds: nextSkillIds }
    })
  }

  const handleOpenAgentSkillPicker = () => {
    if (!agentEditorDraft) {
      return
    }
    setAgentSkillSearch('')
    setAgentSkillPickerOpen(true)
  }

  const handleCloseAgentSkillPicker = () => {
    setAgentSkillPickerOpen(false)
    setAgentSkillSearch('')
  }

  const handleCloseAgentEditor = () => {
    setAgentEditorOpen(false)
    setAgentDeleteConfirmOpen(false)
    setAgentDeleteConfirmText('')
    setAgentWorkspaceDialogOpen(false)
    setAgentWorkspaceDraftContent('')
    setAgentWorkspaceSaving(false)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')
    handleCloseAgentSkillPicker()
    setAgentFormError('')
    setAgentFormNotice('')
  }

  const handleCloseAgentBotBindingDialog = () => {
    setAgentBotBindingDialogOpen(false)
    setQrDialogOpen(false)
  }

  const loadAgentWorkspaceBundle = useCallback(async (agent: AgentRecord) => {
    setAgentWorkspaceDialogLoading(true)
    setAgentWorkspaceDialogError('')
    setAgentWorkspaceSaving(false)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')

    try {
      const bundle = await readAgentWorkspaceBundle(agent.id)
      const nextKey =
        agentWorkspaceSelectedKey && bundle.files.some((file) => file.key === agentWorkspaceSelectedKey)
          ? agentWorkspaceSelectedKey
          : pickDefaultWorkspaceFileKey(bundle)
      const nextFile =
        bundle.files.find((file) => file.key === nextKey) ?? bundle.files.find((file) => file.exists) ?? bundle.files[0] ?? null
      setAgentWorkspaceBundle(bundle)
      setAgentWorkspaceSelectedKey(nextKey)
      setAgentWorkspaceDraftContent(nextFile?.content ?? '')
      setAgentWorkspaceDialogOpen(true)
    } catch (loadError) {
      const message = loadError instanceof Error ? loadError.message : String(loadError)
      setAgentWorkspaceDialogError(message)
      setAgentWorkspaceDraftContent('')
      setAgentWorkspaceDialogOpen(true)
    } finally {
      setAgentWorkspaceDialogLoading(false)
    }
  }, [agentWorkspaceSelectedKey])

  const handleOpenAgentWorkspace = async () => {
    if (!selectedManagedAgent) {
      return
    }
    await loadAgentWorkspaceBundle(selectedManagedAgent)
  }

  const handleRefreshAgentWorkspace = async () => {
    if (!selectedManagedAgent) {
      return
    }
    await loadAgentWorkspaceBundle(selectedManagedAgent)
  }

  const handleCloseAgentWorkspaceDialog = () => {
    setAgentWorkspaceDialogOpen(false)
    setAgentWorkspaceDialogError('')
    setAgentWorkspaceDraftContent('')
    setAgentWorkspaceSaving(false)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')
    setAgentWorkspaceFileLoading(false)
  }

  const handleSelectAgentWorkspaceFile = async (key: string) => {
    const bundle = agentWorkspaceBundle
    const nextFile = bundle?.files.find((file) => file.key === key) ?? null
    setAgentWorkspaceSelectedKey(key)
    if (agentWorkspaceSaveError) {
      setAgentWorkspaceSaveError('')
    }
    if (agentWorkspaceSaveNotice) {
      setAgentWorkspaceSaveNotice('')
    }

    if (!nextFile || !selectedManagedAgent) {
      setAgentWorkspaceDraftContent('')
      return
    }

    if (nextFile.lazyFetch && nextFile.exists) {
      setAgentWorkspaceDraftContent('')
      setAgentWorkspaceFileLoading(true)
      try {
        const loaded = await readAgentWorkspaceFile({
          agentId: selectedManagedAgent.id,
          relativePath: nextFile.relativePath,
        })
        setAgentWorkspaceBundle((previous) => {
          if (!previous) {
            return previous
          }
          return {
            ...previous,
            files: previous.files.map((file) => (file.key === key ? loaded : file)),
          }
        })
        setAgentWorkspaceDraftContent(loaded.content)
      } catch (loadError) {
        const message = loadError instanceof Error ? loadError.message : String(loadError)
        setAgentWorkspaceSaveError(message)
        setAgentWorkspaceDraftContent('')
      } finally {
        setAgentWorkspaceFileLoading(false)
      }
      return
    }

    setAgentWorkspaceDraftContent(nextFile.content ?? '')
  }

  const handleSaveAgentWorkspaceFile = async (file: AgentWorkspaceFile, content: string) => {
    if (!selectedManagedAgent || agentWorkspaceSaving) {
      return
    }

    setAgentWorkspaceSaving(true)
    setAgentWorkspaceSaveError('')
    setAgentWorkspaceSaveNotice('')

    try {
      const bundle = await writeAgentWorkspaceFile({
        agentId: selectedManagedAgent.id,
        relativePath: file.relativePath,
        content,
      })
      const mergedFiles = bundle.files.map((bundleFile) =>
        bundleFile.relativePath === file.relativePath
          ? { ...bundleFile, content, lazyFetch: false, exists: true }
          : bundleFile,
      )
      const mergedBundle = { ...bundle, files: mergedFiles }
      const nextKey =
        agentWorkspaceSelectedKey && mergedBundle.files.some((bundleFile) => bundleFile.key === agentWorkspaceSelectedKey)
          ? agentWorkspaceSelectedKey
          : pickDefaultWorkspaceFileKey(mergedBundle)
      const nextFile =
        mergedBundle.files.find((bundleFile) => bundleFile.key === nextKey) ??
        mergedBundle.files.find((bundleFile) => bundleFile.exists) ??
        mergedBundle.files[0] ??
        null
      setAgentWorkspaceBundle(mergedBundle)
      setAgentWorkspaceSelectedKey(nextKey)
      setAgentWorkspaceDraftContent(nextKey === file.key ? content : (nextFile?.content ?? ''))
      setAgentWorkspaceSaveNotice(`${file.name} 已保存`)
    } catch (saveError) {
      const message = saveError instanceof Error ? saveError.message : String(saveError)
      setAgentWorkspaceSaveError(message)
    } finally {
      setAgentWorkspaceSaving(false)
    }
  }

  const handleSaveAgent = async () => {
    if (!agentEditorDraft || agentSaving) {
      return
    }

    const normalizedDraft = normalizeAgentDraft(agentEditorDraft)
    const validationError = validateAgentDraft(normalizedDraft)
    if (validationError) {
      setAgentFormError(validationError)
      return
    }

    setAgentSaving(true)
    setAgentFormError('')
    setAgentFormNotice('')

    try {
      const savedAgent =
        agentEditorMode === 'create' || !managedAgentId
          ? await createAgent(normalizedDraft)
          : await updateAgent(managedAgentId, normalizedDraft)

      setManagedAgentId(savedAgent.id)
      setAgentEditorMode('edit')
      setAgentEditorDraft(createAgentDraftFromRecord(savedAgent))
      toast.success(agentEditorMode === 'create' ? '智能体已创建。' : '智能体已保存。')
      handleCloseAgentSkillPicker()
      await refreshAgents(savedAgent.id)
    } catch (saveError) {
      const message = saveError instanceof Error ? saveError.message : String(saveError)
      setAgentFormError(message)
    } finally {
      setAgentSaving(false)
    }
  }

  const handleRefreshCurrentAgent = async () => {
    const preferredAgentId = selectedManagedAgent?.id || managedAgentId || undefined

    setAgentRefreshing(true)
    setAgentFormError('')
    if (preferredAgentId) {
      setAgentFormNotice('正在刷新最新配置…')
    }

    try {
      const refreshed = await refreshAgents(preferredAgentId)
      if (preferredAgentId && refreshed) {
        setAgentFormNotice('已刷新当前智能体配置。')
      } else if (!refreshed) {
        setAgentFormNotice('')
      }
    } catch (refreshError) {
      const message = refreshError instanceof Error ? refreshError.message : String(refreshError)
      setAgentFormError(message)
    } finally {
      setAgentRefreshing(false)
    }
  }

  const handleRequestDeleteCurrentAgent = () => {
    if (!selectedManagedAgent || agentSaving) {
      return
    }
    setAgentDeleteConfirmText('')
    setAgentDeleteConfirmOpen(true)
  }

  const handleCloseDeleteAgentDialog = () => {
    if (agentSaving) {
      return
    }
    setAgentDeleteConfirmOpen(false)
    setAgentDeleteConfirmText('')
  }

  const handleConfirmDeleteCurrentAgent = async () => {
    if (!selectedManagedAgent || agentSaving) {
      return
    }
    if (agentDeleteConfirmText.trim() !== '确认删除') {
      setAgentFormError('请输入“确认删除”后再删除智能体。')
      return
    }

    setAgentSaving(true)
    setAgentFormError('')
    setAgentFormNotice('')

    try {
      await deleteAgent(selectedManagedAgent.id)
      setAgentDeleteConfirmOpen(false)
      setAgentDeleteConfirmText('')
      setAgentEditorMode('edit')
      setAgentEditorDraft(null)
      setAgentEditorOpen(false)
      handleCloseAgentSkillPicker()
      await refreshAgents()
    } catch (deleteError) {
      const message = deleteError instanceof Error ? deleteError.message : String(deleteError)
      setAgentFormError(message)
    } finally {
      setAgentSaving(false)
    }
  }

  const handleSetCurrentDefaultAgent = async () => {
    if (!selectedManagedAgent || agentSaving) {
      return
    }

    setAgentSaving(true)
    setAgentFormError('')
    setAgentFormNotice('')

    try {
      const nextDefaultAgent = await setDefaultAgent(selectedManagedAgent.id)
      setDefaultAgentId(nextDefaultAgent?.id ?? '')
      setAgentFormNotice(`已将「${selectedManagedAgent.name}」设为默认智能体。`)
    } catch (updateError) {
      const message = updateError instanceof Error ? updateError.message : String(updateError)
      setAgentFormError(message)
    } finally {
      setAgentSaving(false)
    }
  }

  const handleNewSessionAgentChange = (agentId: string) => {
    setNewSessionAgentId(agentId)
    const targetAgent = agents.find((item) => item.id === agentId)
    if (!targetAgent) {
      return
    }
    setNewSessionLlm(
      isValidConfiguredModelReference(
        targetAgent.defaultProviderId,
        targetAgent.defaultModel,
        mergedProviderDefinitions,
        providerConfigs,
      )
        ? {
            providerId: targetAgent.defaultProviderId,
            model: targetAgent.defaultModel,
          }
        : pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, targetAgent.defaultProviderId),
    )
  }

  const handleConfirmNewSession = () => {
    const targetAgent = agents.find((item) => item.id === newSessionAgentId) ?? defaultAgent
    if (!targetAgent) {
      return
    }

    if (chatGateError) {
      setChatGateError('')
    }
    const nextSessionLlm =
      newSessionLlm &&
      isValidConfiguredModelReference(
        newSessionLlm.providerId,
        newSessionLlm.model,
        mergedProviderDefinitions,
        providerConfigs,
      )
        ? newSessionLlm
        : pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, targetAgent.defaultProviderId)
    resetSessionDraft()
    setComposerAgent(buildConversationAgentSnapshot(targetAgent))
    setComposerSessionLlm(nextSessionLlm)
    setNewSessionDialogOpen(false)
    handleViewChange('chat')
  }

  const handleStartChatWithAgent = (agentId: string) => {
    const targetAgent = agents.find((item) => item.id === agentId)
    if (!targetAgent) {
      return
    }

    if (chatGateError) {
      setChatGateError('')
    }

    const nextSessionLlm = isValidConfiguredModelReference(
      targetAgent.defaultProviderId,
      targetAgent.defaultModel,
      mergedProviderDefinitions,
      providerConfigs,
    )
      ? {
          providerId: targetAgent.defaultProviderId,
          model: targetAgent.defaultModel,
        }
      : pickFallbackSessionLlm(mergedProviderDefinitions, providerConfigs, targetAgent.defaultProviderId)

    resetSessionDraft()
    setComposerAgent(buildConversationAgentSnapshot(targetAgent))
    setComposerSessionLlm(nextSessionLlm)
    setNewSessionDialogOpen(false)
    setAgentEditorOpen(false)
    handleViewChange('chat')
  }

  const updateAgentBotConfig = (channelId: BotChannelId, updates: Partial<BotConfig>) => {
    setAgentEditorDraft((current) => {
      if (!current) {
        return current
      }
      const currentConfigs = createAgentBotConfigState(current.botConfigs)
      return {
        ...current,
        botConfigs: {
          ...currentConfigs,
          [channelId]: {
            ...currentConfigs[channelId],
            ...updates,
          },
        },
      }
    })
    if (agentFormError) {
      setAgentFormError('')
    }
    if (agentFormNotice) {
      setAgentFormNotice('')
    }
  }

  const updateProviderConfig = (providerId: ProviderId, updates: Partial<ProviderConfig>) => {
    setProviderConfigs((previous) => {
      const base = previous[providerId] ?? emptyProviderConfig()
      return {
        ...previous,
        [providerId]: {
          ...base,
          ...updates,
          status:
            updates.status ??
            getProviderStatus(
              {
                ...base,
                ...updates,
              },
              false,
            ),
        },
      }
    })
  }

  const addCustomProvider = (name: string, description: string, apiFormat: ProviderApiFormat) => {
    const id = `custom_${crypto.randomUUID().replace(/-/g, '')}`
    const defaultBaseUrl = apiFormat === 'anthropic' ? 'https://api.anthropic.com' : 'https://api.openai.com/v1'
    const defaultModel = apiFormat === 'anthropic' ? 'claude-sonnet-4-0' : 'gpt-4o-mini'
    setCustomProviderMeta((previous) => [...previous, { id, name, description, apiFormat }])
    setProviderConfigs((previous) => ({
      ...previous,
      [id]: {
        ...emptyProviderConfig(),
        added: true,
        apiFormat,
        baseUrl: defaultBaseUrl,
        model: defaultModel,
        displayName: name.trim() || name,
        status: '未配置',
      },
    }))
    setSelectedProviderId(id)
  }

  const removeCustomProvider = (providerId: ProviderId) => {
    if (!providerId.startsWith('custom_')) {
      return
    }
    setCustomProviderMeta((previous) => previous.filter((item) => item.id !== providerId))
    setProviderConfigs((previous) => {
      const next = { ...previous }
      delete next[providerId]
      return next
    })
  }

  // ── Bot Channel Actions ──

  const handleWechatLogin = async () => {
    if (!selectedManagedAgent) {
      setAgentFormError('请先保存当前智能体，再绑定微信 Bot。')
      return
    }
    setBotLoading(true)
    setQrStatus('waiting')
    setQrCodeUrl('')
    setQrDialogOpen(true)

    const runtimeChannelId = selectedManagedAgent
      ? getBotChannelRuntimeId(selectedManagedAgent.id, 'wechat')
      : ''

    try {
      // Subscribe to QR events before calling login
      const unsub = await subscribeQrCode((event: QrCodeEvent) => {
        if (runtimeChannelId && event.channelId !== runtimeChannelId) {
          return
        }
        if (event.qrcodeUrl) {
          setQrCodeUrl(event.qrcodeUrl)
        }
        if (event.status === 'scanned') {
          setQrStatus('scanned')
        }
        if (event.status === 'confirmed') {
          setQrStatus('confirmed')
          setQrDialogOpen(false)
        }
      })

      const result = await botLoginWechat(runtimeChannelId)
      unsub()

      if (result.connected) {
        const loginToken = result.bot_token ?? ''
        const loginBaseUrl = result.base_url ?? 'https://ilinkai.weixin.qq.com'

        if (!selectedManagedAgent) {
          throw new Error('请先保存当前智能体，再绑定微信 Bot。')
        }
        const botRuntime = resolveRuntimeFromAgentSnapshot(
          buildConversationAgentSnapshot(selectedManagedAgent),
          providerConfigs,
        )
        if (!botRuntime) {
          throw new Error('请先在设置中配置该智能体默认模型对应的 Provider（Base URL、API Key）。')
        }

        updateAgentBotConfig('wechat', {
          status: '已连接',
          clientId: result.account_id ?? '',
          clientSecret: loginBaseUrl,
          token: loginToken,
          imChannelPaused: false,
          ...buildBotRuntimeBindingConfig(botRuntime),
          errorMessage: undefined,
        })
        setQrDialogOpen(false)

        // Auto-start the bot polling immediately after login
        try {
          await botStartWechat(getBotChannelRuntimeId(selectedManagedAgent.id, 'wechat'), selectedManagedAgent.id, loginToken, {
            baseUrl: loginBaseUrl || undefined,
            providerId: botRuntime.providerId,
            providerApiFormat: botRuntime.apiFormat,
            model: botRuntime.model,
            apiKey: botRuntime.apiKey,
            providerBaseUrl: botRuntime.baseUrl,
          })
          updateAgentBotConfig('wechat', { enabled: true, imChannelPaused: false })
        } catch (startError) {
          updateAgentBotConfig('wechat', {
            status: '错误',
            errorMessage: `登录成功但启动失败: ${String(startError)}`,
          })
        }
      } else {
        updateAgentBotConfig('wechat', {
          status: '错误',
          errorMessage: result.message,
        })
        setQrDialogOpen(false)
      }
    } catch (error) {
      updateAgentBotConfig('wechat', {
        status: '错误',
        errorMessage: String(error),
      })
      setQrDialogOpen(false)
    } finally {
      setBotLoading(false)
    }
  }

  const handleWechatStart = async () => {
    if (!selectedManagedAgent) {
      setAgentFormError('请先保存当前智能体，再启动微信 Bot。')
      return
    }

    const config = selectedManagedBotConfigs.wechat
    if (!config.token) {
      updateAgentBotConfig('wechat', { status: '错误', errorMessage: '请先扫码登录获取 token' })
      return
    }
    setBotLoading(true)
    try {
      const botRuntime = resolveRuntimeFromAgentSnapshot(
        buildConversationAgentSnapshot(selectedManagedAgent),
        providerConfigs,
      )
      if (!botRuntime) {
        throw new Error('请先在设置中配置该智能体默认模型对应的 Provider（Base URL、API Key）。')
      }
      updateAgentBotConfig('wechat', {
        ...buildBotRuntimeBindingConfig(botRuntime),
        imChannelPaused: false,
      })
      await botStartWechat(getBotChannelRuntimeId(selectedManagedAgent.id, 'wechat'), selectedManagedAgent.id, config.token, {
        baseUrl: config.clientSecret || undefined,
        routeTag: config.routeTag || undefined,
        providerId: botRuntime.providerId,
        providerApiFormat: botRuntime.apiFormat,
        model: botRuntime.model,
        apiKey: botRuntime.apiKey,
        providerBaseUrl: botRuntime.baseUrl,
      })
      updateAgentBotConfig('wechat', {
        status: '已连接',
        enabled: true,
        imChannelPaused: false,
        errorMessage: undefined,
      })
    } catch (error) {
      updateAgentBotConfig('wechat', { status: '错误', errorMessage: String(error) })
    } finally {
      setBotLoading(false)
    }
  }

  const handleWechatStop = async () => {
    if (!selectedManagedAgent) {
      return
    }
    setBotLoading(true)
    try {
      await botStopWechat(getBotChannelRuntimeId(selectedManagedAgent.id, 'wechat'))
      updateAgentBotConfig('wechat', { status: '未连接', enabled: false, imChannelPaused: true })
    } catch (error) {
      updateAgentBotConfig('wechat', { status: '错误', errorMessage: String(error) })
    } finally {
      setBotLoading(false)
    }
  }

  const handleLarkStart = async () => {
    if (!selectedManagedAgent) {
      setAgentFormError('请先保存当前智能体，再启动飞书 Bot。')
      return
    }

    const config = selectedManagedBotConfigs.lark
    if (!config.clientId.trim() || !config.clientSecret.trim()) {
      updateAgentBotConfig('lark', {
        status: '错误',
        errorMessage: '请先填写飞书 App ID 和 App Secret',
      })
      return
    }

    setBotLoading(true)
    try {
      const botRuntime = resolveRuntimeFromAgentSnapshot(
        buildConversationAgentSnapshot(selectedManagedAgent),
        providerConfigs,
      )
      if (!botRuntime) {
        throw new Error('请先在设置中配置该智能体默认模型对应的 Provider（Base URL、API Key）。')
      }

      updateAgentBotConfig('lark', {
        ...buildBotRuntimeBindingConfig(botRuntime),
        imChannelPaused: false,
        status: '登录中',
        errorMessage: undefined,
      })
      await botStartLark(
        getBotChannelRuntimeId(selectedManagedAgent.id, 'lark'),
        selectedManagedAgent.id,
        config.clientId.trim(),
        config.clientSecret.trim(),
        {
          providerId: botRuntime.providerId,
          providerApiFormat: botRuntime.apiFormat,
          model: botRuntime.model,
          apiKey: botRuntime.apiKey,
          providerBaseUrl: botRuntime.baseUrl,
        },
      )

      updateAgentBotConfig('lark', {
        status: '已连接',
        enabled: true,
        imChannelPaused: false,
        errorMessage: undefined,
      })
    } catch (error) {
      updateAgentBotConfig('lark', { status: '错误', errorMessage: String(error) })
    } finally {
      setBotLoading(false)
    }
  }

  const handleLarkStop = async () => {
    if (!selectedManagedAgent) {
      return
    }
    setBotLoading(true)
    try {
      await botStopLark(getBotChannelRuntimeId(selectedManagedAgent.id, 'lark'))
      updateAgentBotConfig('lark', {
        status: '未连接',
        enabled: false,
        imChannelPaused: true,
        errorMessage: undefined,
      })
    } catch (error) {
      updateAgentBotConfig('lark', { status: '错误', errorMessage: String(error) })
    } finally {
      setBotLoading(false)
    }
  }

  const handleRotatePeerSecret = async () => {
    if (!selectedManagedAgent) {
      return
    }
    setBotLoading(true)
    setAgentFormError('')
    setAgentFormNotice('')
    try {
      await rotateAgentPeerInboundSecret(selectedManagedAgent.id)
      await refreshAgents(selectedManagedAgent.id)
      setAgentFormNotice('已重新生成该智能体的对等入站密钥并写入数据库。')
    } catch (error) {
      setAgentFormError(error instanceof Error ? error.message : String(error))
    } finally {
      setBotLoading(false)
    }
  }

  return (
    <NineClawAppChrome
      chatProviderLabel={chatProviderLabel}
      onSessionLlmSelectChange={handleSessionLlmSelectChange}
      routeOutlet={
        <NineClawRouteOutlet
          view={view}
          agentBuilderActionBusyId={agentBuilderActionBusyId}
          agentBuilderActionError={agentBuilderActionError}
          agentBuilderActionNotice={agentBuilderActionNotice}
          agentBuilderActionTargetId={agentBuilderActionTargetId}
          composerClearRef={composerClearRef}
          composerDraftBackupRef={composerDraftBackupRef}
          chatGateError={chatGateError}
          piError={error}
          appearanceSettings={appearanceSettings}
          loading={loading}
          runningHistoryIds={runningHistoryIds}
          streamingHistoryIds={streamingHistoryIds}
          activeHistoryId={activeHistoryId}
          onAbort={abortPrompt}
          composerAttachmentError={composerAttachmentError}
          composerAttachmentInputRef={composerAttachmentInputRef}
          composerAttachmentUploading={composerAttachmentUploading}
          composerAttachments={composerAttachments}
          onChatAgentBuilderCreate={handleCreateAgentFromDraft}
          onComposerAttachmentInputChange={handleComposerAttachmentInputChange}
          onComposerClearAttachments={clearComposerAttachments}
          onComposerPaste={handleComposerPaste}
          onComposerPickAttachment={openComposerAttachmentPicker}
          onComposerRemoveAttachment={removeComposerAttachment}
          onComposerClearAttachmentError={clearComposerAttachmentError}
          onChatSubmit={handleSubmit}
          activeChatAgent={activeChatAgent}
          submitShortcut={generalSettings.submitShortcut}
          activeHistoryItem={activeHistoryItem}
          sessionLlmSelectOptionsWithFallback={sessionLlmSelectOptionsWithFallback}
          runtimeReady={runtimeReady}
          runtimeBlockingReason={runtimeBlockingReason}
          sessionContextProviderConfig={
            activeHistoryItem?.sessionLlmProviderId
              ? { maxContextTokens: providerConfigs[activeHistoryItem.sessionLlmProviderId]?.maxContextTokens }
              : selectedProviderId
                ? { maxContextTokens: providerConfigs[selectedProviderId]?.maxContextTokens }
                : null
          }
          installedSkills={installedSkills}
          editableAgents={editableAgents}
          onOpenAgentEditor={handleOpenAgentEditor}
          agentEditorDraft={agentEditorDraft}
          agentBotBindingDialogOpen={agentBotBindingDialogOpen}
          agentDeleteConfirmOpen={agentDeleteConfirmOpen}
          agentDeleteConfirmText={agentDeleteConfirmText}
          agentEditorOpen={agentEditorOpen}
          agentFormError={agentFormError}
          agentFormNotice={agentFormNotice}
          agentRefreshing={agentRefreshing}
          agentSaving={agentSaving}
          selectedManagedBotConfigs={selectedManagedBotConfigs}
          botLoading={botLoading}
          botStatusLogForSelected={botStatusLog.filter((entry) =>
            selectedManagedAgent ? entry.channelId === getBotChannelRuntimeId(selectedManagedAgent.id, selectedBotId) : false,
          )}
          agentWorkspaceBundle={agentWorkspaceBundle}
          agentWorkspaceDialogError={agentWorkspaceDialogError}
          agentWorkspaceDialogLoading={agentWorkspaceDialogLoading}
          agentWorkspaceFileLoading={agentWorkspaceFileLoading}
          agentWorkspaceDialogOpen={agentWorkspaceDialogOpen}
          agentWorkspaceDraftContent={agentWorkspaceDraftContent}
          agentWorkspaceSaveError={agentWorkspaceSaveError}
          agentWorkspaceSaveNotice={agentWorkspaceSaveNotice}
          agentWorkspaceSaving={agentWorkspaceSaving}
          agentWorkspaceSelectedKey={agentWorkspaceSelectedKey}
          visibleAgents={visibleAgents}
          defaultAgentId={defaultAgentId}
          onAgentsViewCreateAgent={handleCreateAgentDraft}
          onBotConfigChange={updateAgentBotConfig}
          onCloseEditor={handleCloseAgentEditor}
          onCloseBotBindingDialog={handleCloseAgentBotBindingDialog}
          onCloseDeleteAgentDialog={handleCloseDeleteAgentDialog}
          onCloseWorkspaceDialog={handleCloseAgentWorkspaceDialog}
          onConfirmDeleteAgent={handleConfirmDeleteCurrentAgent}
          onDraftWorkspaceContentChange={setAgentWorkspaceDraftContent}
          onDraftChange={handleAgentDraftChange}
          onDeleteConfirmTextChange={setAgentDeleteConfirmText}
          onRefreshAgents={handleRefreshCurrentAgent}
          onOpenBotBinding={handleOpenAgentBotBinding}
          onOpenWorkspace={handleOpenAgentWorkspace}
          onOpenSkillPicker={handleOpenAgentSkillPicker}
          onRefreshWorkspace={handleRefreshAgentWorkspace}
          onRequestDeleteAgent={handleRequestDeleteCurrentAgent}
          onSaveAgent={handleSaveAgent}
          onSaveWorkspaceFile={handleSaveAgentWorkspaceFile}
          onAgentSearchChange={setAgentSearch}
          onManagedAgentSelect={handleManagedAgentSelect}
          onSelectBot={setSelectedBotId}
          onSelectWorkspaceFile={handleSelectAgentWorkspaceFile}
          onSetDefaultAgent={handleSetCurrentDefaultAgent}
          onToggleSkill={handleAgentSkillToggle}
          onLarkStart={handleLarkStart}
          onLarkStop={handleLarkStop}
          onWechatLogin={handleWechatLogin}
          onWechatStart={handleWechatStart}
          onWechatStop={handleWechatStop}
          onRotatePeerSecret={handleRotatePeerSecret}
          onStartChatWithAgent={handleStartChatWithAgent}
          agentSearch={agentSearch}
          selectedManagedAgent={selectedManagedAgent}
          selectedManagedBotConfig={selectedManagedBotConfig}
          selectedBotDefinition={selectedBotDefinition}
          selectedBotId={selectedBotId}
          agentsLoading={agentsLoading}
          agentsError={agentsError}
          agentEditorMode={agentEditorMode}
          qrCodeUrl={qrCodeUrl}
          qrDialogOpen={qrDialogOpen}
          qrStatus={qrStatus}
          onSetBotLoading={setBotLoading}
          onSetQrDialogOpen={setQrDialogOpen}
          managedAgentId={managedAgentId}
          activeWorkspaceId={activeWorkspaceId}
          onSelectWorkspace={handleSelectWorkspace}
          onBackToWorkspaces={handleBackToWorkspaces}
          onStartNewWorkspaceSession={handleStartNewWorkspaceSession}
          onSelectSession={handleWorkspaceSelectSession}
          onDeleteSession={handleRequestDeleteHistoryItem}
          history={history}
          agents={agents}
          onDispatchDelegatePlan={handleDispatchDelegatePlan}
        />
      }
      appearanceSettings={appearanceSettings}
      effectiveSidebarCollapsed={effectiveSidebarCollapsed}
      shouldHideSidebar={shouldHideSidebar}
      sidebarOverlayOpen={sidebarOverlayOpen}
      setSidebarOverlayOpen={setSidebarOverlayOpen}
      setAppearanceSettings={setAppearanceSettings}
      view={view}
      onNewSession={handleNewSession}
      onViewChange={handleViewChange}
      history={history}
      onClearHistory={clearHistory}
      historyBusy={loading}
      historySearch={historySearch}
      setHistorySearch={setHistorySearch}
      visibleHistory={visibleHistory}
      activeHistoryId={activeHistoryId}
      onHistorySelect={handleHistorySelect}
      onHistoryContextMenu={handleHistoryContextMenu}
      onOpenSettings={openSettings}
      skillInstallDialogOpen={skillInstallDialogOpen}
      skillInstallError={skillInstallError}
      skillInstallLink={skillInstallLink}
      skillInstallLaunching={skillInstallLaunching}
      setSkillInstallLink={setSkillInstallLink}
      setSkillInstallError={setSkillInstallError}
      onCloseSkillInstallDialog={() => {
        if (!skillInstallLaunching) {
          setSkillInstallDialogOpen(false)
        }
      }}
      onConfirmSkillInstall={handleSkillInstallConversation}
      historyContextMenu={historyContextMenu}
      setHistoryContextMenu={setHistoryContextMenu}
      onRequestDeleteHistoryItem={handleRequestDeleteHistoryItem}
      historyDeleteTarget={historyDeleteTarget}
      setHistoryDeleteTarget={setHistoryDeleteTarget}
      historyDeleteBusy={historyDeleteBusy}
      onConfirmDeleteHistoryItem={handleConfirmDeleteHistoryItem}
      newSessionDialogOpen={newSessionDialogOpen}
      agents={agents}
      agentsLoading={agentsLoading}
      sessionLlmSelectOptionsWithFallback={sessionLlmSelectOptionsWithFallback}
      newSessionAgentId={newSessionAgentId}
      newSessionLlm={newSessionLlm}
      sessionLlmEncodedCurrent={sessionLlmEncodedCurrent}
      onNewSessionAgentChange={handleNewSessionAgentChange}
      setNewSessionLlm={setNewSessionLlm}
      onCloseNewSessionDialog={() => setNewSessionDialogOpen(false)}
      onConfirmNewSession={handleConfirmNewSession}
      agentSkillPickerOpen={agentSkillPickerOpen}
      agentEditorOpen={agentEditorOpen}
      agentEditorDraft={agentEditorDraft}
      installedSkills={installedSkills}
      agentSkillSearch={agentSkillSearch}
      setAgentSkillSearch={setAgentSkillSearch}
      visibleAgentSkillOptions={visibleAgentSkillOptions}
      onCloseAgentSkillPicker={handleCloseAgentSkillPicker}
      onToggleAgentSkill={handleAgentSkillToggle}
      settingsOpen={settingsOpen}
      activeProviderBadge={activeProviderBadge}
      mergedProviderDefinitions={mergedProviderDefinitions}
      generalSettings={generalSettings}
      onAddCustomProvider={addCustomProvider}
      onProviderConfigChange={updateProviderConfig}
      onCloseSettings={() => setSettingsOpen(false)}
      onRemoveCustomProvider={removeCustomProvider}
      onSelectProvider={setSelectedProviderId}
      onSelectSettingsTab={setSettingsTab}
      providerConfigs={providerConfigs}
      selectedProviderConfig={selectedProviderConfig}
      selectedProviderDefinition={selectedProviderDefinition}
      selectedProviderId={selectedProviderId}
      setAppearanceSettingsForModal={setAppearanceSettings}
      setGeneralSettings={setGeneralSettings}
      settingsTab={settingsTab}
      settingsSkillsLibrary={settingsSkillsLibrary}
      settingsResourcesLibrary={settingsResourcesLibrary}
    />
  )
}
