import type { Dispatch, SetStateAction } from 'react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { AppIcon } from '../../components/AppIcon'
import { useToast } from '../../hooks/useToast'
import type { AgentRecord, AgentTaskListItem, AgentTaskUpdateInput, InstalledSkillItem, ResourceItem, SkillLibraryTab, SystemSkillCatalog } from '../../types'
import {
  deleteAgentTask,
  listAgentTasks,
  pauseAgentTask,
  resumeAgentTask,
  runAgentTaskNow,
  updateAgentTask,
} from '../../lib/piClient'
import {
  formatMonthlyDays,
  formatAgentTaskScheduleShort,
  formatAgentTaskStatus,
  formatInstalledSkillScopeLabel,
  formatInstalledSkillSource,
  formatOptionalAbsoluteTime,
  formatWeeklyDays,
  groupAgentTasksByStatus,
  parseDailyTimesInput,
  parseNumericDaysInput,
  runAtMsToDatetimeLocalValue,
  SkillDescriptionDisclosure,
} from '../lib'

export type SkillsViewProps = {
  installedSkillCount: number
  installedSkills: InstalledSkillItem[]
  onChangeTab: (tab: SkillLibraryTab) => void
  onInstallByLink: () => void
  onInstallSystemSkill: (skillId: string) => Promise<void> | void
  onRefresh: () => Promise<void> | void
  sessionBusy: boolean
  setSearch: (value: string) => void
  skillsError: string
  skillsLoading: boolean
  systemSkillCount: number
  systemSkillCatalog: SystemSkillCatalog
  systemSkillInstallId: string
  tab: SkillLibraryTab
  skillSearch: string
  visibleSystemSkills: SystemSkillCatalog['skills']
}

export function SkillsView({
  installedSkillCount,
  installedSkills,
  onChangeTab,
  onInstallByLink,
  onInstallSystemSkill,
  onRefresh,
  sessionBusy,
  setSearch,
  skillsError,
  skillsLoading,
  systemSkillCount,
  systemSkillCatalog,
  systemSkillInstallId,
  tab,
  skillSearch,
  visibleSystemSkills,
}: SkillsViewProps) {
  const isInstalledTab = tab === 'installed'

  return (
    <div className="page-shell">
      <header className="page-header">
        <h1>技能</h1>
      </header>

      <div className="page-toolbar">
        <div className="tab-row">
          <button
            type="button"
            className={`tab-button ${isInstalledTab ? 'active' : ''}`}
            onClick={() => onChangeTab('installed')}
          >
            已安装技能
          </button>
          <button
            type="button"
            className={`tab-button ${!isInstalledTab ? 'active' : ''}`}
            onClick={() => onChangeTab('system')}
          >
            系统技能库
          </button>
        </div>
        <button
          type="button"
          className={`page-toolbar-refresh${skillsLoading ? ' is-loading' : ''}`}
          onClick={() => void onRefresh()}
          disabled={skillsLoading}
          aria-busy={skillsLoading}
        >
          <AppIcon name="refresh" size={16} />
          <span>{skillsLoading ? '刷新中…' : '刷新'}</span>
        </button>
      </div>

      <div className="search-row">
        <label className="search-field">
          <AppIcon name="search" size={18} />
          <input
            value={skillSearch}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={isInstalledTab ? '搜索已安装技能、路径或来源…' : '搜索系统技能库…'}
          />
        </label>
        <button
          type="button"
          className="primary-cta"
          onClick={onInstallByLink}
          disabled={skillsLoading || sessionBusy}
        >
          <AppIcon name="plus-circle" size={18} />
          <span>通过链接安装</span>
        </button>
      </div>

      {skillsError ? (
        <div className="skills-feedback error">
          <strong>技能读取失败</strong>
          <span>{skillsError}</span>
        </div>
      ) : null}

      {isInstalledTab ? (
        installedSkills.length > 0 ? (
          <div className="card-grid">
            {installedSkills.map((skill) => (
              <article key={skill.id} className="skill-card">
                <div className="skill-card-head">
                  <div className="skill-title-group">
                    <div className="skill-icon">
                      <AppIcon name="puzzle" size={18} />
                    </div>
                    <div className="skill-title-copy">
                      <h3>{skill.name}</h3>
                      <SkillDescriptionDisclosure description={skill.description} />
                    </div>
                  </div>
                </div>

                <div className="skill-pill-row">
                  <span className={`skill-pill ${skill.scope}`}>{formatInstalledSkillScopeLabel(skill.scope)}</span>
                  <span className="skill-pill subtle">{formatInstalledSkillSource(skill)}</span>
                </div>
                <div className="skill-card-foot skill-card-foot-grid">
                  <span className="skill-path-text" title={skill.path}>
                    {skill.path}
                  </span>
                  <span>{formatOptionalAbsoluteTime(skill.updatedAt)}</span>
                </div>
              </article>
            ))}
          </div>
        ) : (
          <div className="skill-library-empty">
            <div className="skill-library-empty-icon">
              <AppIcon name="puzzle" size={28} />
            </div>
            <strong>
              {skillsLoading
                ? '正在扫描技能目录…'
                : installedSkillCount > 0
                  ? '没有匹配的技能'
                  : '还没有读取到已安装技能'}
            </strong>
            <p>
              {installedSkillCount > 0
                ? '换个关键词试试，支持按技能名、描述、路径和来源搜索。'
                : '将自动扫描工作区和系统目录中的技能。你也可以先通过链接安装，系统会为你打开安装引导会话。'}
            </p>
            {installedSkillCount === 0 ? (
              <button
                type="button"
                className="outline-button"
                onClick={onInstallByLink}
                disabled={skillsLoading || sessionBusy}
              >
                通过链接安装
              </button>
            ) : null}
          </div>
        )
      ) : systemSkillCatalog.available && visibleSystemSkills.length > 0 ? (
        <div className="card-grid">
          {visibleSystemSkills.map((skill) => (
            <article key={skill.id} className="skill-card">
              <div className="skill-card-head">
                <div className="skill-title-group">
                  <div className="skill-icon">
                    <AppIcon name="bag" size={18} />
                  </div>
                  <div className="skill-title-copy">
                    <h3>{skill.name}</h3>
                    <SkillDescriptionDisclosure description={skill.description} />
                  </div>
                </div>
              </div>
              <div className="skill-card-foot skill-card-foot-grid">
                <span>{skill.installUrl ?? '系统内置技能'}</span>
                <button
                  type="button"
                  className="outline-button"
                  onClick={() => void onInstallSystemSkill(skill.id)}
                  disabled={skill.installed || sessionBusy || !!systemSkillInstallId}
                >
                  {skill.installed
                    ? '已安装'
                    : systemSkillInstallId === skill.id
                      ? '安装中…'
                      : '安装到全局技能'}
                </button>
              </div>
            </article>
          ))}
        </div>
      ) : (
        <div className="skill-library-empty">
          <div className="skill-library-empty-icon">
            <AppIcon name="bag" size={28} />
          </div>
          <strong>
            {systemSkillCatalog.available && systemSkillCount > 0 ? '没有匹配的系统技能' : '系统技能库预留中'}
          </strong>
          <p>
            {systemSkillCatalog.available && systemSkillCount > 0
              ? '换个关键词试试，系统技能库上线后会支持按名称和描述检索。'
              : systemSkillCatalog.message}
          </p>
        </div>
      )}
    </div>
  )
}

export type ResourcesViewProps = {
  onSearch: (value: string) => void
  resourceSearch: string
  visibleResources: ResourceItem[]
}

export function ResourcesView({ onSearch, resourceSearch, visibleResources }: ResourcesViewProps) {
  return (
    <div className="page-shell">
      <header className="page-header">
        <h1>资源库</h1>
        <p>把模板、知识沉淀和可复用的交付资产放在同一个工作台里。</p>
      </header>

      <div className="resource-hero">
        <div>
          <strong>资源编排</strong>
          <span>先沉淀知识，再让 pi 从统一入口调度。</span>
        </div>
        <button type="button" className="primary-cta">
          <AppIcon name="plus-circle" size={18} />
          <span>新增资源</span>
        </button>
      </div>

      <label className="search-field wide">
        <AppIcon name="search" size={18} />
        <input value={resourceSearch} onChange={(event) => onSearch(event.target.value)} placeholder="搜索模板、规范、知识沉淀…" />
      </label>

      <div className="resource-grid">
        {visibleResources.map((resource) => (
          <article key={resource.id} className="resource-card">
            <div className="resource-tag">{resource.tag}</div>
            <h3>{resource.title}</h3>
            <p>{resource.description}</p>
            <div className="resource-meta">{resource.updatedAt}</div>
          </article>
        ))}
      </div>
    </div>
  )
}

export type TaskCenterEditPageProps = {
  task: AgentTaskListItem
  editDraft: AgentTaskUpdateInput
  setEditDraft: Dispatch<SetStateAction<AgentTaskUpdateInput | null>>
  actionBusy: boolean
  onBack: () => void
  onOpenAgent: (agentId: string) => void
  onSave: () => void
  onRunNow: () => void
  onPause: () => void
  onResume: () => void
  onDelete: () => void
}

export function TaskCenterEditPage({
  task,
  editDraft,
  setEditDraft,
  actionBusy,
  onBack,
  onOpenAgent,
  onSave,
  onRunNow,
  onPause,
  onResume,
  onDelete,
}: TaskCenterEditPageProps) {
  const [defaultOnceAtMs] = useState(() => Date.now() + 60 * 60 * 1000)

  return (
    <div className="page-shell task-center-page task-linear-page task-edit-page">
      <header className="page-header task-linear-page-toolbar task-edit-detail-header">
        <div className="task-edit-header-leading">
          <button type="button" className="outline-button task-edit-header-back" onClick={onBack} aria-label="返回任务列表">
            <AppIcon name="arrow-left" size={18} />
            <span>返回</span>
          </button>
          <div className="task-linear-page-toolbar-text task-edit-header-titles">
            <h1>{task.title.trim() || '未命名任务'}</h1>
            <p className="task-linear-page-sub">
              {formatAgentTaskStatus(task.status)} · {formatAgentTaskScheduleShort(task)}
              {task.nextRunAt ? ` · 下次 ${formatOptionalAbsoluteTime(task.nextRunAt)}` : ''}
            </p>
          </div>
        </div>
        <div className="task-edit-header-actions">
          <button
            type="button"
            className="primary-cta task-linear-toolbar-cta"
            disabled={actionBusy || task.status === 'deleted'}
            onClick={onRunNow}
          >
            {actionBusy ? '处理中…' : '立即执行'}
          </button>
          {task.status === 'active' ? (
            <button type="button" className="outline-button" disabled={actionBusy} onClick={onPause}>
              {actionBusy ? '处理中…' : '暂停'}
            </button>
          ) : null}
          {task.status === 'paused' ? (
            <button type="button" className="outline-button" disabled={actionBusy} onClick={onResume}>
              {actionBusy ? '处理中…' : '恢复'}
            </button>
          ) : null}
          {task.status !== 'deleted' ? (
            <button type="button" className="outline-button danger" disabled={actionBusy} onClick={onDelete}>
              删除
            </button>
          ) : null}
          <button type="button" className="outline-button" onClick={() => onOpenAgent(task.agentId)}>
            打开智能体
          </button>
        </div>
      </header>

      <div className="task-center-body task-edit-body">
        <section className="task-edit-section">
          <h2 className="task-edit-section-title">上下文</h2>
          <dl className="task-edit-dl">
            <div className="task-edit-dl-row">
              <dt>智能体</dt>
              <dd>{task.agentName}</dd>
            </div>
            <div className="task-edit-dl-row">
              <dt>类型</dt>
              <dd>{task.taskType === 'agent_prompt' ? 'agent_prompt（到点唤起智能体）' : 'reminder（直接提醒）'}</dd>
            </div>
            <div className="task-edit-dl-row">
              <dt>来源会话</dt>
              <dd className="task-edit-dl-mono">{task.sourceSessionId}</dd>
            </div>
            <div className="task-edit-dl-row">
              <dt>投递</dt>
              <dd>
                {task.deliveryKind} → {task.deliveryTarget}
                {task.resultInNewSession ? '（独立会话）' : ''}
              </dd>
            </div>
            <div className="task-edit-dl-row">
              <dt>上次执行</dt>
              <dd>{formatOptionalAbsoluteTime(task.lastRunAt)}</dd>
            </div>
          </dl>
        </section>

        <section className="task-edit-section">
          <h2 className="task-edit-section-title">调度与内容</h2>
          <div className="task-edit-form">
            <label className="task-edit-field">
              <span className="task-edit-label">任务标题</span>
              <input
                className="task-edit-control"
                value={editDraft.title}
                onChange={(event) =>
                  setEditDraft((current) => (current ? { ...current, title: event.target.value } : current))
                }
              />
            </label>
            <label className="task-edit-field">
              <span className="task-edit-label">触发方式</span>
              <select
                className="task-edit-control"
                value={editDraft.scheduleType}
                onChange={(event) =>
                  setEditDraft((current) =>
                    current
                      ? {
                          ...current,
                          scheduleType: event.target.value,
                          intervalMinutes: event.target.value === 'interval' ? current.intervalMinutes || 10 : null,
                          dailyTimes:
                            event.target.value === 'daily_time' ||
                            event.target.value === 'weekly_time' ||
                            event.target.value === 'monthly_time'
                              ? current.dailyTimes
                              : [],
                          weeklyDays: event.target.value === 'weekly_time' ? current.weeklyDays : [],
                          monthlyDays: event.target.value === 'monthly_time' ? current.monthlyDays : [],
                          runAtMs:
                            event.target.value === 'once_at'
                              ? current.runAtMs ?? defaultOnceAtMs
                              : null,
                          resultInNewSession:
                            event.target.value === 'once_at' ? (current.resultInNewSession ?? false) : false,
                        }
                      : current,
                  )
                }
              >
                <option value="interval">每隔若干分钟</option>
                <option value="daily_time">每天固定时间</option>
                <option value="weekly_time">每周固定星期</option>
                <option value="monthly_time">每月固定日期</option>
                <option value="once_at">指定时间（只执行一次）</option>
              </select>
            </label>
            {editDraft.scheduleType === 'interval' ? (
              <label className="task-edit-field">
                <span className="task-edit-label">间隔（分钟）</span>
                <input
                  className="task-edit-control"
                  type="number"
                  min={1}
                  value={editDraft.intervalMinutes ?? 10}
                  onChange={(event) =>
                    setEditDraft((current) =>
                      current
                        ? {
                            ...current,
                            intervalMinutes: Number.parseInt(event.target.value || '0', 10) || 0,
                          }
                        : current,
                    )
                  }
                />
              </label>
            ) : editDraft.scheduleType === 'once_at' ? (
              <>
                <label className="task-edit-field">
                  <span className="task-edit-label">执行时间（本地）</span>
                  <input
                    className="task-edit-control"
                    type="datetime-local"
                    value={runAtMsToDatetimeLocalValue(editDraft.runAtMs ?? defaultOnceAtMs)}
                    onChange={(event) => {
                      const ms = new Date(event.target.value).getTime()
                      setEditDraft((current) =>
                        current && Number.isFinite(ms) ? { ...current, runAtMs: ms } : current,
                      )
                    }}
                  />
                </label>
                <label className="task-edit-field task-edit-field-checkbox">
                  <input
                    type="checkbox"
                    checked={editDraft.resultInNewSession ?? false}
                    onChange={(event) =>
                      setEditDraft((current) =>
                        current ? { ...current, resultInNewSession: event.target.checked } : current,
                      )
                    }
                  />
                  <span className="task-edit-label-inline">在独立会话中展示结果（会话列表会出现新会话）</span>
                </label>
              </>
            ) : (
              <>
                <label className="task-edit-field">
                  <span className="task-edit-label">
                    {editDraft.scheduleType === 'weekly_time'
                      ? '每周时间'
                      : editDraft.scheduleType === 'monthly_time'
                        ? '每月时间'
                        : '每日时间'}
                  </span>
                  <input
                    className="task-edit-control"
                    value={editDraft.dailyTimes.join(', ')}
                    onChange={(event) =>
                      setEditDraft((current) =>
                        current ? { ...current, dailyTimes: parseDailyTimesInput(event.target.value) } : current,
                      )
                    }
                    placeholder="09:00, 18:30"
                  />
                </label>
                {editDraft.scheduleType === 'weekly_time' ? (
                  <label className="task-edit-field">
                    <span className="task-edit-label">星期几（1=周一，7=周日）</span>
                    <input
                      className="task-edit-control"
                      value={editDraft.weeklyDays.join(', ')}
                      onChange={(event) =>
                        setEditDraft((current) =>
                          current
                            ? { ...current, weeklyDays: parseNumericDaysInput(event.target.value, 1, 7) }
                            : current,
                        )
                      }
                      placeholder="1, 3, 5"
                    />
                    {editDraft.weeklyDays.length > 0 ? (
                      <span className="task-edit-label-inline">当前：{formatWeeklyDays(editDraft.weeklyDays)}</span>
                    ) : null}
                  </label>
                ) : null}
                {editDraft.scheduleType === 'monthly_time' ? (
                  <label className="task-edit-field">
                    <span className="task-edit-label">每月几号</span>
                    <input
                      className="task-edit-control"
                      value={editDraft.monthlyDays.join(', ')}
                      onChange={(event) =>
                        setEditDraft((current) =>
                          current
                            ? { ...current, monthlyDays: parseNumericDaysInput(event.target.value, 1, 31) }
                            : current,
                        )
                      }
                      placeholder="5, 20"
                    />
                    {editDraft.monthlyDays.length > 0 ? (
                      <span className="task-edit-label-inline">当前：{formatMonthlyDays(editDraft.monthlyDays)}</span>
                    ) : null}
                  </label>
                ) : null}
              </>
            )}
            <label className="task-edit-field">
              <span className="task-edit-label">时区</span>
              <input
                className="task-edit-control"
                value={editDraft.timezone}
                onChange={(event) =>
                  setEditDraft((current) => (current ? { ...current, timezone: event.target.value } : current))
                }
                placeholder="Asia/Shanghai"
              />
            </label>
            <label className="task-edit-field task-edit-field-grow">
              <span className="task-edit-label">任务内容</span>
              <textarea
                className="task-edit-control task-edit-textarea"
                rows={4}
                value={editDraft.goal}
                onChange={(event) =>
                  setEditDraft((current) => (current ? { ...current, goal: event.target.value } : current))
                }
              />
            </label>
          </div>
        </section>

        <div className="task-edit-footer">
          <button type="button" className="primary-cta task-edit-save" disabled={actionBusy} onClick={onSave}>
            {actionBusy ? '保存中…' : '保存'}
          </button>
        </div>
      </div>
    </div>
  )
}

export type TasksViewProps = {
  agents: AgentRecord[]
  onOpenAgent: (agentId: string) => void
}

export function TasksView({ agents, onOpenAgent }: TasksViewProps) {
  const [tasks, setTasks] = useState<AgentTaskListItem[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [actionTaskId, setActionTaskId] = useState('')
  const [search, setSearch] = useState('')
  const [agentFilter, setAgentFilter] = useState('')
  const [detailTaskId, setDetailTaskId] = useState('')
  const [editDraft, setEditDraft] = useState<AgentTaskUpdateInput | null>(null)
  const [taskDeleteConfirmOpen, setTaskDeleteConfirmOpen] = useState(false)
  const [taskDeleteSubmitting, setTaskDeleteSubmitting] = useState(false)
  const toast = useToast()

  const refreshTasks = useCallback(async () => {
    setLoading(true)
    setError('')
    try {
      const items = await listAgentTasks(agentFilter.trim() || null)
      setTasks(items)
    } catch (fetchError) {
      setError(fetchError instanceof Error ? fetchError.message : String(fetchError))
    } finally {
      setLoading(false)
    }
  }, [agentFilter])

  useEffect(() => {
    void refreshTasks()
  }, [refreshTasks])

  const visibleTasks = useMemo(() => {
    const keyword = search.trim().toLowerCase()
    if (!keyword) {
      return tasks
    }
    return tasks.filter((task) =>
      [
        task.title,
        task.goal,
        task.intentSummary,
        task.agentName,
        task.agentId,
        task.sourceSessionId,
      ]
        .join(' ')
        .toLowerCase()
        .includes(keyword),
    )
  }, [search, tasks])

  const openTaskDetail = useCallback((task: AgentTaskListItem) => {
    setDetailTaskId(task.id)
    setEditDraft({
      title: task.title,
      goal: task.goal || task.intentSummary,
      scheduleType: task.scheduleType,
      timezone: task.timezone || 'Asia/Shanghai',
      intervalMinutes: task.intervalMinutes ?? null,
      dailyTimes: task.dailyTimes ?? [],
      weeklyDays: task.weeklyDays ?? [],
      monthlyDays: task.monthlyDays ?? [],
      runAtMs: task.runAtMs ?? task.nextRunAt ?? null,
      resultInNewSession: task.resultInNewSession ?? false,
    })
  }, [])

  const closeTaskDetail = useCallback(() => {
    setDetailTaskId('')
    setEditDraft(null)
    setTaskDeleteConfirmOpen(false)
    setTaskDeleteSubmitting(false)
  }, [])

  useEffect(() => {
    if (!detailTaskId) {
      return
    }
    const onKeyDown = (event: Event) => {
      if (event instanceof KeyboardEvent && event.key === 'Escape') {
        closeTaskDetail()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [detailTaskId, closeTaskDetail])

  const detailTask = detailTaskId ? tasks.find((t) => t.id === detailTaskId) : undefined
  const taskSections = useMemo(() => groupAgentTasksByStatus(visibleTasks), [visibleTasks])

  if (detailTaskId) {
    if (!detailTask || !editDraft) {
      return (
        <div className="page-shell task-center-page task-linear-page task-edit-page">
          <header className="page-header task-linear-page-toolbar task-edit-detail-header">
            <div className="task-edit-header-leading">
              <button type="button" className="outline-button task-edit-header-back" onClick={closeTaskDetail} aria-label="返回">
                <AppIcon name="arrow-left" size={18} />
                <span>返回</span>
              </button>
              <div className="task-linear-page-toolbar-text task-edit-header-titles">
                <h1>任务不可用</h1>
              </div>
            </div>
          </header>
          <div className="task-center-body task-edit-body">
            <p className="task-linear-empty-hint">该任务可能已被删除或不在当前筛选结果中。</p>
            <button type="button" className="primary-cta task-linear-toolbar-cta" onClick={closeTaskDetail}>
              返回列表
            </button>
          </div>
        </div>
      )
    }

    const actionBusy = actionTaskId === detailTask.id

    return (
      <>
        <TaskCenterEditPage
          task={detailTask}
          editDraft={editDraft}
          setEditDraft={setEditDraft}
          actionBusy={actionBusy}
          onBack={closeTaskDetail}
          onOpenAgent={onOpenAgent}
          onSave={() => {
            setActionTaskId(detailTask.id)
            void updateAgentTask(detailTask.id, editDraft)
              .then(async () => {
                await refreshTasks()
                toast.success('任务已保存。')
              })
              .catch((taskError: unknown) => {
                setError(taskError instanceof Error ? taskError.message : String(taskError))
              })
              .finally(() => setActionTaskId(''))
          }}
          onRunNow={() => {
            setActionTaskId(detailTask.id)
            void runAgentTaskNow(detailTask.id)
              .then(async () => {
                await refreshTasks()
                toast.success('已触发立即执行。')
              })
              .catch((runError: unknown) => {
                setError(runError instanceof Error ? runError.message : String(runError))
              })
              .finally(() => setActionTaskId(''))
          }}
          onPause={() => {
            setActionTaskId(detailTask.id)
            void pauseAgentTask(detailTask.id)
              .then(async () => {
                await refreshTasks()
                toast.success('任务已暂停。')
              })
              .catch((taskError: unknown) => {
                setError(taskError instanceof Error ? taskError.message : String(taskError))
              })
              .finally(() => setActionTaskId(''))
          }}
          onResume={() => {
            setActionTaskId(detailTask.id)
            void resumeAgentTask(detailTask.id)
              .then(async () => {
                await refreshTasks()
                toast.success('任务已恢复。')
              })
              .catch((taskError: unknown) => {
                setError(taskError instanceof Error ? taskError.message : String(taskError))
              })
              .finally(() => setActionTaskId(''))
          }}
          onDelete={() => setTaskDeleteConfirmOpen(true)}
        />
        {taskDeleteConfirmOpen ? (
          <div
            className="confirm-dialog-overlay"
            role="presentation"
            onClick={() => {
              if (!taskDeleteSubmitting) {
                setTaskDeleteConfirmOpen(false)
              }
            }}
          >
            <div
              className="confirm-dialog"
              role="alertdialog"
              aria-modal="true"
              aria-labelledby="task-delete-confirm-title"
              onClick={(event) => event.stopPropagation()}
            >
              <h3 id="task-delete-confirm-title">删除定时任务</h3>
              <p>
                确定要删除「{detailTask.title?.trim() || '此任务'}」吗？删除后无法恢复，相关调度也会停止。
              </p>
              <div className="confirm-dialog-actions">
                <button
                  type="button"
                  className="outline-button"
                  onClick={() => setTaskDeleteConfirmOpen(false)}
                  disabled={taskDeleteSubmitting}
                >
                  取消
                </button>
                <button
                  type="button"
                  className="outline-button confirm-dialog-delete"
                  disabled={taskDeleteSubmitting}
                  onClick={() => {
                    setTaskDeleteSubmitting(true)
                    setActionTaskId(detailTask.id)
                    void deleteAgentTask(detailTask.id)
                      .then(async () => {
                        setTaskDeleteConfirmOpen(false)
                        closeTaskDetail()
                        await refreshTasks()
                        toast.success('任务已删除。')
                      })
                      .catch((taskError: unknown) => {
                        setError(taskError instanceof Error ? taskError.message : String(taskError))
                      })
                      .finally(() => {
                        setTaskDeleteSubmitting(false)
                        setActionTaskId('')
                      })
                  }}
                >
                  {taskDeleteSubmitting ? '删除中…' : '删除'}
                </button>
              </div>
            </div>
          </div>
        ) : null}
      </>
    )
  }

  return (
    <div className="page-shell task-center-page task-linear-page">
      <header className="page-header task-linear-page-toolbar">
        <div className="task-linear-page-toolbar-text">
          <h1>任务中心</h1>
          <p className="task-linear-page-sub">由智能体在对话中创建的定时任务，按状态分组；点按一行进入编辑。</p>
          {!loading && visibleTasks.length === 0 ? <p className="task-center-header-hint">还没有定时任务</p> : null}
        </div>
        <button
          type="button"
          className="primary-cta task-linear-toolbar-cta"
          onClick={() => void refreshTasks()}
          disabled={loading}
        >
          <AppIcon name="refresh" size={16} />
          <span>{loading ? '刷新中…' : '刷新'}</span>
        </button>
      </header>

      <div className="task-center-body task-linear-body">
        <div className="task-linear-filters">
          <label className="search-field task-linear-filter-search">
            <AppIcon name="search" size={18} />
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="按标题、内容、会话、智能体筛选…"
            />
          </label>
          <div className="task-linear-filter-select-wrap">
            <select
              className="task-edit-control task-linear-filter-select"
              value={agentFilter}
              onChange={(event) => setAgentFilter(event.target.value)}
              aria-label="按智能体筛选"
            >
              <option value="">全部智能体</option>
              {agents.map((agent) => (
                <option key={agent.id} value={agent.id}>
                  {agent.name}
                </option>
              ))}
            </select>
          </div>
        </div>

        {error ? (
          <div className="skills-feedback error agent-feedback inline task-center-error">
            <span>{error}</span>
          </div>
        ) : null}

        {visibleTasks.length > 0 ? (
          <div className="task-linear-table-outer">
            {taskSections.map((section) => (
              <section key={section.key} className="task-linear-group">
                <div className="task-linear-group-bar">
                  <span className="task-linear-group-title">{section.label}</span>
                  <span className="task-linear-group-count">{section.items.length}</span>
                </div>
                <div className="task-linear-table-wrap" role="grid" aria-label={`${section.label}任务`}>
                  <div className="task-linear-thead" role="row">
                    <div className="task-linear-th task-linear-col-name" role="columnheader">
                      任务
                    </div>
                    <div className="task-linear-th task-linear-col-schedule" role="columnheader">
                      调度
                    </div>
                    <div className="task-linear-th task-linear-col-status" role="columnheader">
                      状态
                    </div>
                    <div className="task-linear-th task-linear-col-agent" role="columnheader">
                      智能体
                    </div>
                    <div className="task-linear-th task-linear-col-last" role="columnheader">
                      上次执行
                    </div>
                  </div>
                  {section.items.map((task) => {
                    const subtitle = (task.intentSummary || task.goal || '').trim()
                    const subtitleShort = subtitle.length > 72 ? `${subtitle.slice(0, 72)}…` : subtitle
                    return (
                      <button
                        key={task.id}
                        type="button"
                        className="task-linear-row"
                        onClick={() => openTaskDetail(task)}
                      >
                        <div className="task-linear-col task-linear-col-name">
                          <span className="task-linear-row-title">{task.title.trim() || '未命名任务'}</span>
                          {subtitleShort ? <span className="task-linear-row-sub">{subtitleShort}</span> : null}
                        </div>
                        <div className="task-linear-col task-linear-col-schedule">
                          <span className="task-linear-row-primary">{formatAgentTaskScheduleShort(task)}</span>
                          {task.nextRunAt ? (
                            <span className="task-linear-row-sub">下次 {formatOptionalAbsoluteTime(task.nextRunAt)}</span>
                          ) : null}
                        </div>
                        <div className="task-linear-col task-linear-col-status">
                          <span className="task-linear-status-pill">{formatAgentTaskStatus(task.status)}</span>
                        </div>
                        <div className="task-linear-col task-linear-col-agent">
                          <span className="task-linear-row-primary">{task.agentName}</span>
                        </div>
                        <div className="task-linear-col task-linear-col-last">
                          <span className="task-linear-row-primary">{formatOptionalAbsoluteTime(task.lastRunAt)}</span>
                        </div>
                      </button>
                    )
                  })}
                </div>
              </section>
            ))}
          </div>
        ) : (
          <section className="task-center-panel task-center-panel-empty task-linear-empty">
            {loading ? (
              <p className="task-center-empty-title">正在读取任务…</p>
            ) : (
              <p className="task-center-empty-desc">
                在聊天里说「每 10 分钟…」「每天 9 点…」「每周一 9 点…」「每月 5 号 9 点…」或「一次性…」，也可以在任务中心打开已有任务后改触发方式。
              </p>
            )}
          </section>
        )}
      </div>
    </div>
  )
}
