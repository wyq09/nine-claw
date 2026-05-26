import type { Dispatch, MouseEvent, ReactNode, SetStateAction } from 'react'
import { useEffect, useMemo, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { AppIcon } from '../../components/AppIcon'
import { SettingsModal } from '../../components/SettingsModal'
import { llmTraceList, onLlmTraceEvent } from '../../lib/llmTraceClient'
import { sessionLlmLogGet, onSessionLlmLogEvent } from '../../lib/sessionLlmLogClient'
import { groupHistoryIntoSidebarBuckets } from '../../lib/historySidebarBuckets'
import { useHistorySidebarBucketsExpanded } from '../../hooks/useHistorySidebarBucketsExpanded'
import type {
  AgentInput,
  AppearanceSettings,
  GeneralSettings,
  HistoryItem,
  InstalledSkillItem,
  ProviderConfig,
  ProviderDefinition,
  ProviderId,
  SettingsTab,
  ViewKey,
} from '../../types'
import type {
  ImageGenerationSystemConfig,
  ImageProviderConfig,
  ImageProviderDefinition,
} from '../../types/imageGeneration'
import type { ResourcesViewProps, SkillsViewProps } from '../pages/LibraryAndTasks'
import {
  SIDEBAR_FOOTER_SHORTCUTS_ENABLED,
  sessionLlmDecode,
  sessionLlmEncode,
  summarizePrompt,
} from '../lib'
import { getHistorySidebarCardMeta } from '../lib/historySidebarCardMeta'
import { SidebarButton } from '../chat/ChatWorkspace'
import { NewSessionDialog, SkillInstallDialog } from '../pages/SessionDialogs'
import { AgentSkillPickerDialog } from '../agents/AgentDialogsBundle'
import { openLlmTracePopout } from '../lib/llmTracePopout'
import { openSessionLlmLogPopout } from '../lib/sessionLlmLogPopout'
import { matchesTraceScope } from '../workspaces/panels/llmTraceModel'
import { matchesSessionLogScope } from '../workspaces/panels/sessionLlmLogModel'
import type { SessionLlmSelectOption } from './NineClawRouteOutlet'

export type NineClawAppChromeProps = {
  routeOutlet: ReactNode
  chatProviderLabel: string
  onSessionLlmSelectChange: (value: string) => void
  appearanceSettings: AppearanceSettings
  effectiveSidebarCollapsed: boolean
  shouldHideSidebar: boolean
  sidebarOverlayOpen: boolean
  setSidebarOverlayOpen: Dispatch<SetStateAction<boolean>>
  setAppearanceSettings: Dispatch<SetStateAction<AppearanceSettings>>
  view: ViewKey
  onNewSession: () => void
  onViewChange: (next: ViewKey) => void
  history: HistoryItem[]
  onClearHistory: () => void
  historyBusy: boolean
  historySearch: string
  setHistorySearch: Dispatch<SetStateAction<string>>
  visibleHistory: HistoryItem[]
  activeHistoryId: string | null
  onHistorySelect: (id: string) => void
  onHistoryContextMenu: (event: MouseEvent<HTMLButtonElement>, item: HistoryItem) => void
  onOpenSettings: (tab: SettingsTab) => void
  skillInstallDialogOpen: boolean
  skillInstallError: string
  skillInstallLink: string
  skillInstallLaunching: boolean
  setSkillInstallLink: Dispatch<SetStateAction<string>>
  setSkillInstallError: Dispatch<SetStateAction<string>>
  onCloseSkillInstallDialog: () => void
  onConfirmSkillInstall: () => void | Promise<void>
  historyContextMenu: {
    sessionId: string
    title: string
    x: number
    y: number
    canDelete: boolean
  } | null
  setHistoryContextMenu: Dispatch<
    SetStateAction<{
      sessionId: string
      title: string
      x: number
      y: number
      canDelete: boolean
    } | null>
  >
  onRequestDeleteHistoryItem: (sessionId: string) => void
  historyDeleteTarget: { sessionId: string; title: string } | null
  setHistoryDeleteTarget: Dispatch<SetStateAction<{ sessionId: string; title: string } | null>>
  historyDeleteBusy: boolean
  onConfirmDeleteHistoryItem: () => void | Promise<void>
  newSessionDialogOpen: boolean
  agents: import('../../types').AgentRecord[]
  agentsLoading: boolean
  sessionLlmSelectOptionsWithFallback: SessionLlmSelectOption[]
  newSessionAgentId: string
  newSessionLlm: { providerId: ProviderId; model: string } | null
  sessionLlmEncodedCurrent: string
  onNewSessionAgentChange: (agentId: string) => void
  setNewSessionLlm: Dispatch<SetStateAction<{ providerId: ProviderId; model: string } | null>>
  onCloseNewSessionDialog: () => void
  onConfirmNewSession: () => void
  agentSkillPickerOpen: boolean
  agentEditorOpen: boolean
  agentEditorDraft: AgentInput | null
  installedSkills: InstalledSkillItem[]
  agentSkillSearch: string
  setAgentSkillSearch: Dispatch<SetStateAction<string>>
  visibleAgentSkillOptions: import('../../types').InstalledSkillItem[]
  onCloseAgentSkillPicker: () => void
  onToggleAgentSkill: (skillId: string) => void
  settingsOpen: boolean
  activeProviderBadge: string
  mergedProviderDefinitions: ProviderDefinition[]
  generalSettings: GeneralSettings
  imageGenerationSystem: ImageGenerationSystemConfig
  imageProviderConfigs: Record<string, ImageProviderConfig>
  imageProviderDefinitions: ImageProviderDefinition[]
  onAddCustomProvider: (name: string, description: string, apiFormat: import('../../types').ProviderApiFormat) => void
  onSaveImageGenerationSettings: (
    imageProviderConfigs: Record<string, ImageProviderConfig>,
    imageGenerationSystem: ImageGenerationSystemConfig,
  ) => Promise<void>
  onProviderConfigChange: (providerId: ProviderId, updates: Partial<ProviderConfig>) => void
  onCloseSettings: () => void
  onRemoveCustomProvider: (providerId: ProviderId) => void
  onSelectProvider: (id: ProviderId) => void
  onSelectSettingsTab: (tab: SettingsTab) => void
  providerConfigs: Record<string, ProviderConfig>
  selectedProviderConfig: ProviderConfig
  selectedProviderDefinition: ProviderDefinition
  selectedProviderId: ProviderId
  setAppearanceSettingsForModal: Dispatch<SetStateAction<AppearanceSettings>>
  setGeneralSettings: Dispatch<SetStateAction<GeneralSettings>>
  settingsTab: SettingsTab
  settingsSkillsLibrary: SkillsViewProps
  settingsResourcesLibrary: ResourcesViewProps
  settingsMemoryAgents: import('../../types').AgentRecord[]
  settingsMemoryDefaultAgentId: string
  /** 当前可调试的会话 id（含团队空间内会话，与侧栏「单独会话」过滤无关） */
  llmTraceSessionId: string
  /** 在团队空间聊天页时传入工作空间 id，供调试窗口按作用域拉取 */
  llmTraceWorkspaceId: string | null
}

export const NineClawAppChrome = (props: NineClawAppChromeProps) => {
  const {
    routeOutlet,
    chatProviderLabel,
    onSessionLlmSelectChange,
    appearanceSettings,
    effectiveSidebarCollapsed,
    shouldHideSidebar,
    sidebarOverlayOpen,
    setSidebarOverlayOpen,
    setAppearanceSettings,
    view,
    onNewSession,
    onViewChange,
    history,
    onClearHistory,
    historyBusy,
    historySearch,
    setHistorySearch,
    visibleHistory,
    activeHistoryId,
    onHistorySelect,
    onHistoryContextMenu,
    onOpenSettings,
    skillInstallDialogOpen,
    skillInstallError,
    skillInstallLink,
    skillInstallLaunching,
    setSkillInstallLink,
    setSkillInstallError,
    onCloseSkillInstallDialog,
    onConfirmSkillInstall,
    historyContextMenu,
    setHistoryContextMenu,
    onRequestDeleteHistoryItem,
    historyDeleteTarget,
    setHistoryDeleteTarget,
    historyDeleteBusy,
    onConfirmDeleteHistoryItem,
    newSessionDialogOpen,
    agents,
    agentsLoading,
    sessionLlmSelectOptionsWithFallback,
    newSessionAgentId,
    newSessionLlm,
    sessionLlmEncodedCurrent,
    onNewSessionAgentChange,
    setNewSessionLlm,
    onCloseNewSessionDialog,
    onConfirmNewSession,
    agentSkillPickerOpen,
    agentEditorOpen,
    agentEditorDraft,
    installedSkills,
    agentSkillSearch,
    setAgentSkillSearch,
    visibleAgentSkillOptions,
    onCloseAgentSkillPicker,
    onToggleAgentSkill,
    settingsOpen,
    activeProviderBadge,
    mergedProviderDefinitions,
    generalSettings,
    imageGenerationSystem,
    imageProviderConfigs,
    imageProviderDefinitions,
    onAddCustomProvider,
    onSaveImageGenerationSettings,
    onProviderConfigChange,
    onCloseSettings,
    onRemoveCustomProvider,
    onSelectProvider,
    onSelectSettingsTab,
    providerConfigs,
    selectedProviderConfig,
    selectedProviderDefinition,
    selectedProviderId,
    setAppearanceSettingsForModal,
    setGeneralSettings,
    settingsTab,
    settingsSkillsLibrary,
    settingsResourcesLibrary,
    settingsMemoryAgents,
    settingsMemoryDefaultAgentId,
    llmTraceSessionId: llmTraceSessionIdProp,
    llmTraceWorkspaceId,
  } = props
  const historyBuckets = useMemo(() => groupHistoryIntoSidebarBuckets(visibleHistory), [visibleHistory])
  const { mergedBucketOpen, toggleBucket } = useHistorySidebarBucketsExpanded(historyBuckets, activeHistoryId)

  const [standaloneTraceCount, setStandaloneTraceCount] = useState(0)
  const [sessionLogAvailable, setSessionLogAvailable] = useState(false)
  const traceSessionId = llmTraceSessionIdProp.trim()

  useEffect(() => {
    const root = document.documentElement
    const isMacUi = typeof navigator !== 'undefined' && /Mac|iPhone|iPad|iPod/i.test(navigator.userAgent)
    if (isMacUi) {
      root.classList.add('platform-macos')
    }
    return () => {
      root.classList.remove('platform-macos')
    }
  }, [])

  useEffect(() => {
    if (!traceSessionId) {
      return
    }

    let mounted = true
    let unsubscribe: (() => void) | null = null
    void llmTraceList({
      workspaceId: llmTraceWorkspaceId,
      sessionId: traceSessionId,
      days: 3,
      limit: 200,
    })
      .then((list) => {
        if (mounted) {
          setStandaloneTraceCount(list.length)
        }
      })
      .catch(() => {
        if (mounted) {
          setStandaloneTraceCount(0)
        }
      })

    void onLlmTraceEvent((payload) => {
      if (!matchesTraceScope(payload.entry, { workspaceId: llmTraceWorkspaceId, sessionId: traceSessionId })) {
        return
      }
      if (payload.phase === 'started' && mounted) {
        setStandaloneTraceCount((count) => count + 1)
      }
    }).then((unlisten) => {
      unsubscribe = unlisten
    })

    return () => {
      mounted = false
      unsubscribe?.()
    }
  }, [llmTraceWorkspaceId, traceSessionId])

  useEffect(() => {
    if (!traceSessionId) {
      setSessionLogAvailable(false)
      return
    }

    let mounted = true
    let unsubscribe: (() => void) | null = null
    void sessionLlmLogGet({
      workspaceId: llmTraceWorkspaceId,
      sessionId: traceSessionId,
    })
      .then(() => {
        if (mounted) setSessionLogAvailable(true)
      })
      .catch(() => {
        if (mounted) setSessionLogAvailable(false)
      })

    void onSessionLlmLogEvent((payload) => {
      if (!matchesSessionLogScope(payload, { workspaceId: llmTraceWorkspaceId, sessionId: traceSessionId })) return
      if (mounted) setSessionLogAvailable(true)
    }).then((unlisten) => {
      unsubscribe = unlisten
    })

    return () => {
      mounted = false
      unsubscribe?.()
    }
  }, [llmTraceWorkspaceId, traceSessionId])

  const handleTitlebarSidebarToggle = () => {
    if (shouldHideSidebar) {
      setSidebarOverlayOpen((current) => !current)
      return
    }
    setAppearanceSettings((previous) => ({
      ...previous,
      sidebarCollapsed: !previous.sidebarCollapsed,
    }))
  }

  const titlebarSidebarLabel = shouldHideSidebar
    ? sidebarOverlayOpen
      ? '关闭导航'
      : '打开导航'
    : effectiveSidebarCollapsed
      ? '展开侧栏'
      : '折叠侧栏'

  const handleTitlebarMouseDown = (event: MouseEvent<HTMLElement>) => {
    if (event.button !== 0 || event.defaultPrevented) {
      return
    }

    const target = event.target
    if (!(target instanceof HTMLElement)) {
      return
    }

    if (target.closest('[data-titlebar-no-drag="true"]')) {
      return
    }

    event.preventDefault()
    void getCurrentWindow().startDragging()
  }

  return (
    <>
      <main
        className={[
          'app-shell',
          `theme-${appearanceSettings.themeMode}`,
          appearanceSettings.compactSidebar ? 'compact-sidebar' : '',
          effectiveSidebarCollapsed ? 'sidebar-is-collapsed' : '',
          shouldHideSidebar ? 'sidebar-hidden-mode' : '',
          sidebarOverlayOpen ? 'sidebar-overlay-open' : '',
          appearanceSettings.preferReducedMotion ? 'reduce-motion' : '',
        ]
          .filter(Boolean)
          .join(' ')}
      >
        {shouldHideSidebar && sidebarOverlayOpen ? (
          <button
            type="button"
            className="sidebar-overlay-backdrop"
            aria-label="关闭侧栏"
            onClick={() => setSidebarOverlayOpen(false)}
          />
        ) : null}

        <header className="app-window-titlebar" data-tauri-drag-region onMouseDown={handleTitlebarMouseDown}>
          <div className="app-window-titlebar-inner" data-tauri-drag-region>
            <button
              type="button"
              className={`collapse-button app-titlebar-sidebar-toggle ${effectiveSidebarCollapsed ? 'collapsed' : ''}`}
              data-tauri-drag-region="false"
              data-titlebar-no-drag="true"
              aria-label={titlebarSidebarLabel}
              title={titlebarSidebarLabel}
              onClick={handleTitlebarSidebarToggle}
            >
              <AppIcon name="panel" size={16} />
            </button>
            {/* Tauri drag.js 只看 event.target，不向上找；此处必须自带属性，否则点中间空白不触发拖动 */}
            <div className="app-window-titlebar-drag-spacer" data-tauri-drag-region aria-hidden="true" />
            <div className="app-window-titlebar-end" data-tauri-drag-region="false" data-titlebar-no-drag="true">
              {traceSessionId ? (
                <button
                  type="button"
                  className={`titlebar-trace-button${sessionLogAvailable ? ' active' : ''}`}
                  data-titlebar-no-drag="true"
                  onClick={() => void openSessionLlmLogPopout(llmTraceWorkspaceId, traceSessionId)}
                  title="在独立窗口查看当前 session 的文本日志"
                >
                  <AppIcon name="folder" size={13} />
                  <span>日志</span>
                </button>
              ) : null}
              {traceSessionId ? (
                <button
                  type="button"
                  className="titlebar-trace-button"
                  data-titlebar-no-drag="true"
                  onClick={() => void openLlmTracePopout(llmTraceWorkspaceId, traceSessionId)}
                  title="在独立窗口查看当前会话的 LLM 调用链"
                >
                  <AppIcon name="wrench" size={13} />
                  <span>{standaloneTraceCount}</span>
                </button>
              ) : null}
              <div className="titlebar-model-wrap" title={chatProviderLabel} data-titlebar-no-drag="true">
                <label className="titlebar-model-field" data-titlebar-no-drag="true">
                  <select
                    id="titlebar-session-llm"
                    className="titlebar-model-select"
                    data-titlebar-no-drag="true"
                    value={
                      sessionLlmSelectOptionsWithFallback.some((o) => o.value === (sessionLlmEncodedCurrent ?? ''))
                        ? (sessionLlmEncodedCurrent ?? '')
                        : sessionLlmSelectOptionsWithFallback[0]?.value ?? ''
                    }
                    disabled={sessionLlmSelectOptionsWithFallback.length === 0}
                    onChange={(event) => onSessionLlmSelectChange(event.target.value)}
                    aria-label="本会话使用的供应商与模型"
                  >
                    {sessionLlmSelectOptionsWithFallback.length === 0 ? (
                      <option value="">暂无已配置的模型，请先在设置中填写供应商</option>
                    ) : (
                      sessionLlmSelectOptionsWithFallback.map((opt) => (
                        <option key={opt.value} value={opt.value}>
                          {opt.label}
                        </option>
                      ))
                    )}
                  </select>
                </label>
              </div>
            </div>
          </div>
        </header>

        <aside className={`sidebar ${effectiveSidebarCollapsed ? 'collapsed' : ''} ${!shouldHideSidebar || sidebarOverlayOpen ? 'visible' : ''}`}>
          <div className="sidebar-brand">
            <div className="sidebar-brand-main">
              <div className="sidebar-brand-mark">9</div>
              <div className="sidebar-brand-copy">
                <strong>NineClaw</strong>
                <span>多智能体工作台</span>
              </div>
            </div>
          </div>

          <div className="sidebar-section">
            <div className="sidebar-section-title">导航</div>
            <div className="primary-nav">
              <SidebarButton active={view === 'chat'} icon="plus" label="新会话" onClick={onNewSession} />
              <SidebarButton
                active={view === 'agents'}
                icon="bot"
                label="智能体管理"
                onClick={() => onViewChange('agents')}
              />
              <SidebarButton
                active={view === 'tasks'}
                icon="clock"
                label="任务中心"
                onClick={() => onViewChange('tasks')}
              />
              <SidebarButton
                active={view === 'workspaces'}
                icon="users"
                label="团队空间"
                onClick={() => onViewChange('workspaces')}
              />
            </div>
          </div>

          {history.length > 0 ? (
            <div className="task-history">
              <div className="task-header">
                <h2>历史会话</h2>
                <button type="button" className="link-button" onClick={onClearHistory} disabled={historyBusy}>
                  清空
                </button>
              </div>

              <label className="history-search-field">
                <AppIcon name="search" size={16} />
                <input
                  type="search"
                  value={historySearch}
                  onChange={(event) => setHistorySearch(event.target.value)}
                  placeholder="搜索历史会话"
                  aria-label="搜索历史会话"
                />
              </label>

              <div className="history-list">
                {visibleHistory.length > 0 ? (
                  historyBuckets.map((bucket) => (
                    <section key={bucket.key} className="history-bucket">
                      <button
                        type="button"
                        className="history-bucket-head"
                        aria-expanded={mergedBucketOpen[bucket.key]}
                        onClick={() => toggleBucket(bucket.key)}
                      >
                        <span
                          className={`history-bucket-chevron-wrap${mergedBucketOpen[bucket.key] ? ' is-open' : ''}`}
                        >
                          <AppIcon name="chevron-down" size={14} />
                        </span>
                        <span className="history-bucket-label">{bucket.label}</span>
                        <span className="history-bucket-count">{bucket.items.length}</span>
                      </button>
                      {mergedBucketOpen[bucket.key] ? (
                        <div className="history-bucket-items" role="list">
                          {bucket.items.map((item) => {
                            const meta = getHistorySidebarCardMeta(item.status, item.updatedAt || item.createdAt)
                            return (
                              <button
                                key={item.id}
                                type="button"
                                role="listitem"
                                className={`history-card ${item.id === activeHistoryId ? 'active' : ''}`}
                                onClick={() => onHistorySelect(item.id)}
                                onContextMenu={(event) => onHistoryContextMenu(event, item)}
                                title={item.title}
                              >
                                <span className="history-card-body">
                                  <span className="history-card-title">{item.title?.trim() || '未命名会话'}</span>
                                  {item.agent ? (
                                    <span className="history-card-agent">{item.agent.name}</span>
                                  ) : null}
                                  <span className="history-card-meta-row">
                                    <span className={`history-card-meta-icon ${meta.tone}`}>
                                      <AppIcon name={meta.icon} size={12} />
                                    </span>
                                    <span className={`history-card-time ${meta.tone}`}>{meta.label}</span>
                                  </span>
                                </span>
                              </button>
                            )
                          })}
                        </div>
                      ) : null}
                    </section>
                  ))
                ) : (
                  <div className="empty-history-card compact">
                    <div className="empty-history-title">没有匹配的会话</div>
                    <span>试试搜索会话标题、提问内容或回答里的关键词。</span>
                  </div>
                )}
              </div>
            </div>
          ) : null}

          <div className="sidebar-footer">
            {SIDEBAR_FOOTER_SHORTCUTS_ENABLED ? (
              <>
                <button type="button" className="avatar-badge" aria-label="用户">
                  U
                </button>
                <button type="button" className="footer-icon-button" onClick={() => onViewChange('chat')} aria-label="最近会话">
                  <AppIcon name="clock" size={18} />
                </button>
                <button type="button" className="footer-icon-button" onClick={() => onViewChange('agents')} aria-label="AI 组织管理">
                  <AppIcon name="network" size={18} />
                </button>
              </>
            ) : null}
            <button type="button" className="footer-icon-button" onClick={() => onOpenSettings('general')} aria-label="设置">
              <AppIcon name="settings" size={18} />
            </button>
          </div>
        </aside>

        <section className="content-panel">
          {routeOutlet}
        </section>
      </main>

      {skillInstallDialogOpen ? (
        <SkillInstallDialog
          error={skillInstallError}
          link={skillInstallLink}
          loading={skillInstallLaunching}
          onChangeLink={(value) => {
            setSkillInstallLink(value)
            if (skillInstallError) {
              setSkillInstallError('')
            }
          }}
          onClose={onCloseSkillInstallDialog}
          onConfirm={async () => {
            await Promise.resolve(onConfirmSkillInstall())
          }}
        />
      ) : null}

      {historyContextMenu ? (
        <>
          <button
            type="button"
            className="context-menu-backdrop"
            aria-label="关闭会话菜单"
            onClick={() => setHistoryContextMenu(null)}
          />
          <div
            className="history-context-menu"
            role="menu"
            style={{ left: historyContextMenu.x, top: historyContextMenu.y }}
          >
            <button
              type="button"
              className="history-context-menu-item danger"
              role="menuitem"
              onClick={() => onRequestDeleteHistoryItem(historyContextMenu.sessionId)}
              disabled={!historyContextMenu.canDelete}
              title={historyContextMenu.canDelete ? `删除「${historyContextMenu.title}」` : '当前会话仍在生成，暂时不能删除'}
            >
              <AppIcon name="trash" size={16} />
              <span>{historyContextMenu.canDelete ? '删除会话' : '会话生成中，暂不可删'}</span>
            </button>
          </div>
        </>
      ) : null}

      {historyDeleteTarget ? (
        <div
          className="confirm-dialog-overlay"
          role="presentation"
          onClick={() => {
            if (!historyDeleteBusy) {
              setHistoryDeleteTarget(null)
            }
          }}
        >
          <div
            className="confirm-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="history-delete-confirm-title"
            onClick={(event) => event.stopPropagation()}
          >
            <h3 id="history-delete-confirm-title">删除历史会话</h3>
            <p>确定要删除「{summarizePrompt(historyDeleteTarget.title, 32)}」吗？删除后无法恢复。</p>
            <div className="confirm-dialog-actions">
              <button
                type="button"
                className="outline-button"
                onClick={() => setHistoryDeleteTarget(null)}
                disabled={historyDeleteBusy}
              >
                取消
              </button>
              <button
                type="button"
                className="outline-button confirm-dialog-delete"
                onClick={onConfirmDeleteHistoryItem}
                disabled={historyDeleteBusy}
              >
                {historyDeleteBusy ? '删除中…' : '删除'}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {newSessionDialogOpen ? (
        <NewSessionDialog
          agents={agents}
          loading={agentsLoading}
          modelOptions={sessionLlmSelectOptionsWithFallback}
          selectedAgentId={newSessionAgentId}
          selectedModelValue={
            newSessionLlm
              ? sessionLlmEncode(newSessionLlm.providerId, newSessionLlm.model)
              : sessionLlmEncodedCurrent
          }
          onChangeAgent={onNewSessionAgentChange}
          onChangeModel={(value) => {
            const parsed = sessionLlmDecode(value)
            if (parsed) {
              setNewSessionLlm(parsed)
            }
          }}
          onClose={onCloseNewSessionDialog}
          onConfirm={onConfirmNewSession}
        />
      ) : null}

      {agentSkillPickerOpen && agentEditorOpen && agentEditorDraft ? (
        <AgentSkillPickerDialog
          allSkillCount={installedSkills.length}
          searchValue={agentSkillSearch}
          selectedSkillIds={agentEditorDraft.skillIds}
          skills={visibleAgentSkillOptions}
          onClose={onCloseAgentSkillPicker}
          onSearch={setAgentSkillSearch}
          onToggleSkill={onToggleAgentSkill}
        />
      ) : null}

      {settingsOpen ? (
        <SettingsModal
          activeProviderBadge={activeProviderBadge}
          allProviderDefinitions={mergedProviderDefinitions}
          appearanceSettings={appearanceSettings}
          generalSettings={generalSettings}
          imageGenerationSystem={imageGenerationSystem}
          imageProviderConfigs={imageProviderConfigs}
          imageProviderDefinitions={imageProviderDefinitions}
          onAddCustomProvider={onAddCustomProvider}
          onSaveImageGenerationSettings={onSaveImageGenerationSettings}
          onProviderConfigChange={onProviderConfigChange}
          onClose={onCloseSettings}
          onRemoveCustomProvider={onRemoveCustomProvider}
          onSelectProvider={onSelectProvider}
          onSelectTab={onSelectSettingsTab}
          providerConfigs={providerConfigs}
          selectedProviderConfig={selectedProviderConfig}
          selectedProviderDefinition={selectedProviderDefinition}
          selectedProviderId={selectedProviderId}
          setAppearanceSettings={setAppearanceSettingsForModal}
          setGeneralSettings={setGeneralSettings}
          tab={settingsTab}
          skillsLibrary={settingsSkillsLibrary}
          resourcesLibrary={settingsResourcesLibrary}
          memoryAgents={settingsMemoryAgents}
          memoryDefaultAgentId={settingsMemoryDefaultAgentId}
        />
      ) : null}
    </>
  )
}
