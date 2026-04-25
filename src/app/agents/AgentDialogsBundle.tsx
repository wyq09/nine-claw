import { AppIcon } from '../../components/AppIcon'
import type {
  AgentInput,
  AgentLoopConfig,
  AgentRecord,
  InstalledSkillItem,
} from '../../types'
import {
  formatInstalledSkillScopeLabel,
  formatInstalledSkillSource,
  getAgentColor,
  SkillDescriptionDisclosure,
} from '../lib'

const DEFAULT_AGENT_LOOP_CONFIG: AgentLoopConfig = {
  maxIterations: 50,
  iterationTimeoutMs: 120000,
  enableNested: true,
  maxDepth: 3,
  allowExtend: true,
  maxExtendLimit: 200,
  maxConcurrent: 5,
  batchFailStrategy: 'WaitAll',
}

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
        <h3 id="agent-skill-picker-title">添加允许工具</h3>
        <p>从本地已安装技能里搜索并选择。选中的工具会成为这个智能体可使用的工具集合。</p>

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
                {allSkillCount > 0 ? '换个关键词继续搜索，或直接关闭弹窗。' : '先去技能库安装技能，再回到这里选择偏好技能。'}
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
  onClose,
  onCloseDeleteAgentDialog,
  onConfirmDeleteAgent,
  onCreateAgent,
  onDraftChange,
  onDeleteConfirmTextChange,
  onRefreshAgent,
  onOpenSkillPicker,
  onRequestDeleteAgent,
  onSaveAgent,
  onSetDefaultAgent,
  onToggleSkill,
  selectedAgent,
  managedAgentId,
}: AgentEditorDialogProps) {
  const missingSkillIds = (agentDraft?.skillIds ?? []).filter((skillId) => !allSkills.some((skill) => skill.id === skillId))
  const mountedSkills = allSkills.filter((skill) => agentDraft?.skillIds.includes(skill.id) ?? false)
  const editorAccent = getAgentColor(
    selectedAgent ?? { id: 'draft', name: agentDraft?.name || '智能体', accentColor: agentDraft?.accentColor },
  )
  const displayAgentId = agentDraft?.id?.trim() || managedAgentId.trim()

  if (!agentDraft) {
    return null
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
          <div className="agent-detail-hero agent-editor-hero-stage" style={{ borderColor: editorAccent + '1f' }}>
            <div className="agent-editor-hero-head">
              <div className="agent-detail-hero-main">
                <span className="agent-badge large" style={{ backgroundColor: editorAccent }}>
                  <AppIcon name="bot" size={24} />
                </span>
                <div className="agent-detail-copy">
                  <span className="agent-page-kicker">Agent Studio</span>
                  <h2 id="agent-editor-title">{mode === 'create' ? '新建智能体' : selectedAgent?.name ?? '编辑智能体'}</h2>
                  <p>配置智能体的标识、展示信息、自动触发规则、可用工具和提示词内容。</p>
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
              <span className="agent-hero-pill">{agentDraft.skillIds.length} 个允许工具</span>
              {displayAgentId ? <span className="agent-hero-pill">ID: {displayAgentId}</span> : null}
              {selectedAgent?.id === defaultAgentId ? <span className="agent-hero-pill accent">当前默认</span> : null}
            </div>
          </div>
        </header>

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

            <div className="agent-section">
              <div className="agent-section-header">
                <div>
                  <strong>智能体配置</strong>
                  <p>Agent_ID 会自动生成，也可以在保存前后改成更稳定的业务标识。</p>
                </div>
              </div>

              <div className="agent-form-grid">
                <label className="input-field">
                  <span>Agent_ID</span>
                  <input
                    value={agentDraft.id ?? ''}
                    onChange={(event) => onDraftChange({ id: event.target.value })}
                    placeholder={mode === 'create' ? '留空自动生成' : selectedAgent?.id ?? 'agent_id'}
                  />
                </label>
                <label className="input-field">
                  <span>展示名称</span>
                  <input
                    value={agentDraft.name}
                    onChange={(event) => onDraftChange({ name: event.target.value })}
                    placeholder="例如：项目经理"
                  />
                </label>
              </div>

              <label className="input-field agent-field-full">
                <span>描述</span>
                <textarea
                  value={agentDraft.description}
                  onChange={(event) => onDraftChange({ description: event.target.value })}
                  rows={4}
                  placeholder="说明这个智能体负责什么、擅长什么、回答风格和边界。"
                />
              </label>

              <label className="input-field agent-field-full">
                <span>触发条件</span>
                <textarea
                  value={agentDraft.triggerCondition}
                  onChange={(event) => onDraftChange({ triggerCondition: event.target.value })}
                  rows={3}
                  placeholder="说明模型何时应该自动调用这个智能体。"
                />
              </label>

              <label className="agent-toggle-row agent-field-full">
                <input
                  type="checkbox"
                  checked={agentDraft.manualTriggerOnly}
                  onChange={(event) => onDraftChange({ manualTriggerOnly: event.target.checked })}
                />
                <span>
                  <strong>禁止模型自动调用</strong>
                  <small>开启后仅允许用户手动触发这个智能体。</small>
                </span>
              </label>
            </div>

            <div className="agent-section">
              <div className="agent-section-header">
                <div>
                  <strong>Agent Loop 循环配置</strong>
                  <p>启用后此智能体可作为主 Agent 自主循环委派子 Agent 执行任务，直到任务完成。</p>
                </div>
              </div>

              <label className="agent-toggle-row agent-field-full">
                <input
                  type="checkbox"
                  checked={agentDraft.agentLoopConfig != null}
                  onChange={(event) => {
                    if (event.target.checked) {
                      onDraftChange({ agentLoopConfig: { ...DEFAULT_AGENT_LOOP_CONFIG } })
                    } else {
                      onDraftChange({ agentLoopConfig: undefined as unknown as AgentLoopConfig })
                    }
                  }}
                />
                <span>
                  <strong>启用 Agent Loop</strong>
                  <small>开启后此智能体回复中的委派标记会被自动拦截并执行。</small>
                </span>
              </label>

              {agentDraft.agentLoopConfig && (
                <>
                  <div className="agent-form-grid">
                    <label className="input-field">
                      <span>最大迭代次数</span>
                      <input
                        type="number"
                        min={1}
                        max={200}
                        value={agentDraft.agentLoopConfig.maxIterations}
                        onChange={(event) =>
                          onDraftChange({
                            agentLoopConfig: {
                              ...agentDraft.agentLoopConfig!,
                              maxIterations: Math.max(1, Math.min(200, Number(event.target.value) || 50)),
                            },
                          })
                        }
                      />
                    </label>
                    <label className="input-field">
                      <span>单次超时（秒）</span>
                      <input
                        type="number"
                        min={10}
                        max={600}
                        value={Math.round(agentDraft.agentLoopConfig.iterationTimeoutMs / 1000)}
                        onChange={(event) =>
                          onDraftChange({
                            agentLoopConfig: {
                              ...agentDraft.agentLoopConfig!,
                              iterationTimeoutMs: Math.max(10, Math.min(600, Number(event.target.value) || 120)) * 1000,
                            },
                          })
                        }
                      />
                    </label>
                  </div>

                  <div className="agent-form-grid">
                    <label className="input-field">
                      <span>最大并发数</span>
                      <input
                        type="number"
                        min={1}
                        max={20}
                        value={agentDraft.agentLoopConfig.maxConcurrent}
                        onChange={(event) =>
                          onDraftChange({
                            agentLoopConfig: {
                              ...agentDraft.agentLoopConfig!,
                              maxConcurrent: Math.max(1, Math.min(20, Number(event.target.value) || 5)),
                            },
                          })
                        }
                      />
                    </label>
                    <label className="input-field">
                      <span>并发失败策略</span>
                      <select
                        value={agentDraft.agentLoopConfig.batchFailStrategy}
                        onChange={(event) =>
                          onDraftChange({
                            agentLoopConfig: {
                              ...agentDraft.agentLoopConfig!,
                              batchFailStrategy: event.target.value as 'FailFast' | 'WaitAll',
                            },
                          })
                        }
                      >
                        <option value="WaitAll">等待全部完成</option>
                        <option value="FailFast">任一失败即停止</option>
                      </select>
                    </label>
                  </div>

                  <label className="agent-toggle-row agent-field-full">
                    <input
                      type="checkbox"
                      checked={agentDraft.agentLoopConfig.enableNested}
                      onChange={(event) =>
                        onDraftChange({
                          agentLoopConfig: {
                            ...agentDraft.agentLoopConfig!,
                            enableNested: event.target.checked,
                          },
                        })
                      }
                    />
                    <span>
                      <strong>允许嵌套委派</strong>
                      <small>子 Agent 也可以有自己的 Agent Loop，递归执行。</small>
                    </span>
                  </label>

                  {agentDraft.agentLoopConfig.enableNested && (
                    <div className="agent-form-grid">
                      <label className="input-field">
                        <span>嵌套最大深度</span>
                        <input
                          type="number"
                          min={1}
                          max={10}
                          value={agentDraft.agentLoopConfig.maxDepth}
                          onChange={(event) =>
                            onDraftChange({
                              agentLoopConfig: {
                                ...agentDraft.agentLoopConfig!,
                                maxDepth: Math.max(1, Math.min(10, Number(event.target.value) || 3)),
                              },
                            })
                          }
                        />
                      </label>
                      <div />
                    </div>
                  )}

                  <label className="agent-toggle-row agent-field-full">
                    <input
                      type="checkbox"
                      checked={agentDraft.agentLoopConfig.allowExtend}
                      onChange={(event) =>
                        onDraftChange({
                          agentLoopConfig: {
                            ...agentDraft.agentLoopConfig!,
                            allowExtend: event.target.checked,
                          },
                        })
                      }
                    />
                    <span>
                      <strong>允许申请扩容</strong>
                      <small>接近迭代上限时，主 Agent 可向用户申请增加循环次数。</small>
                    </span>
                  </label>

                  {agentDraft.agentLoopConfig.allowExtend && (
                    <div className="agent-form-grid">
                      <label className="input-field">
                        <span>扩容上限</span>
                        <input
                          type="number"
                          min={50}
                          max={1000}
                          value={agentDraft.agentLoopConfig.maxExtendLimit}
                          onChange={(event) =>
                            onDraftChange({
                              agentLoopConfig: {
                                ...agentDraft.agentLoopConfig!,
                                maxExtendLimit: Math.max(50, Math.min(1000, Number(event.target.value) || 200)),
                              },
                            })
                          }
                        />
                      </label>
                      <div />
                    </div>
                  )}
                </>
              )}
            </div>

            <div className="agent-section">
              <div className="agent-section-header">
                <div>
                  <strong>允许使用的工具</strong>
                  <p>从已安装技能中选择此智能体可使用的工具；未安装但已记录的工具可在下方移除。</p>
                </div>

                <button
                  type="button"
                  className="outline-button"
                  onClick={onOpenSkillPicker}
                  disabled={allSkills.length === 0}
                >
                  <AppIcon name="plus" size={16} />
                  <span>添加工具</span>
                </button>
              </div>

              {mountedSkills.length > 0 || missingSkillIds.length > 0 ? (
                <div className="agent-mounted-skill-list">
                  {mountedSkills.map((skill) => (
                    <article key={skill.id} className="agent-mounted-skill">
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
                  <strong>{allSkills.length > 0 ? '还没有允许工具' : '暂无已安装工具'}</strong>
                  <span>
                    {allSkills.length > 0
                      ? '点击右上角“添加工具”，从已安装技能里搜索并添加。'
                      : '先去技能库安装技能，再回到这里配置允许使用的工具。'}
                  </span>
                </div>
              )}
            </div>

            <div className="agent-section">
              <div className="agent-section-header">
                <div>
                  <strong>提示词内容</strong>
                  <p>可使用 {'${ARG}'} 占位符引用本次手动触发或自动调用时的用户输入。</p>
                </div>
              </div>

              <label className="input-field agent-field-full">
                <span>提示词内容</span>
                <textarea
                  value={agentDraft.systemPrompt}
                  onChange={(event) => onDraftChange({ systemPrompt: event.target.value })}
                  rows={8}
                  placeholder="例如：你是合同审查智能体。请围绕 ${ARG} 输出风险点、修改建议和需要用户补充的信息。"
                />
              </label>
            </div>
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
              <p>请输入“确认删除”以删除「{selectedAgent.name}」。</p>
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
