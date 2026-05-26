import { createPortal } from 'react-dom'
import type { SessionWorkspaceEntry, SessionWorkspaceState } from '../../../lib/sessionWorkspaceClient'
import { shortenWorkspacePath } from './sessionWorkspacePreview'

export type WorkspaceMenuAnchor = {
  top: number
  left: number
  width: number
  placement: 'top' | 'bottom'
  maxHeight: number
}

export type ContextMenuState = {
  x: number
  y: number
  entry: SessionWorkspaceEntry
} | null

export function computeWorkspaceMenuAnchor(rect: DOMRect): WorkspaceMenuAnchor {
  const maxHeightBelow = Math.min(360, window.innerHeight - rect.bottom - 12)
  const maxHeightAbove = Math.min(360, Math.max(160, rect.top - 12))
  const placement = maxHeightBelow >= 120 ? 'bottom' : 'top'

  return {
    top: placement === 'bottom' ? rect.bottom + 4 : rect.top - 4,
    left: rect.left,
    width: rect.width,
    placement,
    maxHeight: placement === 'bottom' ? maxHeightBelow : maxHeightAbove,
  }
}

export function clampFloatingPosition(x: number, y: number, width = 220, height = 320) {
  return {
    left: Math.max(8, Math.min(x, window.innerWidth - width - 8)),
    top: Math.max(8, Math.min(y, window.innerHeight - height - 8)),
  }
}

type SessionWorkspaceOverlaysProps = {
  state: SessionWorkspaceState | null
  workspaceMenuOpen: boolean
  workspaceMenuAnchor: WorkspaceMenuAnchor | null
  contextMenu: ContextMenuState
  onCloseWorkspaceMenu: () => void
  onCloseContextMenu: () => void
  onOpenInFinder: () => void
  onSwitchFolder: () => void
  onResetToTopic: () => Promise<void>
  onSwitchRecent: (path: string) => Promise<void>
  onContextAction: (action: string, entry: SessionWorkspaceEntry) => void | Promise<void>
}

export function SessionWorkspaceOverlays({
  state,
  workspaceMenuOpen,
  workspaceMenuAnchor,
  contextMenu,
  onCloseWorkspaceMenu,
  onCloseContextMenu,
  onOpenInFinder,
  onSwitchFolder,
  onResetToTopic,
  onSwitchRecent,
  onContextAction,
}: SessionWorkspaceOverlaysProps) {
  return (
    <>
      {workspaceMenuOpen && workspaceMenuAnchor
        ? createPortal(
            <>
              <button
                type="button"
                className="session-workspace-overlay-backdrop"
                aria-label="关闭菜单"
                onMouseDown={(event) => event.stopPropagation()}
                onClick={(event) => {
                  event.stopPropagation()
                  onCloseWorkspaceMenu()
                }}
              />
              <div
                className="session-workspace-menu"
                data-placement={workspaceMenuAnchor.placement}
                style={{
                  top: workspaceMenuAnchor.top,
                  left: workspaceMenuAnchor.left,
                  minWidth: Math.max(240, workspaceMenuAnchor.width),
                  maxHeight: workspaceMenuAnchor.maxHeight,
                }}
                onMouseDown={(event) => event.stopPropagation()}
                onClick={(event) => event.stopPropagation()}
              >
              <div className="session-workspace-menu-section">
                <button
                  type="button"
                  onClick={() => {
                    onCloseWorkspaceMenu()
                    onOpenInFinder()
                  }}
                >
                  在 Finder 中打开
                </button>
                <button
                  type="button"
                  onClick={() => {
                    onCloseWorkspaceMenu()
                    onSwitchFolder()
                  }}
                >
                  切换到其他文件夹…
                </button>
                <button
                  type="button"
                  disabled={state?.currentIsTopic}
                  onClick={() => {
                    onCloseWorkspaceMenu()
                    void onResetToTopic()
                  }}
                >
                  回到话题工作区
                </button>
              </div>
              {state?.recents.length ? (
                <div className="session-workspace-menu-section">
                  <p className="session-workspace-menu-label">最近目录</p>
                  {state.recents.map((recent) => (
                    <button
                      type="button"
                      key={recent.path}
                      title={recent.path}
                      onClick={() => {
                        onCloseWorkspaceMenu()
                        void onSwitchRecent(recent.path)
                      }}
                    >
                      {shortenWorkspacePath(recent.path)}
                    </button>
                  ))}
                </div>
              ) : null}
              </div>
            </>,
            document.body,
          )
        : null}
      {contextMenu
        ? createPortal(
            (() => {
              const pos = clampFloatingPosition(contextMenu.x, contextMenu.y)
              return (
                <>
                  <button
                    type="button"
                    className="session-workspace-overlay-backdrop"
                    aria-label="关闭菜单"
                    onMouseDown={(event) => event.stopPropagation()}
                    onClick={(event) => {
                      event.stopPropagation()
                      onCloseContextMenu()
                    }}
                  />
                  <div
                    className="session-workspace-context-menu"
                    style={{ left: pos.left, top: pos.top }}
                    onMouseDown={(event) => event.stopPropagation()}
                    onClick={(event) => event.stopPropagation()}
                  >
                  <button type="button" onClick={() => void onContextAction('add', contextMenu.entry)}>
                    添加到会话窗口
                  </button>
                  <button type="button" onClick={() => void onContextAction('reveal', contextMenu.entry)}>
                    在 Finder 中显示
                  </button>
                  <button type="button" onClick={() => void onContextAction('new-file', contextMenu.entry)}>
                    新建文件
                  </button>
                  <button type="button" onClick={() => void onContextAction('new-folder', contextMenu.entry)}>
                    新建文件夹
                  </button>
                  <button type="button" onClick={() => void onContextAction('copy-rel', contextMenu.entry)}>
                    复制路径
                  </button>
                  <button type="button" onClick={() => void onContextAction('copy-abs', contextMenu.entry)}>
                    复制绝对路径
                  </button>
                  <button type="button" onClick={() => void onContextAction('rename', contextMenu.entry)}>
                    重命名
                  </button>
                  <button
                    type="button"
                    className="danger"
                    onClick={() => void onContextAction('delete', contextMenu.entry)}
                  >
                    删除
                  </button>
                  </div>
                </>
              )
            })(),
            document.body,
          )
        : null}
    </>
  )
}
