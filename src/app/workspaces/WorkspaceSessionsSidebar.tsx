import { useMemo, useState, type MouseEvent } from 'react'
import type { HistoryItem } from '../../types'
import { AppIcon } from '../../components/AppIcon'
import { AgentAvatar } from '../../components/AgentAvatar'

export type WorkspaceSessionsSidebarProps = {
  workspaceId: string
  history: HistoryItem[]
  activeHistoryId: string | null
  onSelectSession: (id: string) => void
  onStartNewSession: () => void
  onDeleteSession?: (id: string) => void
  collapsed: boolean
  onToggleCollapsed: () => void
}

type SessionGroup = { key: string; label: string; items: HistoryItem[] }

function sessionMatchesQuery(item: HistoryItem, q: string): boolean {
  if (!q) return true
  const hay = [
    item.title,
    ...item.turns.flatMap((t) => [t.prompt, t.answer, t.thinking]),
  ]
    .join('\n')
    .toLowerCase()
  return hay.includes(q)
}

function groupByTime(items: HistoryItem[]): SessionGroup[] {
  const now = new Date()
  const startOfDay = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
  const startOfYesterday = startOfDay - 24 * 3600 * 1000
  const startOfWeek = startOfDay - 7 * 24 * 3600 * 1000

  const today: HistoryItem[] = []
  const yesterday: HistoryItem[] = []
  const week: HistoryItem[] = []
  const earlier: HistoryItem[] = []

  for (const item of items) {
    const ts = item.updatedAt || item.createdAt
    if (ts >= startOfDay) today.push(item)
    else if (ts >= startOfYesterday) yesterday.push(item)
    else if (ts >= startOfWeek) week.push(item)
    else earlier.push(item)
  }

  const groups: SessionGroup[] = []
  if (today.length) groups.push({ key: 'today', label: '今天', items: today })
  if (yesterday.length) groups.push({ key: 'yesterday', label: '昨天', items: yesterday })
  if (week.length) groups.push({ key: 'week', label: '最近七天', items: week })
  if (earlier.length) groups.push({ key: 'earlier', label: '更早', items: earlier })
  return groups
}

export function WorkspaceSessionsSidebar({
  workspaceId,
  history,
  activeHistoryId,
  onSelectSession,
  onStartNewSession,
  onDeleteSession,
  collapsed,
  onToggleCollapsed,
}: WorkspaceSessionsSidebarProps) {
  const [query, setQuery] = useState('')

  const teamSessions = useMemo(
    () => history.filter((item) => item.workspaceId === workspaceId),
    [history, workspaceId],
  )

  const filteredSessions = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!q) return teamSessions
    return teamSessions.filter((item) => sessionMatchesQuery(item, q))
  }, [teamSessions, query])

  const groups = useMemo(() => groupByTime(filteredSessions), [filteredSessions])

  if (collapsed) {
    return (
      <aside className="workspace-sessions-sidebar collapsed" aria-label="团队会话列表">
        <button
          type="button"
          className="workspace-inline-link workspace-sessions-expand"
          onClick={onToggleCollapsed}
          title="展开会话列表"
          aria-label="展开会话列表"
        >
          <AppIcon name="panel" size={16} />
        </button>
        <button
          type="button"
          className="primary-cta workspace-sessions-new-icon"
          onClick={onStartNewSession}
          title="新会话"
          aria-label="新会话"
        >
          <AppIcon name="plus" size={16} />
        </button>
      </aside>
    )
  }

  return (
    <aside className="workspace-sessions-sidebar" aria-label="团队会话列表">
      <div className="workspace-sessions-head">
        <button
          type="button"
          className="primary-cta workspace-sessions-new"
          onClick={onStartNewSession}
        >
          <AppIcon name="plus" size={14} />
          <span>新会话</span>
        </button>
        <button
          type="button"
          className="workspace-inline-link workspace-sessions-collapse"
          onClick={onToggleCollapsed}
          title="收起会话列表"
          aria-label="收起会话列表"
        >
          <AppIcon name="panel" size={14} />
        </button>
      </div>

      <div className="workspace-sessions-search" role="search">
        <span className="workspace-sessions-search-icon" aria-hidden>
          <AppIcon name="search" size={14} />
        </span>
        <input
          type="search"
          enterKeyHint="search"
          autoComplete="off"
          placeholder="搜索标题或对话内容…"
          aria-label="搜索会话"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        {query ? (
          <button
            type="button"
            className="workspace-sessions-search-clear"
            onClick={() => setQuery('')}
            aria-label="清空搜索"
            title="清空"
          >
            <AppIcon name="close" size={12} />
          </button>
        ) : null}
      </div>

      <div className="workspace-sessions-list">
        {teamSessions.length === 0 ? (
          <div className="workspace-sessions-empty">还没有会话。点「新会话」开始。</div>
        ) : null}
        {teamSessions.length > 0 && filteredSessions.length === 0 ? (
          <div className="workspace-sessions-empty">
            没有与「{query.trim()}」匹配的会话。
            <button type="button" className="workspace-sessions-clear-query" onClick={() => setQuery('')}>
              清空筛选
            </button>
          </div>
        ) : null}
        {groups.map((group) => (
          <section key={group.key} className="workspace-sessions-group">
            <header className="workspace-sessions-group-head">{group.label}</header>
            <ul>
	              {group.items.map((item) => {
	                const isActive = item.id === activeHistoryId
	                const lastPrompt = item.turns[item.turns.length - 1]?.prompt ?? ''
                  const agent = item.agent
	                return (
	                  <li key={item.id}>
                    <button
                      type="button"
                      className={`workspace-sessions-item${isActive ? ' active' : ''}`}
                      onClick={() => onSelectSession(item.id)}
	                    >
                        <div className="workspace-sessions-item-main">
                          <AgentAvatar
                            name={agent?.name || item.title || '会话'}
                            avatarUri={agent?.avatarUri}
                            accentColor={agent?.accentColor}
                            className="workspace-sessions-item-avatar"
                            size={16}
                            fallbackToIcon={!agent}
                          />
                          <div className="workspace-sessions-item-copy">
	                          <div className="workspace-sessions-item-title">
	                            {item.title || '未命名会话'}
	                          </div>
                              {agent?.name ? (
                                <div className="workspace-sessions-item-agent">{agent.name}</div>
                              ) : null}
	                          {lastPrompt ? (
	                            <div className="workspace-sessions-item-sub">
	                              {lastPrompt.length > 48 ? `${lastPrompt.slice(0, 48)}…` : lastPrompt}
	                            </div>
	                          ) : null}
                          </div>
                        </div>
	                      <div className="workspace-sessions-item-meta">
	                        <span>{new Date(item.updatedAt || item.createdAt).toLocaleString()}</span>
	                        <span>·</span>
	                        <span>{item.turns.length} 轮</span>
	                      </div>
                      {onDeleteSession ? (
                        <span
                          className="workspace-sessions-item-delete"
                          role="button"
                          tabIndex={0}
                          title="删除会话"
                          onClick={(event: MouseEvent<HTMLSpanElement>) => {
                            event.stopPropagation()
                            onDeleteSession(item.id)
                          }}
                        >
                          <AppIcon name="trash" size={12} />
                        </span>
                      ) : null}
                    </button>
                  </li>
                )
              })}
            </ul>
          </section>
        ))}
      </div>
    </aside>
  )
}

export default WorkspaceSessionsSidebar
