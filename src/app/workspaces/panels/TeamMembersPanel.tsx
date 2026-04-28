import { useCallback, useMemo, useState } from 'react'
import { AgentAvatar } from '../../../components/AgentAvatar'
import { getAgentColor } from '../../lib'
import type { AgentRecord, WorkspaceMemberView } from '../../../types'

export type TeamMembersPanelProps = {
  members: WorkspaceMemberView[]
  agents: AgentRecord[]
  /** 其他团队（不含当前），用于「从其他工作区复制成员」 */
  peerWorkspaces: { id: string; name: string }[]
  onAddMembersBatch: (agentIds: string[]) => Promise<void>
  onImportMembersFromWorkspace: (sourceWorkspaceId: string) => Promise<void>
  onRemoveMember: (agentId: string) => void
}

export function TeamMembersPanel({
  members,
  agents,
  peerWorkspaces,
  onAddMembersBatch,
  onImportMembersFromWorkspace,
  onRemoveMember,
}: TeamMembersPanelProps) {
  const [search, setSearch] = useState('')
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [batchBusy, setBatchBusy] = useState(false)
  const [importSourceId, setImportSourceId] = useState('')
  const [importBusy, setImportBusy] = useState(false)

  const memberIdSet = useMemo(() => new Set(members.map((m) => m.agentId)), [members])

  const nonMemberAgents = useMemo(() => {
    return agents.filter((a) => !memberIdSet.has(a.id))
  }, [agents, memberIdSet])

  const filteredNonMembers = useMemo(() => {
    const q = search.trim().toLowerCase()
    if (!q) return nonMemberAgents
    return nonMemberAgents.filter((a) => {
      const blob = `${a.name} ${a.id} ${a.summary ?? ''} ${a.description ?? ''}`.toLowerCase()
      return blob.includes(q)
    })
  }, [nonMemberAgents, search])

  const toggle = useCallback((id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }, [])

  const selectAllFiltered = useCallback(() => {
    setSelectedIds((prev) => {
      const next = new Set(prev)
      for (const a of filteredNonMembers) {
        next.add(a.id)
      }
      return next
    })
  }, [filteredNonMembers])

  const clearSelection = useCallback(() => setSelectedIds(new Set()), [])

  const addSelected = async () => {
    const ids = [...selectedIds].filter((id) => nonMemberAgents.some((a) => a.id === id))
    if (ids.length === 0) return
    setBatchBusy(true)
    try {
      await onAddMembersBatch(ids)
      setSelectedIds(new Set())
      setSearch('')
    } finally {
      setBatchBusy(false)
    }
  }

  const runImport = async () => {
    if (!importSourceId.trim()) return
    setImportBusy(true)
    try {
      await onImportMembersFromWorkspace(importSourceId.trim())
      setImportSourceId('')
    } finally {
      setImportBusy(false)
    }
  }

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
              <AgentAvatar
                name={m.name}
                avatarUri={m.avatarUri}
                accentColor={agents.find((agent) => agent.id === m.agentId)?.accentColor ?? null}
                className="workspace-member-avatar"
                size={16}
              />
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

      {peerWorkspaces.length > 0 ? (
        <div className="team-member-import-block">
          <div className="team-member-import-label">从其他工作区复制成员</div>
          <div className="team-member-import-row">
            <select
              className="workspaces-input workspaces-select"
              value={importSourceId}
              onChange={(e) => setImportSourceId(e.target.value)}
              aria-label="选择要复制成员的工作区"
            >
              <option value="">选择工作区…</option>
              {peerWorkspaces.map((w) => (
                <option key={w.id} value={w.id}>
                  {w.name || w.id}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="outline-button team-member-import-btn"
              disabled={!importSourceId || importBusy}
              onClick={() => void runImport()}
            >
              {importBusy ? '导入中…' : '导入'}
            </button>
          </div>
          <p className="team-member-import-hint">仅加入「当前团队还没有」的智能体，已存在的会跳过。</p>
        </div>
      ) : null}

      {nonMemberAgents.length > 0 ? (
        <div className="team-member-add-block">
          <div className="team-member-add-label">从智能体库加入</div>
          <label className="team-member-search-field">
            <span className="visually-hidden">搜索</span>
            <input
              type="search"
              className="workspaces-input"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="搜索名称、ID、简介…"
              autoComplete="off"
            />
          </label>
          <div className="team-member-pick-toolbar">
            <button type="button" className="workspace-inline-link" onClick={selectAllFiltered}>
              全选当前列表
            </button>
            <button type="button" className="workspace-inline-link" onClick={clearSelection}>
              清除选择
            </button>
          </div>
          <ul className="team-member-pick-list" role="listbox" aria-label="待加入智能体">
            {filteredNonMembers.map((a) => {
              const checked = selectedIds.has(a.id)
              const accent = getAgentColor(a)
              return (
                <li key={a.id} className="team-member-pick-row">
                  <label className="team-member-pick-label">
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggle(a.id)}
                    />
                    <AgentAvatar
                      name={a.name}
                      avatarUri={a.avatarUri}
                      accentColor={accent}
                      className="workspace-member-avatar"
                      size={16}
                    />
                    <span className="team-member-pick-name">{a.name}</span>
                    <span className="team-member-pick-id">{a.id}</span>
                  </label>
                </li>
              )
            })}
          </ul>
          {filteredNonMembers.length === 0 ? (
            <p className="workspace-hint">没有匹配的智能体，换个关键词试试。</p>
          ) : null}
          <button
            type="button"
            className="primary-cta team-member-add-batch"
            disabled={
              batchBusy ||
              [...selectedIds].every((id) => !nonMemberAgents.some((a) => a.id === id))
            }
            onClick={() => void addSelected()}
          >
            {batchBusy
              ? '加入中…'
              : `添加选中（${[...selectedIds].filter((id) => nonMemberAgents.some((a) => a.id === id)).length}）`}
          </button>
        </div>
      ) : (
        <p className="workspace-hint">已把所有智能体拉进团队。</p>
      )}
    </div>
  )
}

export default TeamMembersPanel
