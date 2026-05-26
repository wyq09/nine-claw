import { useMemo, useState, type MouseEvent } from 'react'
import type { HistoryItem } from '../../types'
import { AppIcon } from '../../components/AppIcon'
import { AgentAvatar } from '../../components/AgentAvatar'
import { groupHistoryIntoSidebarBuckets } from '../../lib/historySidebarBuckets'
import { useHistorySidebarBucketsExpanded } from '../../hooks/useHistorySidebarBucketsExpanded'
import { getHistorySidebarCardMeta } from '../lib/historySidebarCardMeta'

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

  const groups = useMemo(() => groupHistoryIntoSidebarBuckets(filteredSessions), [filteredSessions])
  const { mergedBucketOpen, toggleBucket } = useHistorySidebarBucketsExpanded(groups, activeHistoryId)

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

        {groups.map((bucket) => (
          <section key={bucket.key} className="history-bucket workspace-sessions-history-bucket">
            <button
              type="button"
              className="history-bucket-head"
              aria-expanded={mergedBucketOpen[bucket.key]}
              onClick={() => toggleBucket(bucket.key)}
            >
              <span className={`history-bucket-chevron-wrap${mergedBucketOpen[bucket.key] ? ' is-open' : ''}`}>
                <AppIcon name="chevron-down" size={14} />
              </span>
              <span className="history-bucket-label">{bucket.label}</span>
              <span className="history-bucket-count">{bucket.items.length}</span>
            </button>
            {mergedBucketOpen[bucket.key] ? (
              <ul className="history-bucket-items workspace-sessions-bucket-ul">
                {bucket.items.map((item) => {
                  const isActive = item.id === activeHistoryId
                  const meta = getHistorySidebarCardMeta(item.status, item.updatedAt || item.createdAt)
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
                            <div className="workspace-sessions-item-meta-row">
                              <span className={`workspace-sessions-item-meta-icon ${meta.tone}`}>
                                <AppIcon name={meta.icon} size={12} />
                              </span>
                              <span className={`workspace-sessions-item-meta-label ${meta.tone}`}>{meta.label}</span>
                            </div>
                          </div>
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
            ) : null}
          </section>
        ))}
      </div>
    </aside>
  )
}

export default WorkspaceSessionsSidebar
