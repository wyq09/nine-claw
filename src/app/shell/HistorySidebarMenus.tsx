import { AppIcon } from '../../components/AppIcon'
import type { HistorySidebarGroup } from '../../lib/historySidebarMeta'
import type { HistoryContextMenuState } from './historyContextMenuTypes'

type HistorySidebarMenusProps = {
  menu: HistoryContextMenuState
  groups: HistorySidebarGroup[]
  onClose: () => void
  onRenameSession: (sessionId: string, title: string) => void
  onTogglePinned: (sessionId: string, pinned: boolean) => void
  onAssignToGroup: (sessionId: string, groupId: string | null) => void
  onCreateGroup: (sessionId: string) => void
  onCopySession: (sessionId: string) => void
  onRegenerateTitle: (sessionId: string) => void
  onRequestDelete: (sessionId: string) => void
  onRenameGroup: (groupId: string, name: string) => void
  onRegenerateGroupName: (groupId: string) => void
  onDissolveGroup: (groupId: string) => void
}

export function HistorySidebarMenus({
  menu,
  groups,
  onClose,
  onRenameSession,
  onTogglePinned,
  onAssignToGroup,
  onCreateGroup,
  onCopySession,
  onRegenerateTitle,
  onRequestDelete,
  onRenameGroup,
  onRegenerateGroupName,
  onDissolveGroup,
}: HistorySidebarMenusProps) {
  const runAndClose = (action: () => void) => {
    action()
    onClose()
  }

  return (
    <>
      <button
        type="button"
        className="context-menu-backdrop"
        aria-label="关闭会话菜单"
        onClick={onClose}
      />
      <div
        className="history-context-menu"
        role="menu"
        style={{ left: menu.x, top: menu.y }}
      >
        {menu.kind === 'session' ? (
          <>
            <button
              type="button"
              className="history-context-menu-item"
              role="menuitem"
              onClick={() => runAndClose(() => onRenameSession(menu.sessionId, menu.title))}
            >
              <AppIcon name="wrench" size={16} />
              <span>重命名</span>
            </button>
            <button
              type="button"
              className="history-context-menu-item"
              role="menuitem"
              onClick={() => runAndClose(() => onTogglePinned(menu.sessionId, !menu.pinned))}
            >
              <AppIcon name="arrow-up" size={16} />
              <span>{menu.pinned ? '取消置顶' : '置顶'}</span>
            </button>
            <div className="history-context-menu-submenu">
              <button type="button" className="history-context-menu-item" role="menuitem">
                <AppIcon name="folder" size={16} />
                <span>添加到分组</span>
                <span className="history-context-menu-arrow">›</span>
              </button>
              <div className="history-context-submenu-panel" role="menu">
                {groups.map((group) => (
                  <button
                    type="button"
                    key={group.id}
                    className="history-context-menu-item"
                    role="menuitem"
                    onClick={() => runAndClose(() => onAssignToGroup(menu.sessionId, group.id))}
                  >
                    <span className="history-context-menu-check">
                      {menu.groupId === group.id ? <AppIcon name="check" size={14} /> : null}
                    </span>
                    <span>{group.name}</span>
                  </button>
                ))}
                {menu.groupId ? (
                  <button
                    type="button"
                    className="history-context-menu-item"
                    role="menuitem"
                    onClick={() => runAndClose(() => onAssignToGroup(menu.sessionId, null))}
                  >
                    <AppIcon name="close" size={14} />
                    <span>移出分组</span>
                  </button>
                ) : null}
                <button
                  type="button"
                  className="history-context-menu-item"
                  role="menuitem"
                  onClick={() => runAndClose(() => onCreateGroup(menu.sessionId))}
                >
                  <AppIcon name="plus" size={16} />
                  <span>新建分组</span>
                </button>
              </div>
            </div>
            <button
              type="button"
              className="history-context-menu-item"
              role="menuitem"
              onClick={() => runAndClose(() => onCopySession(menu.sessionId))}
            >
              <AppIcon name="book" size={16} />
              <span>复制</span>
            </button>
            <button
              type="button"
              className="history-context-menu-item"
              role="menuitem"
              onClick={() => runAndClose(() => onRegenerateTitle(menu.sessionId))}
            >
              <AppIcon name="refresh" size={16} />
              <span>重新生成标题</span>
            </button>
            <button
              type="button"
              className="history-context-menu-item danger"
              role="menuitem"
              onClick={() => runAndClose(() => onRequestDelete(menu.sessionId))}
              disabled={!menu.canDelete}
              title={menu.canDelete ? `删除「${menu.title}」` : '当前会话仍在生成，暂时不能删除'}
            >
              <AppIcon name="trash" size={16} />
              <span>{menu.canDelete ? '删除' : '生成中，暂不可删'}</span>
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              className="history-context-menu-item"
              role="menuitem"
              onClick={() => runAndClose(() => onRenameGroup(menu.groupId, menu.title))}
            >
              <AppIcon name="wrench" size={16} />
              <span>重命名分组</span>
            </button>
            <button
              type="button"
              className="history-context-menu-item"
              role="menuitem"
              onClick={() => runAndClose(() => onRegenerateGroupName(menu.groupId))}
            >
              <AppIcon name="refresh" size={16} />
              <span>重新生成分组名</span>
            </button>
            <button
              type="button"
              className="history-context-menu-item danger"
              role="menuitem"
              onClick={() => runAndClose(() => onDissolveGroup(menu.groupId))}
            >
              <AppIcon name="trash" size={16} />
              <span>解散分组</span>
            </button>
          </>
        )}
      </div>
    </>
  )
}
