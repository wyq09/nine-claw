import { lazy, Suspense, type RefObject, type ChangeEvent, type ClipboardEvent } from 'react'
import type {
  AgentBuilderDraft,
  AgentInput,
  AgentRecord,
  AgentWorkspaceBundle,
  AgentWorkspaceFile,
  AppearanceSettings,
  BotChannelId,
  BotConfig,
  BotDefinition,
  ConversationAgentSnapshot,
  GeneralSettings,
  HistoryItem,
  InstalledSkillItem,
  PersistedChatAttachment,
  ProviderConfig,
  ViewKey,
} from '../../types'
import type { BotStatusEvent } from '../../lib/piClient'
import { ChatView } from '../chat/ChatWorkspace'

const AgentsView = lazy(async () => {
  const module = await import('../agents/AgentsView')
  return { default: module.AgentsView }
})

const TasksView = lazy(async () => {
  const module = await import('../pages/LibraryAndTasks')
  return { default: module.TasksView }
})

export type SessionLlmSelectOption = { value: string; label: string }

export type NineClawRouteOutletProps = {
  view: ViewKey
  agentBuilderActionBusyId: string
  agentBuilderActionError: string
  agentBuilderActionNotice: string
  agentBuilderActionTargetId: string
  composerClearRef: RefObject<(() => void) | null>
  composerDraftBackupRef: RefObject<string>
  chatGateError: string
  piError: string
  appearanceSettings: AppearanceSettings
  loading: boolean
  runningHistoryIds: string[]
  streamingHistoryIds: string[]
  activeHistoryId: string | null
  onAbort: () => void
  composerAttachmentError: string
  composerAttachmentInputRef: RefObject<HTMLInputElement | null>
  composerAttachmentUploading: boolean
  composerAttachments: PersistedChatAttachment[]
  onChatAgentBuilderCreate: (draft: AgentBuilderDraft, actionId: string) => void | Promise<void>
  onComposerAttachmentInputChange: (event: ChangeEvent<HTMLInputElement>) => void
  onComposerClearAttachments: () => void
  onComposerPaste: (event: ClipboardEvent<HTMLTextAreaElement>) => void
  onComposerPickAttachment: () => void
  onComposerRemoveAttachment: (id: string) => void
  onComposerClearAttachmentError: () => void
  onChatSubmit: (text: string) => void | Promise<void>
  activeChatAgent: ConversationAgentSnapshot | null
  submitShortcut: GeneralSettings['submitShortcut']
  activeHistoryItem: HistoryItem | null
  sessionLlmSelectOptionsWithFallback: SessionLlmSelectOption[]
  runtimeReady: boolean
  runtimeBlockingReason: string | null
  sessionContextProviderConfig: Pick<ProviderConfig, 'maxContextTokens'> | null
  installedSkills: InstalledSkillItem[]
  editableAgents: AgentRecord[]
  onOpenAgentEditor: (agentId: string) => void
  agentEditorDraft: AgentInput | null
  agentBotBindingDialogOpen: boolean
  agentDeleteConfirmOpen: boolean
  agentDeleteConfirmText: string
  agentEditorOpen: boolean
  agentFormError: string
  agentFormNotice: string
  agentRefreshing: boolean
  agentSaving: boolean
  selectedManagedBotConfigs: Record<BotChannelId, BotConfig>
  botLoading: boolean
  botStatusLogForSelected: BotStatusEvent[]
  agentWorkspaceBundle: AgentWorkspaceBundle | null
  agentWorkspaceDialogError: string
  agentWorkspaceDialogLoading: boolean
  agentWorkspaceFileLoading: boolean
  agentWorkspaceDialogOpen: boolean
  agentWorkspaceDraftContent: string
  agentWorkspaceSaveError: string
  agentWorkspaceSaveNotice: string
  agentWorkspaceSaving: boolean
  agentWorkspaceSelectedKey: string
  visibleAgents: AgentRecord[]
  defaultAgentId: string
  onAgentsViewCreateAgent: () => void
  onBotConfigChange: (channelId: BotChannelId, updates: Partial<BotConfig>) => void
  onCloseEditor: () => void
  onCloseBotBindingDialog: () => void
  onCloseDeleteAgentDialog: () => void
  onCloseWorkspaceDialog: () => void
  onConfirmDeleteAgent: () => void | Promise<void>
  onDraftWorkspaceContentChange: (value: string) => void
  onDraftChange: (updates: Partial<AgentInput>) => void
  onDeleteConfirmTextChange: (value: string) => void
  onRefreshAgents: () => void | Promise<void>
  onOpenBotBinding: (agentId: string) => void
  onOpenWorkspace: () => void | Promise<void>
  onOpenSkillPicker: () => void
  onRefreshWorkspace: () => void | Promise<void>
  onRequestDeleteAgent: () => void
  onSaveAgent: () => void | Promise<void>
  onSaveWorkspaceFile: (file: AgentWorkspaceFile, content: string) => void | Promise<void>
  onAgentSearchChange: (value: string) => void
  onManagedAgentSelect: (id: string) => void
  onSelectBot: (id: BotChannelId) => void
  onSelectWorkspaceFile: (key: string) => void | Promise<void>
  onSetDefaultAgent: () => void | Promise<void>
  onToggleSkill: (skillId: string) => void
  onLarkStart: () => void | Promise<void>
  onLarkStop: () => void | Promise<void>
  onWechatLogin: () => void | Promise<void>
  onWechatStart: () => void | Promise<void>
  onWechatStop: () => void | Promise<void>
  onRotatePeerSecret: () => void | Promise<void>
  onStartChatWithAgent: (agentId: string) => void
  agentSearch: string
  selectedManagedAgent: AgentRecord | null
  selectedManagedBotConfig: BotConfig
  selectedBotDefinition: BotDefinition
  selectedBotId: BotChannelId
  agentsLoading: boolean
  agentsError: string
  agentEditorMode: 'create' | 'edit'
  qrCodeUrl: string
  qrDialogOpen: boolean
  qrStatus: 'waiting' | 'scanned' | 'confirmed' | 'error'
  onSetBotLoading: (value: boolean) => void
  onSetQrDialogOpen: (value: boolean) => void
  managedAgentId: string
}

export const NineClawRouteOutlet = (props: NineClawRouteOutletProps) => {
  const {
    view,
    agentBuilderActionBusyId,
    agentBuilderActionError,
    agentBuilderActionNotice,
    agentBuilderActionTargetId,
    composerClearRef,
    composerDraftBackupRef,
    chatGateError,
    piError,
    appearanceSettings,
    loading,
    runningHistoryIds,
    streamingHistoryIds,
    activeHistoryId,
    onAbort,
    composerAttachmentError,
    composerAttachmentInputRef,
    composerAttachmentUploading,
    composerAttachments,
    onChatAgentBuilderCreate,
    onComposerAttachmentInputChange,
    onComposerClearAttachments,
    onComposerPaste,
    onComposerPickAttachment,
    onComposerRemoveAttachment,
    onComposerClearAttachmentError,
    onChatSubmit,
    activeChatAgent,
    submitShortcut,
    activeHistoryItem,
    sessionLlmSelectOptionsWithFallback,
    runtimeReady,
    runtimeBlockingReason,
    sessionContextProviderConfig,
    installedSkills,
    editableAgents,
    onOpenAgentEditor,
    agentEditorDraft,
    agentBotBindingDialogOpen,
    agentDeleteConfirmOpen,
    agentDeleteConfirmText,
    agentEditorOpen,
    agentFormError,
    agentFormNotice,
    agentRefreshing,
    agentSaving,
    selectedManagedBotConfigs,
    botLoading,
    botStatusLogForSelected,
    agentWorkspaceBundle,
    agentWorkspaceDialogError,
    agentWorkspaceDialogLoading,
    agentWorkspaceFileLoading,
    agentWorkspaceDialogOpen,
    agentWorkspaceDraftContent,
    agentWorkspaceSaveError,
    agentWorkspaceSaveNotice,
    agentWorkspaceSaving,
    agentWorkspaceSelectedKey,
    visibleAgents,
    defaultAgentId,
    onAgentsViewCreateAgent,
    onBotConfigChange,
    onCloseEditor,
    onCloseBotBindingDialog,
    onCloseDeleteAgentDialog,
    onCloseWorkspaceDialog,
    onConfirmDeleteAgent,
    onDraftWorkspaceContentChange,
    onDraftChange,
    onDeleteConfirmTextChange,
    onRefreshAgents,
    onOpenBotBinding,
    onOpenWorkspace,
    onOpenSkillPicker,
    onRefreshWorkspace,
    onRequestDeleteAgent,
    onSaveAgent,
    onSaveWorkspaceFile,
    onAgentSearchChange,
    onManagedAgentSelect,
    onSelectBot,
    onSelectWorkspaceFile,
    onSetDefaultAgent,
    onToggleSkill,
    onLarkStart,
    onLarkStop,
    onWechatLogin,
    onWechatStart,
    onWechatStop,
    onRotatePeerSecret,
    onStartChatWithAgent,
    agentSearch,
    selectedManagedAgent,
    selectedManagedBotConfig,
    selectedBotDefinition,
    selectedBotId,
    agentsLoading,
    agentsError,
    agentEditorMode,
    qrCodeUrl,
    qrDialogOpen,
    qrStatus,
    onSetBotLoading,
    onSetQrDialogOpen,
    managedAgentId,
  } = props

  const routeFallback = (
    <div className="page-shell task-center-page task-linear-page">
      <div className="task-center-body task-linear-body">
        <section className="task-center-panel task-center-panel-empty task-linear-empty">
          <p className="task-center-empty-title">正在打开页面…</p>
          <p className="task-center-empty-desc">首屏之外的模块会按需加载。</p>
        </section>
      </div>
    </div>
  )

  if (view === 'chat') {
    return (
      <ChatView
        agentBuilderActionBusyId={agentBuilderActionBusyId}
        agentBuilderActionError={agentBuilderActionError}
        agentBuilderActionNotice={agentBuilderActionNotice}
        agentBuilderActionTargetId={agentBuilderActionTargetId}
        composerClearRef={composerClearRef}
        composerDraftBackupRef={composerDraftBackupRef}
        error={chatGateError || piError}
        showExecutionRail={appearanceSettings.showExecutionRail}
        showThinkingProcess={appearanceSettings.showThinkingProcess}
        globalBusy={loading}
        runningHistoryIds={runningHistoryIds}
        streamingHistoryIds={streamingHistoryIds}
        activeHistoryId={activeHistoryId ?? ''}
        onAbort={onAbort}
        attachmentError={composerAttachmentError}
        attachmentInputRef={composerAttachmentInputRef}
        attachmentUploading={composerAttachmentUploading}
        composerAttachments={composerAttachments}
        onCreateAgentDraft={onChatAgentBuilderCreate}
        onComposerAttachmentInputChange={onComposerAttachmentInputChange}
        onComposerClearAttachments={onComposerClearAttachments}
        onComposerPaste={onComposerPaste}
        onComposerPickAttachment={onComposerPickAttachment}
        onComposerRemoveAttachment={onComposerRemoveAttachment}
        onComposerClearAttachmentError={onComposerClearAttachmentError}
        onSubmit={(text) => {
          void onChatSubmit(text)
        }}
        selectedAgent={activeChatAgent}
        submitShortcut={submitShortcut}
        activeHistoryItem={activeHistoryItem}
        runtimeReady={runtimeReady}
        runtimeBlockingReason={runtimeBlockingReason}
        sessionContextProviderConfig={sessionContextProviderConfig}
      />
    )
  }

  if (view === 'tasks') {
    return (
      <Suspense fallback={routeFallback}>
        <TasksView agents={editableAgents} onOpenAgent={onOpenAgentEditor} />
      </Suspense>
    )
  }

  return (
    <Suspense fallback={routeFallback}>
      <AgentsView
        agentDraft={agentEditorDraft}
        agentBotBindingDialogOpen={agentBotBindingDialogOpen}
        agentDeleteConfirmOpen={agentDeleteConfirmOpen}
        agentDeleteConfirmText={agentDeleteConfirmText}
        agentEditorOpen={agentEditorOpen}
        agentFormError={agentFormError}
        agentFormNotice={agentFormNotice}
        agentRefreshing={agentRefreshing}
        agentSaving={agentSaving}
        botConfigs={selectedManagedBotConfigs}
        botLoading={botLoading}
        botStatusLog={botStatusLogForSelected}
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
        agents={visibleAgents}
        allSkills={installedSkills}
        defaultAgentId={defaultAgentId}
        onCreateAgent={onAgentsViewCreateAgent}
        onBotConfigChange={onBotConfigChange}
        onCloseEditor={onCloseEditor}
        onCloseBotBindingDialog={onCloseBotBindingDialog}
        onCloseDeleteAgentDialog={onCloseDeleteAgentDialog}
        onCloseWorkspaceDialog={onCloseWorkspaceDialog}
        onConfirmDeleteAgent={onConfirmDeleteAgent}
        onDraftWorkspaceContentChange={onDraftWorkspaceContentChange}
        onDraftChange={onDraftChange}
        onDeleteConfirmTextChange={onDeleteConfirmTextChange}
        onRefreshAgents={onRefreshAgents}
        onOpenEditor={onOpenAgentEditor}
        onOpenBotBinding={onOpenBotBinding}
        onOpenWorkspace={onOpenWorkspace}
        onOpenSkillPicker={onOpenSkillPicker}
        onRefreshWorkspace={onRefreshWorkspace}
        onRequestDeleteAgent={onRequestDeleteAgent}
        onSaveAgent={onSaveAgent}
        onSaveWorkspaceFile={onSaveWorkspaceFile}
        onSearch={onAgentSearchChange}
        onSelectAgent={onManagedAgentSelect}
        onSelectBot={onSelectBot}
        onSelectWorkspaceFile={onSelectWorkspaceFile}
        onSetDefaultAgent={onSetDefaultAgent}
        onToggleSkill={onToggleSkill}
        onLarkStart={onLarkStart}
        onLarkStop={onLarkStop}
        onWechatLogin={onWechatLogin}
        onWechatStart={onWechatStart}
        onWechatStop={onWechatStop}
        onRotatePeerSecret={onRotatePeerSecret}
        onStartChatWithAgent={onStartChatWithAgent}
        searchValue={agentSearch}
        selectedAgent={selectedManagedAgent}
        selectedBotConfig={selectedManagedBotConfig}
        selectedBotDefinition={selectedBotDefinition}
        selectedBotId={selectedBotId}
        loading={agentsLoading}
        error={agentsError}
        mode={agentEditorMode}
        modelOptions={sessionLlmSelectOptionsWithFallback}
        qrCodeUrl={qrCodeUrl}
        qrDialogOpen={qrDialogOpen}
        qrStatus={qrStatus}
        setBotLoading={onSetBotLoading}
        setQrDialogOpen={onSetQrDialogOpen}
        managedAgentId={managedAgentId}
      />
    </Suspense>
  )
}
