import { useEffect, useState } from 'react'
import { Check, Copy } from 'lucide-react'
import { AppIcon } from '../../components/AppIcon'
import type {
  AgentExecutionMode,
  AgentInput,
  AgentRecord,
  AgentScenarioLlmConfig,
  InstalledSkillItem,
  PeerGatewayInfo,
} from '../../types'
import { getPeerGatewayInfo } from '../../lib/piClient'
import {
  buildAutoAgentSummary,
  createAgentBotConfigState,
  formatAgentExecutionModeLabel,
  formatInstalledSkillScopeLabel,
  formatInstalledSkillSource,
  getAgentColor,
  normalizeAgentScenarioLlmConfigInDraft,
  sessionLlmDecode,
  sessionLlmEncode,
  SkillDescriptionDisclosure,
} from '../lib'

export type AgentSkillPickerDialogProps = {
  allSkillCount: number
  searchValue: string
  selectedSkillIds: string[]
  skills: InstalledSkillItem[]
  onClose: () => void
  onSearch: (value: string) => void
  onToggleSkill: (skillId: string) => void
}

export function AgentSkillPickerDialog({
  allSkillCount,
  searchValue,
  selectedSkillIds,
  skills,
  onClose,
  onSearch,
  onToggleSkill,
}: AgentSkillPickerDialogProps) {
  return (
    <div className="confirm-dialog-overlay" role="presentation" onClick={onClose}>
      <div
        className="confirm-dialog agent-skill-picker-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-skill-picker-title"
        onClick={(event) => event.stopPropagation()}
      >
        <h3 id="agent-skill-picker-title">添加挂载技能</h3>
        <p>从本地已安装技能里搜索并选择。选中的技能会作为当前智能体的运行时能力注入。</p>

        <label className="input-field skill-install-field">
          <span>搜索技能</span>
          <input
            autoFocus
            value={searchValue}
            onChange={(event) => onSearch(event.target.value)}
            placeholder="搜索技能名称、说明、来源或路径"
          />
        </label>

        <div className="agent-skill-picker-meta">
          <span>已安装 {allSkillCount} 个</span>
          <span>已选中 {selectedSkillIds.length} 个</span>
        </div>

        <div className="agent-skill-picker-list">
          {skills.length > 0 ? (
            skills.map((skill) => {
              const active = selectedSkillIds.includes(skill.id)
              return (
                <article
                  key={skill.id}
                  className={`agent-skill-option ${active ? 'active' : ''}`}
                >
                  <div className="agent-skill-option-copy">
                    <strong>{skill.name}</strong>
                    <SkillDescriptionDisclosure description={skill.description} className="skill-description-inset" />
                    <small>
                      {formatInstalledSkillScopeLabel(skill.scope)} · {formatInstalledSkillSource(skill)}
                    </small>
                  </div>
                  <div className="agent-skill-option-actions">
                    <button
                      type="button"
                      className="agent-skill-option-action"
                      onClick={() => onToggleSkill(skill.id)}
                    >
                      {active ? '已添加' : '添加技能'}
                    </button>
                  </div>
                </article>
              )
            })
          ) : (
            <div className="agent-skill-picker-empty">
              <strong>{allSkillCount > 0 ? '没有匹配的技能' : '暂无已安装技能'}</strong>
              <span>
                {allSkillCount > 0 ? '换个关键词继续搜索，或直接关闭弹窗。' : '先去技能库安装技能，再回到这里挂载。'}
              </span>
            </div>
          )}
        </div>

        <div className="confirm-dialog-actions">
          <button type="button" className="outline-button" onClick={onClose}>
            完成
          </button>
        </div>
      </div>
    </div>
  )
}

export function buildAgentPeerSnippet(
  info: PeerGatewayInfo,
  savedAgentId: string | null,
  peerSecret: string,
): string {
  const lines: string[] = []
  lines.push('── NineClaw 智能体对等接入说明 ──')
  lines.push('')
  if (info.envOverrideActive) {
    lines.push('【说明】监听地址由环境变量 NINECLAW_PEER_BIND 决定，与应用内「设置 → 通用」中的端口无关。')
    lines.push('')
  }
  if (!info.enabled) {
    lines.push(
      '【注意】当前未监听对等 HTTP。请在 NineClaw「设置 → 通用 → 对等 HTTP（虾）」中启用并配置端口（默认 1052），或设置环境变量 NINECLAW_PEER_BIND。',
    )
    lines.push('')
  }
  if (info.listenAddress) {
    lines.push(`进程监听：${info.listenAddress}`)
  }
  if (info.publicBaseUrl) {
    lines.push(`API 接口地址：${info.publicBaseUrl}`)
  }
  if (info.inboundUrl) {
    lines.push(`入站接口（POST）：${info.inboundUrl}`)
  }
  if (info.healthUrl) {
    lines.push(`健康检查（GET）：${info.healthUrl}`)
  }
  lines.push('')
  lines.push('鉴权：请求头 Authorization: Bearer <本智能体入站密钥>')
  lines.push('')
  if (savedAgentId) {
    lines.push(`本智能体 ID（JSON 字段 toAgentId）：${savedAgentId}`)
  } else {
    lines.push('本智能体 ID：请先保存智能体，保存后即可在此看到稳定 ID。')
  }
  lines.push(
    peerSecret
      ? `本智能体入站密钥：${peerSecret}`
      : '本智能体入站密钥：（保存智能体后由系统生成；或在「机器人 → 虾/对等」查看 / 重新生成）',
  )
  lines.push('')
  lines.push('请求 JSON 示例：')
  lines.push(
    JSON.stringify(
      {
        protocol: 'nineclaw-peer',
        version: 1,
        fromAgentId: '<我的名字>',
        toAgentId: savedAgentId || '<保存后替换为本智能体ID>',
        threadId: '同一会话固定字符串',
        text: '你好',
      },
      null,
      2,
    ),
  )
  lines.push('')
  lines.push('同步成功时响应示例（统一信封，字段均为 camelCase）：')
  lines.push(
    JSON.stringify(
      {
        protocol: 'nineclaw-peer',
        version: 1,
        ok: true,
        kind: 'inboundReply',
        fromAgentId: '<我的名字>',
        toAgentId: savedAgentId || '<本智能体ID>',
        threadId: '同一会话固定字符串',
        reply: '助手回复正文',
      },
      null,
      2,
    ),
  )
  return lines.join('\n')
}

export type AgentEditorDialogProps = {
  agentDraft: AgentInput | null
  agentDeleteConfirmOpen: boolean
  agentDeleteConfirmText: string
  agentFormError: string
  agentFormNotice: string
  agentRefreshing: boolean
  agentSaving: boolean
  allSkills: InstalledSkillItem[]
  defaultAgentId: string
  mode: 'create' | 'edit'
  modelOptions: { value: string; label: string }[]
  onClose: () => void
  onCloseDeleteAgentDialog: () => void
  onConfirmDeleteAgent: () => void
  onCreateAgent: () => void
  onDraftChange: (updates: Partial<AgentInput>) => void
  onDeleteConfirmTextChange: (value: string) => void
  onRefreshAgent: () => void
  onOpenWorkspace: () => void
  onOpenSkillPicker: () => void
  onRequestDeleteAgent: () => void
  onSaveAgent: () => void
  onSetDefaultAgent: () => void
  onToggleSkill: (skillId: string) => void
  selectedAgent: AgentRecord | null
  /** 已保存智能体的 id；新建为空字符串 */
  managedAgentId: string
}

export function AgentEditorDialog({
  agentDraft,
  agentDeleteConfirmOpen,
  agentDeleteConfirmText,
  agentFormError,
  agentFormNotice,
  agentRefreshing,
  agentSaving,
  allSkills,
  defaultAgentId,
  mode,
  modelOptions,
  onClose,
  onCloseDeleteAgentDialog,
  onConfirmDeleteAgent,
  onCreateAgent,
  onDraftChange,
  onDeleteConfirmTextChange,
  onRefreshAgent,
  onOpenWorkspace,
  onOpenSkillPicker,
  onRequestDeleteAgent,
  onSaveAgent,
  onSetDefaultAgent,
  onToggleSkill,
  selectedAgent,
  managedAgentId,
}: AgentEditorDialogProps) {
  const [peerGatewayInfo, setPeerGatewayInfo] = useState<PeerGatewayInfo | null>(null)
  const [peerGatewayLoadError, setPeerGatewayLoadError] = useState('')
  const [peerSnippetCopied, setPeerSnippetCopied] = useState(false)
  type AgentEditorTab = 'basics' | 'models' | 'integrations' | 'advanced'
  const [editorTab, setEditorTab] = useState<AgentEditorTab>('basics')

  useEffect(() => {
    let cancelled = false
    setPeerGatewayLoadError('')
    void getPeerGatewayInfo()
      .then((value) => {
        if (!cancelled) {
          setPeerGatewayInfo(value)
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setPeerGatewayLoadError(error instanceof Error ? error.message : String(error))
        }
      })
    return () => {
      cancelled = true
    }
  }, [managedAgentId])

  const peerDraftConfigs = createAgentBotConfigState(agentDraft?.botConfigs)
  const peerDraftSecret =
    peerDraftConfigs.peer?.peerSharedSecret?.trim() || peerDraftConfigs.peer?.clientSecret?.trim() || ''
  const savedAgentIdForPeer = managedAgentId.trim() || null
  const peerSnippetText =
    peerGatewayInfo !== null
      ? buildAgentPeerSnippet(peerGatewayInfo, savedAgentIdForPeer, peerDraftSecret)
      : ''

  const selectedModelValue =
    agentDraft?.defaultProviderId.trim() && agentDraft?.defaultModel.trim()
      ? sessionLlmEncode(agentDraft.defaultProviderId, agentDraft.defaultModel)
      : ''
  const generatedSummary = buildAutoAgentSummary(agentDraft?.description ?? '', agentDraft?.name ?? '')
  const missingSkillIds = (agentDraft?.skillIds ?? []).filter((skillId) => !allSkills.some((skill) => skill.id === skillId))
  const mountedSkills = allSkills.filter((skill) => agentDraft?.skillIds.includes(skill.id) ?? false)
  const editorAccent = getAgentColor(
    selectedAgent ?? { id: 'draft', name: agentDraft?.name || '智能体', accentColor: agentDraft?.accentColor },
  )
  useEffect(() => {
    setEditorTab('basics')
  }, [managedAgentId, mode])

  if (!agentDraft) {
    return null
  }

  const scenarioCfg = agentDraft.scenarioLlmConfig ?? {}
  const titleScenarioValue =
    scenarioCfg.titleGeneration?.providerId?.trim() && scenarioCfg.titleGeneration.model?.trim()
      ? sessionLlmEncode(scenarioCfg.titleGeneration.providerId, scenarioCfg.titleGeneration.model)
      : ''
  const memoryScenarioValue =
    scenarioCfg.memoryExtraction?.providerId?.trim() && scenarioCfg.memoryExtraction.model?.trim()
      ? sessionLlmEncode(scenarioCfg.memoryExtraction.providerId, scenarioCfg.memoryExtraction.model)
      : ''
  const taskPushScenarioValue =
    scenarioCfg.taskPushNotificationCopy?.providerId?.trim() &&
    scenarioCfg.taskPushNotificationCopy.model?.trim()
      ? sessionLlmEncode(
          scenarioCfg.taskPushNotificationCopy.providerId,
          scenarioCfg.taskPushNotificationCopy.model,
        )
      : ''

  const setScenarioSlot = (
    key: 'titleGeneration' | 'memoryExtraction' | 'taskPushNotificationCopy',
    encoded: string,
  ) => {
    const parsed = encoded.trim() ? sessionLlmDecode(encoded) : null
    const next: AgentScenarioLlmConfig = { ...scenarioCfg }
    if (!parsed) {
      if (key === 'titleGeneration') {
        delete next.titleGeneration
      } else if (key === 'memoryExtraction') {
        delete next.memoryExtraction
      } else {
        delete next.taskPushNotificationCopy
      }
    } else {
      next[key] = { providerId: parsed.providerId, model: parsed.model }
    }
    onDraftChange({ scenarioLlmConfig: normalizeAgentScenarioLlmConfigInDraft(next) })
  }

  return (
    <div className="agent-editor-page-root">
      <div
        className="agent-editor-dialog agent-editor-page"
        role="dialog"
        aria-modal="true"
        aria-labelledby="agent-editor-title"
      >
        <header className="agent-editor-hero-band">
          <div className="agent-detail-hero agent-editor-hero-stage" style={{ borderColor: `${editorAccent}1f` }}>
            <div className="agent-editor-hero-head">
              <div className="agent-detail-hero-main">
                <span className="agent-badge large" style={{ backgroundColor: editorAccent }}>
                  <AppIcon name="bot" size={24} />
                </span>
                <div className="agent-detail-copy">
                  <span className="agent-page-kicker">{mode === 'create' ? 'Create Agent' : 'Agent Editor'}</span>
                  <h2 id="agent-editor-title">{mode === 'create' ? '新建智能体' : selectedAgent?.name ?? '编辑智能体'}</h2>
                  <p>
                    {mode === 'create'
                      ? '先填名字与角色说明并挂载技能；在「模型配置」「三方对接」「高级」中完成其余设置。'
                      : selectedAgent?.summary ?? '用下方 Tab 切换分区：基本信息、模型、三方对接、高级。'}
                  </p>
                </div>
              </div>
              <div className="agent-editor-hero-toolbar">
                <button type="button" className="outline-button" onClick={onClose}>
                  <AppIcon name="arrow-left" size={16} />
                  <span>返回列表</span>
                </button>
                <button
                  type="button"
                  className="outline-button"
                  onClick={onRefreshAgent}
                  disabled={agentRefreshing || agentSaving || mode !== 'edit' || !selectedAgent}
                >
                  <AppIcon name="refresh" size={16} />
                  <span>{agentRefreshing ? '刷新中…' : '刷新'}</span>
                </button>
                <button type="button" className="icon-button subtle" onClick={onClose} aria-label="关闭智能体配置页">
                  <AppIcon name="close" size={18} />
                </button>
              </div>
            </div>
            <div className="agent-hero-pills">
              <span className="agent-hero-pill">{mode === 'create' ? '未保存' : '用户智能体'}</span>
              {agentDraft.executionMode !== 'single' ? (
                <span className="agent-hero-pill">{formatAgentExecutionModeLabel(agentDraft.executionMode)}</span>
              ) : null}
              <span className="agent-hero-pill">{agentDraft.skillIds.length} 个挂载技能</span>
              {selectedAgent ? <span className="agent-hero-pill">ID: {selectedAgent.id}</span> : null}
              {selectedAgent?.id === defaultAgentId ? <span className="agent-hero-pill accent">当前默认</span> : null}
            </div>
          </div>
        </header>

        <div className="agent-editor-tablist" role="tablist" aria-label="智能体配置分区">
          <button
            type="button"
            role="tab"
            className={`agent-editor-tab ${editorTab === 'basics' ? 'active' : ''}`}
            aria-selected={editorTab === 'basics'}
            onClick={() => setEditorTab('basics')}
          >
            基本信息
          </button>
          <button
            type="button"
            role="tab"
            className={`agent-editor-tab ${editorTab === 'models' ? 'active' : ''}`}
            aria-selected={editorTab === 'models'}
            onClick={() => setEditorTab('models')}
          >
            模型配置
          </button>
          <button
            type="button"
            role="tab"
            className={`agent-editor-tab ${editorTab === 'integrations' ? 'active' : ''}`}
            aria-selected={editorTab === 'integrations'}
            onClick={() => setEditorTab('integrations')}
          >
            三方对接
          </button>
          <button
            type="button"
            role="tab"
            className={`agent-editor-tab ${editorTab === 'advanced' ? 'active' : ''}`}
            aria-selected={editorTab === 'advanced'}
            onClick={() => setEditorTab('advanced')}
          >
            高级
          </button>
        </div>

        <div className="agent-editor-dialog-scroll">
          <div className="agent-detail-card agent-editor-card agent-editor-form-surface">
            {agentFormError ? (
              <div className="skills-feedback error agent-feedback inline">
                <strong>保存失败</strong>
                <span>{agentFormError}</span>
              </div>
            ) : null}

            {agentFormNotice ? (
              <div className="skills-feedback success agent-feedback inline">
                <strong>已更新</strong>
                <span>{agentFormNotice}</span>
              </div>
            ) : null}

            {editorTab === 'basics' ? (
              <>
	            <div className="agent-section">
	              <div className="agent-section-header">
	                <div>
	                  <strong>基本信息</strong>
	                  <p>名字与角色说明；列表摘要默认从角色说明自动生成，也可在「高级」里手动覆盖。</p>
	                </div>
	              </div>

	              <div className="agent-form-grid">
	                <label className="input-field agent-field-full">
                  <span>名字</span>
                  <input
                    value={agentDraft.name}
                    onChange={(event) => onDraftChange({ name: event.target.value })}
                    placeholder="例如：诉讼项目助理"
                  />
	                </label>
	              </div>

	              <label className="input-field agent-field-full">
	                <span>角色说明</span>
	                <textarea
	                  value={agentDraft.description}
	                  onChange={(event) => onDraftChange({ description: event.target.value })}
	                  rows={5}
	                  placeholder="说明这个智能体负责什么、擅长什么、回答风格和边界。"
	                />
	              </label>

	              <div className="agent-helper-copy">
	                <strong>列表摘要将自动生成</strong>
	                <span>
	                  当前预览：{generatedSummary || '输入角色说明后会自动生成'}。若要手动改写摘要，请打开「高级」分页。定时类任务请在「任务中心」管理。
	                </span>
	              </div>
	            </div>

            <div className="agent-section">
              <div className="agent-section-header">
                <div>
                  <strong>挂载技能</strong>
                  <p>通过搜索弹窗选择已安装技能。挂载后会通过 pi 的 `--skill` 注入到当前智能体运行时。</p>
                </div>

                <button
                  type="button"
                  className="outline-button"
                  onClick={onOpenSkillPicker}
                  disabled={allSkills.length === 0}
                >
                  <AppIcon name="plus" size={16} />
                  <span>添加技能</span>
                </button>
              </div>

              {mountedSkills.length > 0 || missingSkillIds.length > 0 ? (
                <div className="agent-mounted-skill-list">
                  {mountedSkills.map((skill) => (
                    <article
                      key={skill.id}
                      className="agent-mounted-skill"
                    >
                      <div className="agent-mounted-skill-copy">
                        <strong>{skill.name}</strong>
                        <SkillDescriptionDisclosure description={skill.description} className="skill-description-inset" />
                        <small>
                          {formatInstalledSkillScopeLabel(skill.scope)} · {formatInstalledSkillSource(skill)}
                        </small>
                      </div>
                      <button
                        type="button"
                        className="agent-mounted-skill-action"
                        onClick={() => onToggleSkill(skill.id)}
                      >
                        <AppIcon name="close" size={16} />
                        <span>移除</span>
                      </button>
                    </article>
                  ))}

                  {missingSkillIds.map((skillId) => (
                    <article key={skillId} className="agent-mounted-skill missing">
                      <div className="agent-mounted-skill-copy">
                        <strong>{skillId}</strong>
                        <small>本地未找到，点击即可移除</small>
                      </div>
                      <button
                        type="button"
                        className="agent-mounted-skill-action"
                        onClick={() => onToggleSkill(skillId)}
                      >
                        <AppIcon name="close" size={16} />
                        <span>移除</span>
                      </button>
                    </article>
                  ))}
                </div>
              ) : (
                <div className="agent-empty-block">
                  <strong>{allSkills.length > 0 ? '还没有挂载技能' : '暂无已安装技能'}</strong>
                  <span>
                    {allSkills.length > 0
                      ? '点击右上角“添加技能”，从已安装技能里搜索并挂载。'
                      : '先去技能库安装技能，再回到这里进行挂载。'}
                  </span>
                </div>
              )}

              {missingSkillIds.length > 0 ? (
                <div className="agent-missing-skills">
                  <strong>有技能记录当前未在本地找到：</strong>
                  <span>{missingSkillIds.join('、')}</span>
                </div>
              ) : null}
            </div>

            <div className="agent-section">
              <div className="agent-section-header">
                <div>
                  <strong>工作区 Markdown</strong>
                  <p>查看这个智能体对应的 `IDENTITY.md`、`ROLE.md`、`MEMORY.md`、`WORKING.md` 等文件内容。</p>
                </div>

                <button
                  type="button"
                  className="outline-button"
                  onClick={onOpenWorkspace}
                  disabled={!selectedAgent || mode !== 'edit'}
                >
                  <AppIcon name="book" size={16} />
                  <span>{selectedAgent && mode === 'edit' ? '查看 md 文件' : '保存后可查看'}</span>
                </button>
              </div>

              <div className="agent-workspace-hint">
                <span>智能体 ID：{selectedAgent?.id ?? '保存后生成'}</span>
                <span>私有目录：{selectedAgent ? `agents/${selectedAgent.id}/` : '尚未创建'}</span>
                <span>共享文件：`AGENTS.md`、`SOUL.md`、`USER.md`、`MEMORY.md`、`TOOLS.md`</span>
              </div>
            </div>
              </>
            ) : null}

            {editorTab === 'models' ? (
              <div className="agent-section agent-model-scenarios">
                <div className="agent-section-header">
                  <div>
                    <strong>模型配置</strong>
                    <p>
                      为每个场景单独指定模型；未设置的场景将自动使用「默认对话模型」。记忆相关写入遵循工作区规则（含 MEMORY.md 与用户记忆索引）。
                    </p>
                  </div>
                </div>

                <div className="agent-model-scenario-list">
                  <div className="agent-model-scenario-row">
                    <div className="agent-model-scenario-copy">
                      <strong>默认对话模型</strong>
                      <span>主对话、新建话题时使用的模型</span>
                    </div>
                    <label className="input-field agent-model-scenario-select">
                      <select
                        value={selectedModelValue}
                        onChange={(event) => {
                          const parsed = sessionLlmDecode(event.target.value)
                          if (parsed) {
                            onDraftChange({
                              defaultProviderId: parsed.providerId,
                              defaultModel: parsed.model,
                            })
                          }
                        }}
                      >
                        {modelOptions.length === 0 ? (
                          <option value="">暂无已配置模型</option>
                        ) : (
                          modelOptions.map((option) => (
                            <option key={option.value} value={option.value}>
                              {option.label}
                            </option>
                          ))
                        )}
                      </select>
                    </label>
                  </div>

                  <div className="agent-model-scenario-row">
                    <div className="agent-model-scenario-copy">
                      <strong>标题生成</strong>
                      <span>每次对话后自动生成会话标题（配置已保存；与主对话模型分工时可单独指定）</span>
                    </div>
                    <label className="input-field agent-model-scenario-select">
                      <select
                        value={titleScenarioValue}
                        onChange={(event) => setScenarioSlot('titleGeneration', event.target.value)}
                      >
                        <option value="">同默认对话模型</option>
                        {modelOptions.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    </label>
                  </div>

                  <div className="agent-model-scenario-row">
                    <div className="agent-model-scenario-copy">
                      <strong>记忆提取与分类</strong>
                      <span>每轮对话后的记忆沉淀与分类（MEMORY.md / 用户记忆；与主模型分工时可单独指定）</span>
                    </div>
                    <label className="input-field agent-model-scenario-select">
                      <select
                        value={memoryScenarioValue}
                        onChange={(event) => setScenarioSlot('memoryExtraction', event.target.value)}
                      >
                        <option value="">同默认对话模型</option>
                        {modelOptions.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    </label>
                  </div>

                  <div className="agent-model-scenario-row">
                    <div className="agent-model-scenario-copy">
                      <strong>定时任务推送文案</strong>
                      <span>
                        根据任务正文生成任务中心列表标题与一句话简介，并用于桌面推送标题与摘要；未指定时与默认对话模型相同
                      </span>
                    </div>
                    <label className="input-field agent-model-scenario-select">
                      <select
                        value={taskPushScenarioValue}
                        onChange={(event) => setScenarioSlot('taskPushNotificationCopy', event.target.value)}
                      >
                        <option value="">同默认对话模型</option>
                        {modelOptions.map((option) => (
                          <option key={option.value} value={option.value}>
                            {option.label}
                          </option>
                        ))}
                      </select>
                    </label>
                  </div>
                </div>
              </div>
            ) : null}

            {editorTab === 'integrations' ? (
              <div className="agent-section agent-integrations-section">
                <div className="agent-section-header">
                  <div>
                    <strong>三方对接</strong>
                    <p>与外部系统互通时的地址与说明。对等 HTTP（虾）入站与全应用共用监听端口；下列信息含本智能体 ID 与密钥，可一键复制给对方。</p>
                  </div>
                </div>
                <div className="agent-settings-panel-card">
                  <div className="agent-peer-integration-card">
                    <div className="agent-peer-integration-head">
                      <strong>对等 HTTP（虾）对接</strong>
                      <span>全应用共用一个监听端口；下列地址与一键说明会包含本智能体的密钥与 ID。</span>
                    </div>
                    {peerGatewayLoadError ? (
                      <div className="skills-feedback error agent-feedback inline">
                        <span>读取网关信息失败：{peerGatewayLoadError}</span>
                      </div>
                    ) : null}
                    {peerGatewayInfo ? (
                      <>
                        <div className="agent-peer-api-grid">
                          <label className="input-field">
                            <span>监听地址（NINECLAW_PEER_BIND）</span>
                            <input
                              readOnly
                              value={
                                peerGatewayInfo.enabled && peerGatewayInfo.listenAddress
                                  ? peerGatewayInfo.listenAddress
                                  : '未配置（未监听）'
                              }
                            />
                          </label>
                          <label className="input-field">
                            <span>入站 API（POST）</span>
                            <input readOnly value={peerGatewayInfo.inboundUrl ?? '—'} />
                          </label>
                          <label className="input-field">
                            <span>健康检查（GET）</span>
                            <input readOnly value={peerGatewayInfo.healthUrl ?? '—'} />
                          </label>
                        </div>
                        <div className="agent-peer-snippet-toolbar">
                          <span className="agent-peer-snippet-label">给对方的一键说明（含密钥）</span>
                          <button
                            type="button"
                            className="outline-button"
                            onClick={() => {
                              void navigator.clipboard.writeText(peerSnippetText).then(() => {
                                setPeerSnippetCopied(true)
                                window.setTimeout(() => setPeerSnippetCopied(false), 2000)
                              })
                            }}
                          >
                            {peerSnippetCopied ? (
                              <>
                                <Check size={16} />
                                <span>已复制</span>
                              </>
                            ) : (
                              <>
                                <Copy size={16} />
                                <span>复制全文</span>
                              </>
                            )}
                          </button>
                        </div>
                        <pre className="agent-peer-snippet-pre">{peerSnippetText}</pre>
                      </>
                    ) : (
                      <div className="agent-workspace-hint">
                        <span>正在读取对等网关信息…</span>
                      </div>
                    )}
                  </div>
                </div>
              </div>
            ) : null}

            {editorTab === 'advanced' ? (
	            <div className="agent-section">
	                <div className="agent-advanced-stack">
	                  <div className="agent-subsection">
	                    <div className="agent-subsection-header">
	                      <div>
	                        <strong>运行与路由</strong>
	                        <p>默认模型来自智能体配置，但聊天窗口里仍支持用户按当前会话临时切换，不会回写智能体默认值。</p>
	                      </div>
	                    </div>

	                    <div className="agent-form-grid">
	                      <label className="input-field">
	                        <span>列表摘要覆盖（可选）</span>
	                        <input
	                          value={agentDraft.summary}
	                          onChange={(event) => onDraftChange({ summary: event.target.value })}
	                          placeholder={generatedSummary || '留空时自动生成'}
	                        />
	                      </label>

	                      <label className="input-field">
	                        <span>执行模式</span>
	                        <select
	                          value={agentDraft.executionMode}
	                          onChange={(event) =>
	                            onDraftChange({
	                              executionMode: (event.target.value as AgentExecutionMode) || 'single',
	                            })
	                          }
	                        >
	                          <option value="single">单智能体</option>
	                          <option value="supervisor">协调者（预留）</option>
	                          <option value="worker">执行者（预留）</option>
	                        </select>
	                      </label>
	                    </div>

	                    <div className="agent-helper-copy subtle">
	                      <strong>自动摘要预览</strong>
	                      <span>{generatedSummary || '输入角色说明后生成摘要。'}</span>
	                    </div>

	                    <label className="input-field agent-field-full">
	                      <span>高级指令（可选）</span>
	                      <textarea
	                        value={agentDraft.systemPrompt}
	                        onChange={(event) => onDraftChange({ systemPrompt: event.target.value })}
	                        rows={6}
	                        placeholder="补充额外执行约束、回答方式或边界要求。留空时会根据名字和角色说明自动生成角色上下文。"
	                      />
	                    </label>
	                  </div>
	                </div>
	            </div>
            ) : null}
          </div>
        </div>

        <div className="agent-editor-dialog-footer">
          <button type="button" className="outline-button" onClick={onCreateAgent} disabled={agentSaving}>
            重新新建
          </button>
          {selectedAgent && mode === 'edit' ? (
            <button
              type="button"
              className="outline-button"
              onClick={onSetDefaultAgent}
              disabled={agentSaving || selectedAgent.id === defaultAgentId}
            >
              {selectedAgent.id === defaultAgentId ? '当前默认' : '设为默认'}
            </button>
          ) : null}
          {selectedAgent && mode === 'edit' ? (
            <button type="button" className="outline-button danger" onClick={onRequestDeleteAgent} disabled={agentSaving}>
              删除
            </button>
          ) : null}
          <button type="button" className="outline-button primary" onClick={onSaveAgent} disabled={agentSaving}>
            {agentSaving ? '保存中…' : mode === 'create' ? '创建智能体' : '保存修改'}
          </button>
        </div>

        {agentDeleteConfirmOpen && selectedAgent ? (
          <div className="confirm-dialog-overlay" role="presentation" onClick={onCloseDeleteAgentDialog}>
            <div className="confirm-dialog" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
              <h3>删除智能体</h3>
              <p>删除后会一并清除这个智能体的数据库记录、绑定机器人数据和 md 工作区。</p>
              <p>请输入 `确认删除` 以删除「{selectedAgent.name}」。</p>
              <label className="input-field">
                <span>确认口令</span>
                <input
                  value={agentDeleteConfirmText}
                  onChange={(event) => onDeleteConfirmTextChange(event.target.value)}
                  placeholder="确认删除"
                />
              </label>
              <div className="confirm-dialog-actions">
                <button type="button" className="outline-button" onClick={onCloseDeleteAgentDialog} disabled={agentSaving}>
                  取消
                </button>
                <button
                  type="button"
                  className="outline-button confirm-dialog-delete"
                  onClick={onConfirmDeleteAgent}
                  disabled={agentSaving || agentDeleteConfirmText.trim() !== '确认删除'}
                >
                  {agentSaving ? '删除中…' : '确认删除'}
                </button>
              </div>
            </div>
          </div>
        ) : null}

      </div>
    </div>
  )
}
