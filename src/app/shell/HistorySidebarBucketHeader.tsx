import type { KeyboardEvent, MouseEvent } from 'react'
import { AppIcon } from '../../components/AppIcon'
import type { HistorySidebarBucket, HistorySidebarItem } from '../../lib/historySidebarBuckets'

type HistorySidebarBucketHeaderProps = {
  bucket: HistorySidebarBucket<HistorySidebarItem>
  open: boolean
  editingGroupId: string
  editingGroupName: string
  onToggle: (key: string) => void
  onOpenGroupMenu: (event: MouseEvent<HTMLButtonElement>, bucket: HistorySidebarBucket<HistorySidebarItem>) => void
  onEditGroupNameChange: (name: string) => void
  onConfirmGroupEdit: () => void
  onCancelGroupEdit: () => void
}

export function HistorySidebarBucketHeader({
  bucket,
  open,
  editingGroupId,
  editingGroupName,
  onToggle,
  onOpenGroupMenu,
  onEditGroupNameChange,
  onConfirmGroupEdit,
  onCancelGroupEdit,
}: HistorySidebarBucketHeaderProps) {
  const handleEditKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault()
      onConfirmGroupEdit()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      onCancelGroupEdit()
    }
  }

  if (editingGroupId && bucket.groupId === editingGroupId) {
    return (
      <div className="history-bucket-edit-row">
        <input
          className="history-bucket-edit-input"
          value={editingGroupName}
          autoFocus
          onChange={(event) => onEditGroupNameChange(event.target.value)}
          onKeyDown={handleEditKeyDown}
          aria-label="编辑历史分组名称"
        />
        <button type="button" className="history-bucket-icon-button" onClick={onConfirmGroupEdit} aria-label="保存分组名称">
          <AppIcon name="check" size={14} />
        </button>
        <button type="button" className="history-bucket-icon-button" onClick={onCancelGroupEdit} aria-label="取消编辑分组名称">
          <AppIcon name="close" size={14} />
        </button>
      </div>
    )
  }

  return (
    <div className={`history-bucket-head-row ${bucket.kind === 'group' ? 'is-group' : ''}`}>
      <button
        type="button"
        className="history-bucket-head"
        aria-expanded={open}
        onClick={() => onToggle(bucket.key)}
      >
        <span className={`history-bucket-chevron-wrap${open ? ' is-open' : ''}`}>
          <AppIcon name="chevron-down" size={14} />
        </span>
        <span className="history-bucket-label">{bucket.label}</span>
        <span className="history-bucket-count">{bucket.items.length}</span>
      </button>
      {bucket.kind === 'group' && bucket.groupId ? (
        <button
          type="button"
          className="history-bucket-menu-button"
          aria-label={`打开「${bucket.label}」分组菜单`}
          title="分组菜单"
          onClick={(event) => onOpenGroupMenu(event, bucket)}
          onContextMenu={(event) => onOpenGroupMenu(event, bucket)}
        >
          <AppIcon name="more" size={15} />
        </button>
      ) : null}
    </div>
  )
}
