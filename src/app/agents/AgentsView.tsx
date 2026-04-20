import { useState } from 'react'
import { open, save } from '@tauri-apps/plugin-dialog'
import { AppIcon } from '../../components/AppIcon'
import type {
  AgentInput,
  AgentRecord,
  AgentWorkspaceBundle,
  AgentWorkspaceFile,
  BotChannelId,
  BotConfig,
  InstalledSkillItem,
} from '../../types'
import type { BotStatusEvent } from '../../lib/piClient'
import { botDefinitions } from '../../mockData'
import { exportAgentPackage, importAgentPackage } from '../../lib/piClient'
import { getAgentColor } from '../lib'
import { AgentEditorDialog } from './AgentDialogsBundle'
import { AgentBotBindingDialog, AgentWorkspaceDialog } from './AgentChannelDialogs'

export type AgentsViewProps = {
  agentDraft: AgentInput | null
  agentBotBindingDialogOpen: boolean
  agentDeleteConfirmOpen: boolean
  agentDeleteConfirmText: string
  agentEditorOpen: boolean
  agentFormError: string
  agentFormNotice: string
  agentRefreshing: boolean
  agentSaving: boolean
  botConfigs: Record<string, BotConfig>
  botLoading: boolean
  botStatusLog: BotStatusEvent[]
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
  agents: AgentRecord[]
  allSkills: InstalledSkillItem[]
  defaultAgentId: string
  error: string
  loading: boolean
  mode: 'create' | 'edit'
  modelOptions: { value: string; label: string }[]
  qrCodeUrl: string
  qrDialogOpen: boolean
  qrStatus: 'waiting' | 'scanned' | 'confirmed' | 'error'
  onBotConfigChange: (channelId: BotChannelId, updates: Partial<BotConfig>) => void
  onCloseEditor: () => void
  onCloseBotBindingDialog: () => void
  onCloseDeleteAgentDialog: () => void
  onCloseWorkspaceDialog: () => void
  onConfirmDeleteAgent: () => void
  onCreateAgent: () => void
  onDraftChange: (updates: Partial<AgentInput>) => void
  onDeleteConfirmTextChange: (value: string) => void
  onDraftWorkspaceContentChange: (value: string) => void
  onRefreshAgents: () => void
  onOpenWorkspace: () => void
  onOpenSkillPicker: () => void
  onOpenBotBinding: (id: string) => void
  onOpenEditor: (id: string) => void
  onRequestDeleteAgent: () => void
  onRefreshWorkspace: () => void
  onSaveAgent: () => void
  onSaveWorkspaceFile: (file: AgentWorkspaceFile, content: string) => void | Promise<void>
  onSearch: (value: string) => void
  onSelectAgent: (id: string) => void
  onSelectBot: (id: BotChannelId) => void
  onSelectWorkspaceFile: (key: string) => void | Promise<void>
  setBotLoading: (loading: boolean) => void
  onSetDefaultAgent: () => void
  onToggleSkill: (skillId: string) => void
  onLarkStart: () => void
  onLarkStop: () => void
  onWechatLogin: () => void
  onWechatStart: () => void
  onWechatStop: () => void
  onRotatePeerSecret: () => void
  onStartChatWithAgent: (id: string) => void
  setQrDialogOpen: (open: boolean) => void
  searchValue: string
  selectedAgent: AgentRecord | null
  selectedBotConfig: BotConfig
  selectedBotDefinition: (typeof botDefinitions)[number]
  selectedBotId: BotChannelId
  managedAgentId: string
}

export function AgentsView({
  agentDraft,
  agentBotBindingDialogOpen,
  agentDeleteConfirmOpen,
  agentDeleteConfirmText,
  agentEditorOpen,
  agentFormError,
  agentFormNotice,
  agentRefreshing,
  agentSaving,
  botConfigs,
  botLoading,
  botStatusLog,
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
  agents,
  allSkills,
  defaultAgentId,
  error,
  loading,
  mode,
  modelOptions,
  qrCodeUrl,
  qrDialogOpen,
  qrStatus,
  onBotConfigChange,
  onCloseEditor,
  onCloseBotBindingDialog,
  onCloseDeleteAgentDialog,
  onCloseWorkspaceDialog,
  onConfirmDeleteAgent,
  onCreateAgent,
  onDraftChange,
  onDeleteConfirmTextChange,
  onDraftWorkspaceContentChange,
  onRefreshAgents,
  onOpenWorkspace,
  onOpenSkillPicker,
  onOpenBotBinding,
  onOpenEditor,
  onRequestDeleteAgent,
  onRefreshWorkspace,
  onSaveAgent,
  onSaveWorkspaceFile,
  onSearch,
  onSelectAgent,
  onSelectBot,
  onSelectWorkspaceFile,
  setBotLoading,
  onSetDefaultAgent,
  onToggleSkill,
  onLarkStart,
  onLarkStop,
  onWechatLogin,
  onWechatStart,
  onWechatStop,
  onRotatePeerSecret,
  onStartChatWithAgent,
  setQrDialogOpen,
  searchValue,
  selectedAgent,
  selectedBotConfig,
  selectedBotDefinition,
  selectedBotId,
  managedAgentId,
}: AgentsViewProps) {
  const studioCountLabel = loading ? '正在同步智能体…' : '已保存智能体'

  const [exportDialogOpen, setExportDialogOpen] = useState(false)
  /** 本次导出目标（单个智能体）；与列表选中可能同步，但以该状态为准避免竞态 */
  const [exportTargetAgent, setExportTargetAgent] = useState<AgentRecord | null>(null)
  const [exportIncludeSecrets, setExportIncludeSecrets] = useState(false)
  const [exportIncludeSharedRoot, setExportIncludeSharedRoot] = useState(false)
  const [packageBusy, setPackageBusy] = useState(false)
  const [packageNotice, setPackageNotice] = useState<{ kind: 'ok' | 'err'; text: string } | null>(null)

  const safeDefaultExportName = (name: string) => {
    const base = name.replace(/[\\/:*?"<>|]+/g, '_').trim() || 'agent'
    return `${base}.nineclaw-agent.zip`
  }

  const closeExportDialog = () => {
    if (packageBusy) {
      return
    }
    setExportDialogOpen(false)
    setExportTargetAgent(null)
  }

  const handleOpenExportDialog = () => {
    setPackageNotice(null)
    if (!selectedAgent) {
      setPackageNotice({ kind: 'err', text: '请先在列表中点选要导出的那一个智能体，或使用该行「导出包」按钮。' })
      return
    }
    setExportTargetAgent(selectedAgent)
    setExportDialogOpen(true)
  }

  const handleOpenExportDialogForAgent = (agent: AgentRecord) => {
    setPackageNotice(null)
    onSelectAgent(agent.id)
    setExportTargetAgent(agent)
    setExportDialogOpen(true)
  }

  const handleConfirmExport = async () => {
    if (!exportTargetAgent) {
      return
    }
    const destPath = await save({
      defaultPath: safeDefaultExportName(exportTargetAgent.name),
      filters: [{ name: 'NineClaw 智能体包', extensions: ['zip'] }],
    })
    if (destPath == null) {
      return
    }
    setPackageBusy(true)
    setPackageNotice(null)
    try {
      await exportAgentPackage({
        agentId: exportTargetAgent.id,
        destPath,
        includeSecrets: exportIncludeSecrets,
        includeSharedRoot: exportIncludeSharedRoot,
      })
      setExportDialogOpen(false)
      setExportTargetAgent(null)
      setExportIncludeSecrets(false)
      setExportIncludeSharedRoot(false)
      setPackageNotice({ kind: 'ok', text: `已将「${exportTargetAgent.name}」导出到 ${destPath}` })
    } catch (err) {
      setPackageNotice({ kind: 'err', text: err instanceof Error ? err.message : String(err) })
    } finally {
      setPackageBusy(false)
    }
  }

  const handleImportPackage = async () => {
    setPackageNotice(null)
    const picked = await open({
      multiple: false,
      filters: [{ name: 'NineClaw 智能体包', extensions: ['zip'] }],
    })
    const packagePath = Array.isArray(picked) ? picked[0] : picked
    if (packagePath == null) {
      return
    }
    setPackageBusy(true)
    try {
      const result = await importAgentPackage(packagePath)
      await onRefreshAgents()
      onSelectAgent(result.agent.id)
      const warnText = result.warnings.length > 0 ? `\n${result.warnings.join('\n')}` : ''
      setPackageNotice({ kind: 'ok', text: `已导入「${result.agent.name}」${warnText}` })
    } catch (err) {
      setPackageNotice({ kind: 'err', text: err instanceof Error ? err.message : String(err) })
    } finally {
      setPackageBusy(false)
    }
  }

  return (
    <div className={`agent-layout ${agentEditorOpen ? 'agent-layout-editor-open' : ''}`}>
      <div className="agent-studio-shell" hidden={agentEditorOpen}>
        <header className="agent-page-header agent-page-header-inline">
          <div>
            <span className="agent-page-kicker">Agent Studio</span>
            <h1>智能体管理</h1>
            <p></p>
          </div>

          <div className="agent-page-header-actions">
            <button type="button" className="outline-button" onClick={() => void onRefreshAgents()} disabled={loading || agentRefreshing}>
              <AppIcon name="refresh" size={18} />
              <span>{agentRefreshing ? '刷新中…' : '刷新列表'}</span>
            </button>

            <button
              type="button"
              className="outline-button"
              title="仅导出当前列表中选中的那一个智能体（含其 workspace 目录）"
              onClick={() => void handleOpenExportDialog()}
              disabled={loading || packageBusy || !selectedAgent}
            >
              <AppIcon name="download" size={18} />
              <span>导出所选</span>
            </button>

            <button
              type="button"
              className="outline-button"
              onClick={() => void handleImportPackage()}
              disabled={loading || packageBusy}
            >
              <AppIcon name="upload" size={18} />
              <span>导入</span>
            </button>

            <button type="button" className="create-agent-button agent-create-inline" onClick={onCreateAgent}>
              <AppIcon name="plus" size={20} />
              <span>新建智能体</span>
            </button>
          </div>
        </header>

        <section className="agent-list-panel">
          <div className="agent-sidebar-toolbar agent-toolbar-inline">
            <label className="search-field wide agent-search-field">
              <AppIcon name="search" size={18} />
              <input value={searchValue} onChange={(event) => onSearch(event.target.value)} placeholder="搜索名字、简介或介绍…" />
            </label>

            <div className="agent-sidebar-summary">
              <strong>{agents.length}</strong>
              <span>{studioCountLabel}</span>
            </div>
          </div>

          {error ? (
            <div className="skills-feedback error agent-feedback inline">
              <strong>智能体读取失败</strong>
              <span>{error}</span>
            </div>
          ) : null}

          {packageNotice ? (
            <div
              className={`skills-feedback agent-feedback inline ${packageNotice.kind === 'err' ? 'error' : 'success'}`}
            >
              <strong>{packageNotice.kind === 'err' ? '包操作失败' : '包操作成功'}</strong>
              <span className="agent-package-notice-text">{packageNotice.text}</span>
            </div>
          ) : null}

          <div className="agent-list-shell">
            <div className="agent-list-caption">我的智能体</div>
            <div className="agent-list">
              {agents.length > 0 ? (
                agents.map((agent) => (
                  <div
                    key={agent.id}
                    role="button"
                    tabIndex={0}
                    className={`agent-row list ${selectedAgent?.id === agent.id ? 'active' : ''}`}
                    onClick={() => onSelectAgent(agent.id)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault()
                        onSelectAgent(agent.id)
                      }
                    }}
                  >
                    <span className="agent-row-tone" style={{ backgroundColor: getAgentColor(agent) }} />
                    <span className="agent-badge" style={{ backgroundColor: getAgentColor(agent) }}>
                      <AppIcon name="bot" size={18} />
                    </span>
                    <span className="agent-copy">
                      <strong>
                        {agent.name}
                        {agent.id === defaultAgentId ? <span className="agent-inline-tag">默认</span> : null}
                      </strong>
                      <span className="agent-inline-id">ID: {agent.id}</span>
                      <span>{agent.summary}</span>
                      <small>{agent.description || '进入配置页，补充介绍、模型与技能。'}</small>
                    </span>
                    <span className="agent-list-meta">
                      <span className="agent-list-meta-pill">
                        {agent.defaultProviderId} · {agent.defaultModel}
                      </span>
                      <span className="agent-list-meta-pill">{agent.skillIds.length} 个技能</span>
                      <button
                        type="button"
                        className="agent-row-action"
                        onClick={(event) => {
                          event.stopPropagation()
                          onStartChatWithAgent(agent.id)
                        }}
                      >
                        直接聊天
                      </button>
                      <button
                        type="button"
                        className="agent-row-action"
                        onClick={(event) => {
                          event.stopPropagation()
                          onOpenBotBinding(agent.id)
                        }}
                      >
                        绑定 IM
                      </button>
                      <button
                        type="button"
                        className="agent-row-action"
                        onClick={(event) => {
                          event.stopPropagation()
                          handleOpenExportDialogForAgent(agent)
                        }}
                      >
                        导出包
                      </button>
                      <button
                        type="button"
                        className="agent-row-action"
                        onClick={(event) => {
                          event.stopPropagation()
                          onOpenEditor(agent.id)
                        }}
                      >
                        编辑配置
                      </button>
                    </span>
                  </div>
                ))
              ) : (
                <div className="empty-history-card compact agent-list-empty">
                  <div className="empty-history-title">{loading ? '正在读取智能体…' : '还没有已保存智能体'}</div>
                  <span>先新建一个智能体，再为它配置简介、模型和挂载技能。</span>
                </div>
              )}
            </div>
          </div>
        </section>

        {exportDialogOpen ? (
          <div
            className="confirm-dialog-overlay"
            role="presentation"
            onClick={closeExportDialog}
          >
            <div
              className="confirm-dialog"
              role="dialog"
              aria-modal="true"
              aria-labelledby="agent-export-title"
              onClick={(event) => event.stopPropagation()}
            >
              <h3 id="agent-export-title">导出单个智能体包</h3>
              <p>
                仅包含下方指定智能体在库中的配置及其 workspace 目录（记忆、raw、inbox 等），不会导出其他智能体或聊天记录。ZIP 默认不包含通道密钥。
              </p>
              {exportTargetAgent ? (
                <p className="agent-export-target-meta">
                  <strong>{exportTargetAgent.name}</strong>
                  <span className="agent-inline-id">ID: {exportTargetAgent.id}</span>
                </p>
              ) : null}
              <label className="input-field agent-export-option">
                <input
                  type="checkbox"
                  checked={exportIncludeSecrets}
                  onChange={(event) => setExportIncludeSecrets(event.target.checked)}
                  disabled={packageBusy}
                />
                <span>包含通道凭证与 API 密钥（不安全，勿分享）</span>
              </label>
              <label className="input-field agent-export-option">
                <input
                  type="checkbox"
                  checked={exportIncludeSharedRoot}
                  onChange={(event) => setExportIncludeSharedRoot(event.target.checked)}
                  disabled={packageBusy}
                />
                <span>同时打包工作区根目录共享策略文件（AGENTS / SOUL / USER 等）</span>
              </label>
              <div className="confirm-dialog-actions">
                <button type="button" className="outline-button" onClick={closeExportDialog} disabled={packageBusy}>
                  取消
                </button>
                <button
                  type="button"
                  className="create-agent-button"
                  onClick={() => void handleConfirmExport()}
                  disabled={packageBusy || !exportTargetAgent}
                >
                  {packageBusy ? '导出中…' : '选择保存位置'}
                </button>
              </div>
            </div>
          </div>
        ) : null}
      </div>

      {agentEditorOpen && agentDraft ? (
        <AgentEditorDialog
          agentDraft={agentDraft}
          agentDeleteConfirmOpen={agentDeleteConfirmOpen}
          agentDeleteConfirmText={agentDeleteConfirmText}
          agentFormError={agentFormError}
          agentFormNotice={agentFormNotice}
          agentRefreshing={agentRefreshing}
          agentSaving={agentSaving}
          allSkills={allSkills}
          defaultAgentId={defaultAgentId}
          mode={mode}
          modelOptions={modelOptions}
          onClose={onCloseEditor}
          onCloseDeleteAgentDialog={onCloseDeleteAgentDialog}
          onConfirmDeleteAgent={onConfirmDeleteAgent}
          onCreateAgent={onCreateAgent}
          onDraftChange={onDraftChange}
          onDeleteConfirmTextChange={onDeleteConfirmTextChange}
          onRefreshAgent={onRefreshAgents}
          onOpenWorkspace={onOpenWorkspace}
          onOpenSkillPicker={onOpenSkillPicker}
          onRequestDeleteAgent={onRequestDeleteAgent}
          onSaveAgent={onSaveAgent}
          onSetDefaultAgent={onSetDefaultAgent}
          onToggleSkill={onToggleSkill}
          selectedAgent={selectedAgent}
          managedAgentId={managedAgentId}
        />
      ) : null}

      {agentBotBindingDialogOpen && selectedAgent ? (
        <AgentBotBindingDialog
          agentId={selectedAgent.id}
          agentName={selectedAgent.name}
          botConfigs={botConfigs}
          botLoading={botLoading}
          botStatusLog={botStatusLog}
          formError={agentFormError}
          formNotice={agentFormNotice}
          onBotConfigChange={onBotConfigChange}
          onClose={onCloseBotBindingDialog}
          onLarkStart={onLarkStart}
          onLarkStop={onLarkStop}
          onRotatePeerSecret={onRotatePeerSecret}
          onSave={onSaveAgent}
          onSelectBot={onSelectBot}
          onWechatLogin={onWechatLogin}
          onWechatStart={onWechatStart}
          onWechatStop={onWechatStop}
          qrCodeUrl={qrCodeUrl}
          qrDialogOpen={qrDialogOpen}
          qrStatus={qrStatus}
          selectedBotConfig={selectedBotConfig}
          selectedBotDefinition={selectedBotDefinition}
          selectedBotId={selectedBotId}
          setBotLoading={setBotLoading}
          setQrDialogOpen={setQrDialogOpen}
          saving={agentSaving}
        />
      ) : null}

      {agentWorkspaceDialogOpen ? (
        <AgentWorkspaceDialog
          agentName={selectedAgent?.name ?? '智能体'}
          bundle={agentWorkspaceBundle}
          draftContent={agentWorkspaceDraftContent}
          error={agentWorkspaceDialogError}
          fileBodyLoading={agentWorkspaceFileLoading}
          loading={agentWorkspaceDialogLoading}
          onClose={onCloseWorkspaceDialog}
          onDraftChange={onDraftWorkspaceContentChange}
          onRefresh={onRefreshWorkspace}
          onSaveFile={onSaveWorkspaceFile}
          onSelectFile={onSelectWorkspaceFile}
          saveError={agentWorkspaceSaveError}
          saveLoading={agentWorkspaceSaving}
          saveNotice={agentWorkspaceSaveNotice}
          selectedFileKey={agentWorkspaceSelectedKey}
        />
      ) : null}
    </div>
  )
}
