import type { KeyboardEvent, MouseEvent } from 'react'
import { memo } from 'react'
import { AppIcon } from '../../components/AppIcon'
import type { HistorySidebarItem } from '../../lib/historySidebarBuckets'
import { getHistorySidebarCardMeta } from '../lib/historySidebarCardMeta'

type HistorySidebarCardProps = {
  item: HistorySidebarItem
  active: boolean
  onSelect: (id: string) => void
  onContextMenu: (event: MouseEvent<HTMLElement>, item: HistorySidebarItem) => void
  editing: boolean
  editTitle: string
  onStartMenu: (event: MouseEvent<HTMLButtonElement>, item: HistorySidebarItem) => void
  onEditTitleChange: (title: string) => void
  onConfirmEdit: () => void
  onCancelEdit: () => void
}

export const HistorySidebarCard = memo(function HistorySidebarCard({
  item,
  active,
  onSelect,
  onContextMenu,
  editing,
  editTitle,
  onStartMenu,
  onEditTitleChange,
  onConfirmEdit,
  onCancelEdit,
}: HistorySidebarCardProps) {
  const meta = getHistorySidebarCardMeta(item.status, item.updatedAt || item.createdAt)
  const handleEditKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault()
      onConfirmEdit()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      onCancelEdit()
    }
  }

  return (
    <div
      role="listitem"
      className={`history-card ${active ? 'active' : ''} ${item.pinned ? 'is-pinned' : ''}`}
      onContextMenu={(event) => onContextMenu(event, item)}
      title={item.title}
    >
      {editing ? (
        <span className="history-card-edit-row">
          <input
            className="history-card-edit-input"
            value={editTitle}
            autoFocus
            onChange={(event) => onEditTitleChange(event.target.value)}
            onKeyDown={handleEditKeyDown}
            aria-label="编辑会话标题"
          />
          <button type="button" className="history-card-edit-action" onClick={onConfirmEdit} aria-label="保存标题">
            <AppIcon name="check" size={14} />
          </button>
          <button type="button" className="history-card-edit-action" onClick={onCancelEdit} aria-label="取消编辑">
            <AppIcon name="close" size={14} />
          </button>
        </span>
      ) : (
        <>
          <button type="button" className="history-card-main" onClick={() => onSelect(item.id)}>
            <span className="history-card-body">
              <span className="history-card-title">
                {item.pinned ? <AppIcon name="arrow-up" size={12} /> : null}
                <span>{item.title?.trim() || '未命名会话'}</span>
              </span>
              {item.agent ? <span className="history-card-agent">{item.agent.name}</span> : null}
              <span className="history-card-meta-row">
                <span className={`history-card-meta-icon ${meta.tone}`}>
                  <AppIcon name={meta.icon} size={12} />
                </span>
                <span className={`history-card-time ${meta.tone}`}>{meta.label}</span>
              </span>
            </span>
          </button>
          <button
            type="button"
            className="history-card-menu-button"
            onClick={(event) => onStartMenu(event, item)}
            aria-label={`打开「${item.title?.trim() || '未命名会话'}」菜单`}
            title="更多"
          >
            <AppIcon name="more" size={16} />
          </button>
        </>
      )}
    </div>
  )
})
