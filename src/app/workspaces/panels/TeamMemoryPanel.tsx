import { useState } from 'react'
import type { WorkspaceMemoryRecord } from '../../../types'
import { AppIcon } from '../../../components/AppIcon'

export type TeamMemoryPanelProps = {
  memories: WorkspaceMemoryRecord[]
  newMemoTitle: string
  newMemoContent: string
  onNewMemoTitleChange: (value: string) => void
  onNewMemoContentChange: (value: string) => void
  onWriteMemo: () => void
  onDeleteMemory: (memoryId: string) => void | Promise<void>
  loading: boolean
}

export function TeamMemoryPanel({
  memories,
  newMemoTitle,
  newMemoContent,
  onNewMemoTitleChange,
  onNewMemoContentChange,
  onWriteMemo,
  onDeleteMemory,
  loading,
}: TeamMemoryPanelProps) {
  const [deleteFlow, setDeleteFlow] = useState<{
    record: WorkspaceMemoryRecord
    step: 1 | 2
  } | null>(null)
  const [deleteSubmitting, setDeleteSubmitting] = useState(false)

  const closeDeleteDialog = () => {
    if (deleteSubmitting) return
    setDeleteFlow(null)
  }

  const runDelete = async () => {
    if (!deleteFlow) return
    const { id } = deleteFlow.record
    setDeleteSubmitting(true)
    try {
      await Promise.resolve(onDeleteMemory(id))
      setDeleteFlow(null)
    } catch {
      /* 错误由 TeamDrawer 展示 */
    } finally {
      setDeleteSubmitting(false)
    }
  }

  return (
    <div className="task-center-panel workspace-panel">
      <div className="workspace-panel-head">
        <strong className="workspace-panel-title">共享记忆</strong>
        <span className="workspace-panel-meta">{memories.length} 条</span>
      </div>
      <div className="workspace-memo-compose">
        <input
          className="workspaces-input"
          placeholder="记忆标题"
          value={newMemoTitle}
          onChange={(event) => onNewMemoTitleChange(event.target.value)}
        />
        <textarea
          className="workspaces-input workspaces-textarea"
          rows={3}
          placeholder="可选内容（支持 Markdown，也会落盘到 teams/<id>/memory/entries）"
          value={newMemoContent}
          onChange={(event) => onNewMemoContentChange(event.target.value)}
        />
        <div className="workspace-memo-actions">
          <button
            type="button"
            className="primary-cta"
            onClick={onWriteMemo}
            disabled={loading || !newMemoTitle.trim()}
          >
            写入记忆
          </button>
        </div>
      </div>
      <ul className="workspace-memory-list">
        {memories.length === 0 ? (
          <li className="workspace-memory-empty">还没有共享记忆。</li>
        ) : null}
        {memories.map((m) => (
          <li key={m.id} className="workspace-memory-item">
            <div className="workspace-memory-head">
              <div className="workspace-memory-head-text">
                <strong className="workspace-memory-title">{m.title}</strong>
                <span className="workspace-memory-time">
                  {new Date(m.updatedAt).toLocaleString()}
                </span>
              </div>
              <button
                type="button"
                className="icon-button subtle workspace-memory-delete"
                aria-label="删除这条共享记忆"
                disabled={loading || deleteSubmitting}
                onClick={() => setDeleteFlow({ record: m, step: 1 })}
              >
                <AppIcon name="trash" size={16} />
              </button>
            </div>
            {m.content ? <pre className="workspace-memory-content">{m.content}</pre> : null}
          </li>
        ))}
      </ul>

      {deleteFlow ? (
        <div className="confirm-dialog-overlay" role="presentation" onClick={closeDeleteDialog}>
          <div
            className="confirm-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby={
              deleteFlow.step === 1 ? 'workspace-memory-delete-s1' : 'workspace-memory-delete-s2'
            }
            onClick={(event) => event.stopPropagation()}
          >
            {deleteFlow.step === 1 ? (
              <>
                <h3 id="workspace-memory-delete-s1">删除共享记忆</h3>
                <p>
                  确定要删除「{deleteFlow.record.title.trim() || '此条目'}」吗？
                </p>
                <div className="confirm-dialog-actions">
                  <button
                    type="button"
                    className="outline-button"
                    onClick={closeDeleteDialog}
                    disabled={deleteSubmitting}
                  >
                    取消
                  </button>
                  <button
                    type="button"
                    className="outline-button"
                    onClick={() => setDeleteFlow({ ...deleteFlow, step: 2 })}
                    disabled={deleteSubmitting}
                  >
                    继续
                  </button>
                </div>
              </>
            ) : (
              <>
                <h3 id="workspace-memory-delete-s2">再次确认</h3>
                <p>
                  删除后无法恢复，数据库记录与 teams 目录下的对应 Markdown 文件都会移除。确定继续吗？
                </p>
                <div className="confirm-dialog-actions">
                  <button
                    type="button"
                    className="outline-button"
                    onClick={() => setDeleteFlow({ ...deleteFlow, step: 1 })}
                    disabled={deleteSubmitting}
                  >
                    返回
                  </button>
                  <button
                    type="button"
                    className="outline-button confirm-dialog-delete"
                    disabled={deleteSubmitting}
                    onClick={() => void runDelete()}
                  >
                    {deleteSubmitting ? '删除中…' : '确认删除'}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      ) : null}
    </div>
  )
}

export default TeamMemoryPanel
