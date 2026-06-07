import { useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import { AppIcon, type IconName } from '../../components/AppIcon'
import { NumericDraftField } from '../../components/NumericDraftField'
import type {
  AgentInput,
  AgentLoopConfig,
  AgentRecord,
  AgentScenarioLlmConfig,
  AgentToolId,
  InstalledSkillItem,
} from '../../types'
import { defaultRuntimeParameters } from '../../mockData'
import {
  AGENT_TOOL_OPTIONS,
  formatInstalledSkillScopeLabel,
  formatInstalledSkillSource,
  normalizeAgentScenarioLlmConfigInDraft,
  sessionLlmDecode,
  sessionLlmEncode,
  SkillDescriptionDisclosure,
  suggestAgentIdFromName,
} from '../lib'

const DEFAULT_AGENT_LOOP_CONFIG: AgentLoopConfig = {
  maxIterations: 80,
  iterationTimeoutMs: 120000,
  enableNested: true,
  maxDepth: 3,
  allowExtend: true,
  maxExtendLimit: 200,
  maxConcurrent: 5,
  batchFailStrategy: 'waitAll',
}

type AgentEditorSectionId = 'profile' | 'model' | 'scenes' | 'loop' | 'tools' | 'skills' | 'prompt'

type AgentEditorConfigurationProps = {
  agentDraft: AgentInput
  allSkills: InstalledSkillItem[]
  mode: 'create' | 'edit'
  modelOptions: { value: string; label: string }[]
  onDraftChange: (updates: Partial<AgentInput>) => void
  onOpenPromptEditor: () => void
  onOpenSkillPicker: () => void
  onToggleSkill: (skillId: string) => void
  selectedAgent: AgentRecord | null
}

type SectionMeta = {
  id: AgentEditorSectionId
  icon: IconName
  title: string
  description: string
  meta: string
}

export function AgentEditorConfiguration({
  agentDraft,
  allSkills,
  mode,
  modelOptions,
  onDraftChange,
  onOpenPromptEditor,
  onOpenSkillPicker,
  onToggleSkill,
  selectedAgent,
}: AgentEditorConfigurationProps) {
  const [activeSection, setActiveSection] = useState<AgentEditorSectionId>('profile')
  const [mountedSkillSearch, setMountedSkillSearch] = useState('')
  const missingSkillIds = (agentDraft.skillIds ?? []).filter((skillId) => !allSkills.some((skill) => skill.id === skillId))
  const mountedSkills = allSkills.filter((skill) => agentDraft.skillIds.includes(skill.id))
  const defaultModelLabel = agentDraft.defaultProviderId && agentDraft.defaultModel
    ? `${agentDraft.defaultProviderId} / ${agentDraft.defaultModel}`
    : '会话默认'
  const normalizedMountedSkillSearch = mountedSkillSearch.trim().toLowerCase()
  const visibleMountedSkills = normalizedMountedSkillSearch
    ? mountedSkills.filter((skill) =>
        [
          skill.id,
          skill.name,
          skill.description,
          skill.path,
          skill.source ?? '',
          formatInstalledSkillScopeLabel(skill.scope),
          formatInstalledSkillSource(skill),
        ].some((value) => value.toLowerCase().includes(normalizedMountedSkillSearch)),
      )
    : mountedSkills
  const visibleMissingSkillIds = normalizedMountedSkillSearch
    ? missingSkillIds.filter((skillId) => skillId.toLowerCase().includes(normalizedMountedSkillSearch))
    : missingSkillIds
  const hasSkillRecords = mountedSkills.length > 0 || missingSkillIds.length > 0
  const hasVisibleSkillRecords = visibleMountedSkills.length > 0 || visibleMissingSkillIds.length > 0

  const sectionItems = useMemo<SectionMeta[]>(
    () => [
      {
        id: 'profile',
        icon: 'bot',
        title: '基础信息',
        description: '名称、摘要、说明与触发条件',
        meta: agentDraft.manualTriggerOnly ? '仅手动' : '可自动',
      },
      {
        id: 'model',
        icon: 'provider',
        title: '模型',
        description: '默认 LLM 与 Provider',
        meta: defaultModelLabel,
      },
      {
        id: 'scenes',
        icon: 'sparkles',
        title: '场景模型',
        description: '标题、记忆、任务通知',
        meta: `${Object.keys(agentDraft.scenarioLlmConfig ?? {}).length} 项`,
      },
      {
        id: 'loop',
        icon: 'network',
        title: 'Agent Loop',
        description: '循环、并发、嵌套委派',
        meta: agentDraft.agentLoopConfig ? '已启用' : '关闭',
      },
      {
        id: 'tools',
        icon: 'wrench',
        title: '工具权限',
        description: '限制底层原子工具',
        meta: `${agentDraft.allowedToolIds.length} 个`,
      },
      {
        id: 'skills',
        icon: 'puzzle',
        title: '技能',
        description: '允许加载的本地技能',
        meta: `${agentDraft.skillIds.length} 个`,
      },
      {
        id: 'prompt',
        icon: 'book',
        title: '提示词',
        description: '系统提示词与放大编辑',
        meta: `${agentDraft.systemPrompt.trim().length} 字`,
      },
    ],
    [
      agentDraft.agentLoopConfig,
      agentDraft.allowedToolIds.length,
      agentDraft.manualTriggerOnly,
      agentDraft.scenarioLlmConfig,
      agentDraft.skillIds.length,
      agentDraft.systemPrompt,
      defaultModelLabel,
    ],
  )

  const toggleAllowedTool = (toolId: AgentToolId) => {
    const active = agentDraft.allowedToolIds.includes(toolId)
    onDraftChange({
      allowedToolIds: active
        ? agentDraft.allowedToolIds.filter((item) => item !== toolId)
        : [...agentDraft.allowedToolIds, toolId],
    })
  }

  const scenarioSlotEncoded = (slot: { providerId: string; model: string } | undefined) => {
    const providerId = slot?.providerId?.trim() ?? ''
    const model = slot?.model?.trim() ?? ''
    return providerId && model ? sessionLlmEncode(providerId, model) : sessionLlmEncode('', '')
  }

  const setScenarioSlot = (
    key: 'titleGeneration' | 'memoryExtraction' | 'taskPushNotificationCopy',
    encodedValue: string,
  ) => {
    const decoded = encodedValue ? sessionLlmDecode(encodedValue) : null
    const nextSlot =
      decoded && decoded.providerId.trim() && decoded.model.trim()
        ? { providerId: decoded.providerId.trim(), model: decoded.model.trim() }
        : undefined
    const merged: AgentScenarioLlmConfig = {
      ...(agentDraft.scenarioLlmConfig ?? {}),
      [key]: nextSlot,
    }
    onDraftChange({ scenarioLlmConfig: normalizeAgentScenarioLlmConfigInDraft(merged) })
  }

  const renderSection = () => {
    if (activeSection === 'profile') {
      return (
        <AgentEditorPanel title="基础信息" description={mode === 'create' ? 'Agent_ID 会自动生成，也可以自定义业务标识。' : 'Agent_ID 创建后不可修改。'}>
          <div className="agent-form-grid">
            <label className="input-field">
              <span>Agent_ID</span>
              <div className="agent-inline-field-action">
                <input
                  value={agentDraft.id ?? ''}
                  onChange={(event) => onDraftChange({ id: event.target.value })}
                  placeholder={mode === 'create' ? '留空自动生成' : selectedAgent?.id ?? 'agent_id'}
                  disabled={mode === 'edit'}
                />
                {mode === 'create' ? (
                  <button
                    type="button"
                    className="outline-button compact"
                    onClick={() => {
                      const suggested = suggestAgentIdFromName(agentDraft.name)
                      if (suggested) onDraftChange({ id: suggested })
                    }}
                    title="根据展示名称生成推荐 ID"
                  >
                    生成
                  </button>
                ) : null}
              </div>
            </label>
            <label className="input-field">
              <span>展示名称</span>
              <input
                value={agentDraft.name}
                onChange={(event) => onDraftChange({ name: event.target.value })}
                placeholder="例如：项目经理"
              />
            </label>
            <label className="input-field agent-field-full">
              <span>一句话摘要</span>
              <input
                value={agentDraft.summary}
                onChange={(event) => onDraftChange({ summary: event.target.value })}
                placeholder="在列表中展示的短摘要"
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
        </AgentEditorPanel>
      )
    }

    if (activeSection === 'model') {
      return (
        <AgentEditorPanel title="模型" description="此智能体使用的 LLM 模型。留空则使用会话级默认模型。">
          <div className="agent-form-grid">
            <label className="input-field agent-field-full">
              <span>模型</span>
              <select
                value={sessionLlmEncode(agentDraft.defaultProviderId, agentDraft.defaultModel)}
                onChange={(event) => {
                  const decoded = sessionLlmDecode(event.target.value)
                  if (decoded) {
                    onDraftChange({ defaultProviderId: decoded.providerId, defaultModel: decoded.model })
                  }
                }}
              >
                {modelOptions.length === 0 ? (
                  <option value="">暂无可用模型，请先在设置中配置</option>
                ) : (
                  <>
                    <option value={sessionLlmEncode('', '')}>使用会话默认</option>
                    {modelOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </>
                )}
              </select>
            </label>
          </div>
        </AgentEditorPanel>
      )
    }

    if (activeSection === 'scenes') {
      return (
        <AgentEditorPanel title="场景模型" description="与默认模型解耦的辅助调用；不选则统一使用本智能体默认模型。">
          <div className="agent-form-grid">
            <label className="input-field">
              <span>会话标题生成</span>
              <select
                value={scenarioSlotEncoded(agentDraft.scenarioLlmConfig?.titleGeneration)}
                onChange={(event) => setScenarioSlot('titleGeneration', event.target.value)}
              >
                <option value={sessionLlmEncode('', '')}>使用智能体默认</option>
                {modelOptions.map((option) => (
                  <option key={`title-${option.value}`} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="input-field">
              <span>团队记忆提取</span>
              <select
                value={scenarioSlotEncoded(agentDraft.scenarioLlmConfig?.memoryExtraction)}
                onChange={(event) => setScenarioSlot('memoryExtraction', event.target.value)}
              >
                <option value={sessionLlmEncode('', '')}>使用智能体默认</option>
                {modelOptions.map((option) => (
                  <option key={`mem-${option.value}`} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="input-field agent-field-full">
              <span>任务通知标题与简介</span>
              <select
                value={scenarioSlotEncoded(agentDraft.scenarioLlmConfig?.taskPushNotificationCopy)}
                onChange={(event) => setScenarioSlot('taskPushNotificationCopy', event.target.value)}
              >
                <option value={sessionLlmEncode('', '')}>使用智能体默认</option>
                {modelOptions.map((option) => (
                  <option key={`push-${option.value}`} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
              <small>定时任务在任务中心 / 系统推送中展示的标题与一句话简介生成。</small>
            </label>
          </div>
        </AgentEditorPanel>
      )
    }

    if (activeSection === 'loop') {
      return (
        <AgentEditorPanel title="Agent Loop" description="启用后此智能体可作为主 Agent 自主循环委派子 Agent 执行任务。">
          <label className="agent-toggle-row agent-field-full">
            <input
              type="checkbox"
              checked={agentDraft.agentLoopConfig != null}
              onChange={(event) => {
                onDraftChange({
                  agentLoopConfig: event.target.checked ? { ...DEFAULT_AGENT_LOOP_CONFIG } : undefined,
                })
              }}
            />
            <span>
              <strong>启用 Agent Loop</strong>
              <small>开启后此智能体回复中的委派标记会被自动拦截并执行。</small>
            </span>
          </label>

          {agentDraft.agentLoopConfig ? (
            <AgentLoopFields agentDraft={agentDraft} onDraftChange={onDraftChange} />
          ) : (
            <div className="agent-empty-block">
              <strong>Agent Loop 未启用</strong>
              <span>开启后可配置超时、并发、嵌套委派与扩容策略。</span>
            </div>
          )}
        </AgentEditorPanel>
      )
    }

    if (activeSection === 'tools') {
      return (
        <AgentEditorPanel title="工具权限" description="限制此智能体运行时可调用的底层原子工具；未勾选的工具不会注入给模型。">
          <div className="agent-tool-permission-grid" role="group" aria-label="允许使用的工具">
            {AGENT_TOOL_OPTIONS.map((tool) => {
              const active = agentDraft.allowedToolIds.includes(tool.id)
              return (
                <button
                  key={tool.id}
                  type="button"
                  className={`agent-tool-permission ${active ? 'active' : ''}`}
                  onClick={() => toggleAllowedTool(tool.id)}
                  title={tool.description}
                  aria-pressed={active}
                >
                  {tool.label}
                </button>
              )
            })}
          </div>

          {agentDraft.allowedToolIds.length === 0 ? (
            <div className="agent-empty-block warning">
              <strong>当前没有允许的工具</strong>
              <span>保存后此智能体只能生成文本，不能读取文件、执行命令、联网或委派。</span>
            </div>
          ) : null}
        </AgentEditorPanel>
      )
    }

    if (activeSection === 'skills') {
      return (
        <AgentEditorPanel
          title="技能"
          description="从已安装技能中选择此智能体可加载的技能；未安装但已记录的技能可在下方移除。"
          action={
            <button type="button" className="outline-button" onClick={onOpenSkillPicker} disabled={allSkills.length === 0}>
              <AppIcon name="plus" size={16} />
              <span>添加技能</span>
            </button>
          }
        >
          {hasSkillRecords ? (
            <>
              <label className="input-field agent-mounted-skill-search">
                <span>搜索已添加技能</span>
                <div className="agent-search-input-wrap">
                  <AppIcon name="search" size={16} />
                  <input
                    value={mountedSkillSearch}
                    onChange={(event) => setMountedSkillSearch(event.target.value)}
                    placeholder="按名称、描述、ID、来源或路径搜索"
                  />
                </div>
              </label>

              {hasVisibleSkillRecords ? (
                <div className="agent-mounted-skill-list">
                  {visibleMountedSkills.map((skill) => (
                    <article key={skill.id} className="agent-mounted-skill">
                      <div className="agent-mounted-skill-copy">
                        <strong>{skill.name}</strong>
                        <SkillDescriptionDisclosure description={skill.description} className="skill-description-inset" />
                        <small>
                          {formatInstalledSkillScopeLabel(skill.scope)} · {formatInstalledSkillSource(skill)}
                        </small>
                      </div>
                      <button type="button" className="agent-mounted-skill-action" onClick={() => onToggleSkill(skill.id)}>
                        <AppIcon name="close" size={16} />
                        <span>移除</span>
                      </button>
                    </article>
                  ))}

                  {visibleMissingSkillIds.map((skillId) => (
                    <article key={skillId} className="agent-mounted-skill missing">
                      <div className="agent-mounted-skill-copy">
                        <strong>{skillId}</strong>
                        <small>本地未找到，点击即可移除</small>
                      </div>
                      <button type="button" className="agent-mounted-skill-action" onClick={() => onToggleSkill(skillId)}>
                        <AppIcon name="close" size={16} />
                        <span>移除</span>
                      </button>
                    </article>
                  ))}
                </div>
              ) : (
                <div className="agent-empty-block">
                  <strong>没有匹配的已添加技能</strong>
                  <span>换个关键词，或清空搜索查看全部已添加技能。</span>
                </div>
              )}
            </>
          ) : (
            <div className="agent-empty-block">
              <strong>{allSkills.length > 0 ? '还没有允许技能' : '暂无已安装技能'}</strong>
              <span>
                {allSkills.length > 0
                  ? '点击右上角“添加技能”，从已安装技能里搜索并添加。'
                  : '先去技能库安装技能，再回到这里配置允许使用的技能。'}
              </span>
            </div>
          )}
        </AgentEditorPanel>
      )
    }

    return (
      <AgentEditorPanel
        title="提示词"
        description="可使用 ${ARG} 占位符引用本次手动触发或自动调用时的用户输入。"
        action={
          <button type="button" className="outline-button" onClick={onOpenPromptEditor} title="放大编辑提示词">
            <AppIcon name="eye" size={16} />
            <span>放大编辑</span>
          </button>
        }
      >
        <label className="input-field agent-field-full">
          <span>提示词内容</span>
          <textarea
            value={agentDraft.systemPrompt}
            onChange={(event) => onDraftChange({ systemPrompt: event.target.value })}
            rows={10}
            placeholder="例如：你是合同审查智能体。请围绕 ${ARG} 输出风险点、修改建议和需要用户补充的信息。"
          />
        </label>
      </AgentEditorPanel>
    )
  }

  return (
    <div className="agent-editor-config-layout">
      <nav className="agent-editor-section-menu" aria-label="智能体配置菜单">
        <span className="agent-editor-menu-label">配置项</span>
        {sectionItems.map((section) => (
          <button
            key={section.id}
            type="button"
            className={`agent-editor-menu-item ${activeSection === section.id ? 'active' : ''}`}
            onClick={() => setActiveSection(section.id)}
            aria-label={section.title}
            aria-current={activeSection === section.id ? 'page' : undefined}
          >
            <AppIcon name={section.icon} size={18} />
            <span className="agent-editor-menu-copy">
              <strong>{section.title}</strong>
              <small>{section.description}</small>
            </span>
            <span className="agent-editor-menu-meta">{section.meta}</span>
          </button>
        ))}
      </nav>

      <main className="agent-editor-section-pane" aria-live="polite">
        {renderSection()}
      </main>
    </div>
  )
}

function AgentEditorPanel({
  action,
  children,
  description,
  title,
}: {
  action?: ReactNode
  children: ReactNode
  description: string
  title: string
}) {
  return (
    <section className="agent-section agent-editor-section-panel">
      <div className="agent-section-header">
        <div>
          <strong>{title}</strong>
          <p>{description}</p>
        </div>
        {action}
      </div>
      <div className="agent-editor-section-body">{children}</div>
    </section>
  )
}

function AgentLoopFields({
  agentDraft,
  onDraftChange,
}: {
  agentDraft: AgentInput
  onDraftChange: (updates: Partial<AgentInput>) => void
}) {
  const config = agentDraft.agentLoopConfig
  if (!config) {
    return null
  }

  const patchLoop = (updates: Partial<AgentLoopConfig>) => {
    onDraftChange({ agentLoopConfig: { ...config, ...updates } })
  }

  return (
    <>
      <div className="agent-workspace-hint agent-field-full">
        <span>
          单次对话内工具调用最大轮数由 <strong>设置 → 参数 → Agent 循环</strong> 中的「最大迭代次数」统一配置（默认{' '}
          {defaultRuntimeParameters.maxAgentToolRoundsPerDialogue} 轮）。
        </span>
      </div>
      <div className="agent-form-grid">
        <label className="input-field">
          <span>单次超时（秒）</span>
          <NumericDraftField
            aria-label="单次超时（秒）"
            value={Math.round(config.iterationTimeoutMs / 1000)}
            min={10}
            max={600}
            fallbackOnBlur={120}
            onCommit={(seconds) => patchLoop({ iterationTimeoutMs: seconds * 1000 })}
          />
        </label>
        <label className="input-field">
          <span>最大并发数</span>
          <NumericDraftField
            aria-label="最大并发数"
            value={config.maxConcurrent}
            min={1}
            max={20}
            fallbackOnBlur={5}
            onCommit={(next) => patchLoop({ maxConcurrent: next })}
          />
        </label>
        <label className="input-field">
          <span>并发失败策略</span>
          <select
            value={config.batchFailStrategy}
            onChange={(event) => patchLoop({ batchFailStrategy: event.target.value as 'failFast' | 'waitAll' })}
          >
            <option value="waitAll">等待全部完成</option>
            <option value="failFast">任一失败即停止</option>
          </select>
        </label>
      </div>

      <label className="agent-toggle-row agent-field-full">
        <input
          type="checkbox"
          checked={config.enableNested}
          onChange={(event) => patchLoop({ enableNested: event.target.checked })}
        />
        <span>
          <strong>允许嵌套委派</strong>
          <small>子 Agent 也可以有自己的 Agent Loop，递归执行。</small>
        </span>
      </label>

      {config.enableNested ? (
        <div className="agent-form-grid">
          <label className="input-field">
            <span>嵌套最大深度</span>
            <NumericDraftField
              aria-label="嵌套最大深度"
              value={config.maxDepth}
              min={1}
              max={10}
              fallbackOnBlur={3}
              onCommit={(next) => patchLoop({ maxDepth: next })}
            />
          </label>
        </div>
      ) : null}

      <label className="agent-toggle-row agent-field-full">
        <input
          type="checkbox"
          checked={config.allowExtend}
          onChange={(event) => patchLoop({ allowExtend: event.target.checked })}
        />
        <span>
          <strong>允许申请扩容</strong>
          <small>接近迭代上限时，主 Agent 可向用户申请增加循环次数。</small>
        </span>
      </label>

      {config.allowExtend ? (
        <div className="agent-form-grid">
          <label className="input-field">
            <span>扩容上限</span>
            <NumericDraftField
              aria-label="扩容上限"
              value={config.maxExtendLimit}
              min={50}
              max={1000}
              fallbackOnBlur={200}
              onCommit={(next) => patchLoop({ maxExtendLimit: next })}
            />
          </label>
        </div>
      ) : null}
    </>
  )
}
