import { useEffect, useState, type FormEvent } from 'react'
import type { AgentRecord, WorkspaceRecord } from '../../types'
import {
  listAgents,
  workspaceCreate,
  workspaceList,
  workspaceSetArchived,
} from '../../lib/piClient'
import { AppIcon } from '../../components/AppIcon'

export type WorkspacesViewProps = {
  /** 点击任一团队时上抛，由外层切到 `WorkspaceChatPage` */
  onSelectWorkspace?: (id: string) => void
}

/** 团队空间首页：列表 + 新建。点击团队由外层切换到聊天页。 */
export function WorkspacesView({ onSelectWorkspace }: WorkspacesViewProps = {}) {
  const [workspaces, setWorkspaces] = useState<WorkspaceRecord[]>([])
  const [agents, setAgents] = useState<AgentRecord[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [creating, setCreating] = useState(false)
  const [draftName, setDraftName] = useState('')
  const [draftDesc, setDraftDesc] = useState('')
  const [draftSupervisor, setDraftSupervisor] = useState('')

  const refresh = async () => {
    setLoading(true)
    setError('')
    try {
      const [ws, ag] = await Promise.all([workspaceList(false), listAgents()])
      setWorkspaces(ws)
      setAgents(ag.filter((a) => !a.isArchived))
      if (ag.length > 0 && !draftSupervisor) {
        setDraftSupervisor(ag[0]?.id ?? '')
      }
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    void refresh()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const onCreate = async (event: FormEvent) => {
    event.preventDefault()
    if (!draftName.trim() || !draftSupervisor) {
      setError('请填写名称并选择主智能体')
      return
    }
    try {
      setLoading(true)
      const ws = await workspaceCreate({
        name: draftName.trim(),
        description: draftDesc.trim(),
        supervisorAgentId: draftSupervisor,
      })
      setDraftName('')
      setDraftDesc('')
      setCreating(false)
      await refresh()
      onSelectWorkspace?.(ws.id)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }

  const onArchive = async (id: string) => {
    try {
      await workspaceSetArchived(id, true)
      await refresh()
    } catch (e) {
      setError(String(e))
    }
  }

  return (
    <div className="page-shell task-center-page workspaces-page">
      <header className="page-header workspaces-topbar">
        <div className="workspaces-topbar-text">
          <h1>团队空间</h1>
          <p>把多个智能体组队成工作空间，共享项目材料与记忆，由主智能体统一接收任务并派发。</p>
        </div>
        <button
          type="button"
          className="primary-cta workspaces-topbar-cta"
          onClick={() => setCreating((v) => !v)}
        >
          <AppIcon name="plus" size={16} />
          <span>{creating ? '收起' : '新建团队'}</span>
        </button>
      </header>

      <div className="task-center-body workspaces-body">
        {error ? (
          <div className="skills-feedback error agent-feedback inline task-center-error">
            <span>{error}</span>
          </div>
        ) : null}

        {creating ? (
          <form className="task-center-panel workspaces-create-panel" onSubmit={onCreate}>
            <div className="workspaces-form-grid">
              <label className="workspaces-field">
                <span className="workspaces-field-label">名称</span>
                <input
                  className="workspaces-input"
                  value={draftName}
                  onChange={(event) => setDraftName(event.target.value)}
                  placeholder="例如：产品发布小组"
                  autoFocus
                />
              </label>
              <label className="workspaces-field">
                <span className="workspaces-field-label">主智能体</span>
                <select
                  className="workspaces-input workspaces-select"
                  value={draftSupervisor}
                  onChange={(event) => setDraftSupervisor(event.target.value)}
                >
                  {agents.map((a) => (
                    <option value={a.id} key={a.id}>
                      {a.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="workspaces-field workspaces-field-wide">
                <span className="workspaces-field-label">简介</span>
                <textarea
                  className="workspaces-input workspaces-textarea"
                  rows={2}
                  value={draftDesc}
                  onChange={(event) => setDraftDesc(event.target.value)}
                  placeholder="团队目标、里程碑、分工……"
                />
              </label>
            </div>
            <div className="workspaces-form-actions">
              <button type="button" className="outline-button" onClick={() => setCreating(false)}>
                取消
              </button>
              <button type="submit" className="primary-cta" disabled={loading}>
                创建
              </button>
            </div>
          </form>
        ) : null}

        {workspaces.length === 0 && !loading ? (
          <section className="task-center-panel task-center-panel-empty workspaces-empty">
            <p className="task-center-empty-title">还没有团队空间</p>
            <p className="task-center-empty-desc">点「新建团队」选一位主智能体，再拉其他成员进来共享材料与记忆。</p>
          </section>
        ) : null}

        {workspaces.length > 0 ? (
          <div className="workspaces-grid">
            {workspaces.map((w) => {
              const supervisor = agents.find((a) => a.id === w.supervisorAgentId)
              return (
                <button
                  type="button"
                  key={w.id}
                  className="workspace-card"
                  onClick={() => onSelectWorkspace?.(w.id)}
                >
                  <div className="workspace-card-head">
                    <strong className="workspace-card-title">{w.name}</strong>
                    <span className="workspace-card-badge">主：{supervisor?.name ?? w.supervisorAgentId}</span>
                  </div>
                  <p className="workspace-card-desc">{w.description || '（暂无描述）'}</p>
                  <div className="workspace-card-foot">
                    <span className="workspace-card-time">
                      创建 {new Date(w.createdAt).toLocaleDateString()}
                    </span>
                    <span
                      role="button"
                      tabIndex={0}
                      className="workspace-card-archive"
                      onClick={(event) => {
                        event.stopPropagation()
                        void onArchive(w.id)
                      }}
                    >
                      归档
                    </span>
                  </div>
                </button>
              )
            })}
          </div>
        ) : null}
      </div>
    </div>
  )
}

export default WorkspacesView
