import { useEffect, useState } from 'react'
import { FolderGit2, GitBranch, Layers } from 'lucide-react'
import type { SessionForkWorkspaceMode } from '../../lib/chatForkClient'

export type CreateBranchDialogProps = {
  /** 目标分支点那一轮的提问预览（截断展示，帮助用户确认分支位置）。 */
  forkPointPrompt: string
  busy: boolean
  error?: string
  onCancel: () => void
  onConfirm: (mode: SessionForkWorkspaceMode) => void
}

const MODE_OPTIONS: Array<{
  value: SessionForkWorkspaceMode
  title: string
  description: string
  icon: typeof GitBranch
  recommended?: boolean
}> = [
  {
    value: 'share',
    title: '共享工作空间',
    description: '分支与原会话共用同一工作目录，文件改动彼此可见，适合继续深入同一个任务。',
    icon: Layers,
    recommended: true,
  },
  {
    value: 'copy',
    title: '独立拷贝',
    description: '将原会话当前工作目录完整复制到新分支，后续文件修改完全隔离，互不影响。',
    icon: FolderGit2,
  },
]

/** 「创建对话分支」确认弹窗：选择共享工作空间或独立拷贝后派生新会话。 */
export function CreateBranchDialog({
  forkPointPrompt,
  busy,
  error,
  onCancel,
  onConfirm,
}: CreateBranchDialogProps) {
  const [mode, setMode] = useState<SessionForkWorkspaceMode>('share')

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !busy) {
        onCancel()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [busy, onCancel])

  const preview = forkPointPrompt.trim().slice(0, 80)

  return (
    <div
      className="branch-dialog-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="创建对话分支"
      onClick={() => {
        if (!busy) {
          onCancel()
        }
      }}
    >
      <div className="branch-dialog" onClick={(event) => event.stopPropagation()}>
        <div className="branch-dialog-header">
          <strong>创建对话分支</strong>
        </div>
        <div className="branch-dialog-body">
          <p className="branch-dialog-desc">
            分支将包含截止到该消息（含）的完整对话历史。
          </p>
          {preview ? <p className="branch-dialog-fork-point">「{preview}」</p> : null}
          <div className="branch-dialog-modes" role="radiogroup" aria-label="工作区模式">
            {MODE_OPTIONS.map((option) => {
              const Icon = option.icon
              const selected = mode === option.value
              return (
                <button
                  key={option.value}
                  type="button"
                  role="radio"
                  aria-checked={selected}
                  className={`branch-dialog-mode${selected ? ' selected' : ''}`}
                  onClick={() => setMode(option.value)}
                  disabled={busy}
                >
                  <span className="branch-dialog-mode-icon">
                    <Icon size={18} strokeWidth={1.75} aria-hidden />
                  </span>
                  <span className="branch-dialog-mode-text">
                    <span className="branch-dialog-mode-title">
                      {option.title}
                      {option.recommended ? (
                        <span className="branch-dialog-mode-badge">推荐</span>
                      ) : null}
                    </span>
                    <span className="branch-dialog-mode-desc">{option.description}</span>
                  </span>
                </button>
              )
            })}
          </div>
          {error ? <div className="branch-dialog-error">{error}</div> : null}
        </div>
        <div className="branch-dialog-actions">
          <button type="button" className="outline-button" onClick={onCancel} disabled={busy}>
            取消
          </button>
          <button
            type="button"
            className="outline-button primary"
            onClick={() => onConfirm(mode)}
            disabled={busy}
          >
            {busy ? '创建中…' : mode === 'copy' ? '拷贝并创建' : '创建分支'}
          </button>
        </div>
      </div>
    </div>
  )
}
