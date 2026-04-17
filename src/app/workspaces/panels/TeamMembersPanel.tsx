import { useMemo } from 'react'
import type { AgentRecord, WorkspaceMemberView } from '../../../types'

export type TeamMembersPanelProps = {
  members: WorkspaceMemberView[]
  agents: AgentRecord[]
  newMemberId: string
  onNewMemberIdChange: (id: string) => void
  onAddMember: () => void
  onRemoveMember: (agentId: string) => void
}

export function TeamMembersPanel({
  members,
  agents,
  newMemberId,
  onNewMemberIdChange,
  onAddMember,
  onRemoveMember,
}: TeamMembersPanelProps) {
  const nonMemberAgents = useMemo(() => {
    const ids = new Set(members.map((m) => m.agentId))
    return agents.filter((a) => !ids.has(a.id))
  }, [agents, members])

  return (
    <div className="task-center-panel workspace-panel">
      <div className="workspace-panel-head">
        <strong className="workspace-panel-title">成员</strong>
        <span className="workspace-panel-meta">{members.length} 人</span>
      </div>
      <ul className="workspace-member-list">
        {members.map((m) => (
          <li
            key={m.agentId}
            className={`workspace-member-item${m.role === 'supervisor' ? ' supervisor' : ''}`}
          >
            <div className="workspace-member-identity">
              <span className="workspace-member-name">{m.name}</span>
              <span className={`workspace-member-role${m.role === 'supervisor' ? ' supervisor' : ''}`}>
                {m.role === 'supervisor' ? '主智能体' : '成员'}
              </span>
            </div>
            {m.role !== 'supervisor' ? (
              <button
                type="button"
                className="workspace-inline-link"
                onClick={() => onRemoveMember(m.agentId)}
              >
                移除
              </button>
            ) : null}
          </li>
        ))}
      </ul>
      {nonMemberAgents.length > 0 ? (
        <div className="workspace-inline-form">
          <select
            className="workspaces-input workspaces-select"
            value={newMemberId}
            onChange={(event) => onNewMemberIdChange(event.target.value)}
          >
            {nonMemberAgents.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="primary-cta workspace-inline-cta"
            onClick={onAddMember}
            disabled={!newMemberId}
          >
            加入
          </button>
        </div>
      ) : (
        <p className="workspace-hint">已把所有智能体拉进团队。</p>
      )}
    </div>
  )
}

export default TeamMembersPanel
