import type { WorkspaceMemoryRecord } from '../../../types'

export type TeamMemoryPanelProps = {
  memories: WorkspaceMemoryRecord[]
  newMemoTitle: string
  newMemoContent: string
  onNewMemoTitleChange: (value: string) => void
  onNewMemoContentChange: (value: string) => void
  onWriteMemo: () => void
  loading: boolean
}

export function TeamMemoryPanel({
  memories,
  newMemoTitle,
  newMemoContent,
  onNewMemoTitleChange,
  onNewMemoContentChange,
  onWriteMemo,
  loading,
}: TeamMemoryPanelProps) {
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
              <strong className="workspace-memory-title">{m.title}</strong>
              <span className="workspace-memory-time">
                {new Date(m.updatedAt).toLocaleString()}
              </span>
            </div>
            {m.content ? <pre className="workspace-memory-content">{m.content}</pre> : null}
          </li>
        ))}
      </ul>
    </div>
  )
}

export default TeamMemoryPanel
