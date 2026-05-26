import { useEffect, useRef, useState, type ChangeEvent, type RefObject } from 'react'
import type { WorkspaceRecord, WorkspaceResourceRecord } from '../../../types'
import { AppIcon } from '../../../components/AppIcon'
import { ImagePreviewModal } from '../../chat/TurnAndTools'
import {
  loadLocalMediaPreview,
  openLocalFile,
  workspaceDefaultSupervisorOrchestrationPrompt,
  workspaceReadResourceText,
  workspaceResourceAbsolutePath,
  workspaceUpdate,
} from '../../../lib/piClient'

function formatKb(size: number): string {
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  return `${(size / 1024 / 1024).toFixed(1)} MB`
}

function isHtmlResource(mime: string, fileName: string): boolean {
  if (mime.toLowerCase().trim() === 'text/html') return true
  const ext = fileName.split('.').pop()?.toLowerCase()
  return ext === 'html' || ext === 'htm'
}

/** 应用内用 UTF-8 文本读取；勿把 Office OpenXML（含 *ml* 片段）误判为 XML 文本。 */
function isTextLikeResource(mime: string, fileName: string): boolean {
  const m = mime.toLowerCase().trim()
  if (
    m.includes('openxmlformats') ||
    m.includes('officedocument') ||
    m.includes('spreadsheetml') ||
    m.includes('wordprocessingml') ||
    m.includes('presentationml') ||
    m === 'application/vnd.ms-excel' ||
    m === 'application/msword' ||
    m === 'application/vnd.ms-powerpoint'
  ) {
    return false
  }
  if (
    m.startsWith('text/') ||
    m === 'application/json' ||
    m === 'application/xml' ||
    m === 'text/xml' ||
    m.endsWith('+xml') ||
    m.includes('javascript')
  ) {
    return true
  }
  const ext = fileName.split('.').pop()?.toLowerCase()
  return ['md', 'markdown', 'txt', 'json', 'csv', 'log', 'env', 'yml', 'yaml', 'toml', 'ini', 'sh', 'ts', 'tsx', 'js', 'jsx', 'css', 'html', 'htm'].includes(
    ext || '',
  )
}

function isImagePreviewResource(mime: string, fileName: string): boolean {
  const m = mime.toLowerCase()
  if (m.startsWith('image/')) return true
  const ext = fileName.split('.').pop()?.toLowerCase()
  return ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'ico', 'svg'].includes(ext || '')
}

export type TeamResourcesPanelProps = {
  workspace: WorkspaceRecord
  /** 除主智能体外、可委派的成员数量（用于拉取与运行时一致的默认提示词） */
  delegateableMemberCount: number
  onWorkspaceUpdated?: (record: WorkspaceRecord) => void
  resources: WorkspaceResourceRecord[]
  onPickFile: (ref: RefObject<HTMLInputElement | null>) => void
  onUpload: (event: ChangeEvent<HTMLInputElement>) => void
  onDeleteResource: (resourceId: string) => void | Promise<void>
}

export function TeamResourcesPanel({
  workspace,
  delegateableMemberCount,
  onWorkspaceUpdated,
  resources,
  onPickFile,
  onUpload,
  onDeleteResource,
}: TeamResourcesPanelProps) {
  const workspaceId = workspace.id
  const uploadRef = useRef<HTMLInputElement | null>(null)
  const [promptDraft, setPromptDraft] = useState('')
  const [defaultPromptMd, setDefaultPromptMd] = useState('')
  const [promptExpandOpen, setPromptExpandOpen] = useState(false)
  const [promptSaving, setPromptSaving] = useState(false)
  const [promptError, setPromptError] = useState('')
  const [deleteFlow, setDeleteFlow] = useState<{
    record: WorkspaceResourceRecord
    step: 1 | 2
  } | null>(null)
  const [deleteSubmitting, setDeleteSubmitting] = useState(false)
  const [previewBusy, setPreviewBusy] = useState(false)
  const [previewError, setPreviewError] = useState('')
  const [textPreview, setTextPreview] = useState<{ fileName: string; content: string } | null>(null)
  const [imagePreview, setImagePreview] = useState<{ fileName: string; src: string } | null>(null)
  const [fallbackPreview, setFallbackPreview] = useState<{
    fileName: string
    mime: string
    localPath: string
    openError?: string
  } | null>(null)

  useEffect(() => {
    setPromptDraft(workspace.supervisorOrchestrationPrompt ?? '')
  }, [workspace.id, workspace.updatedAt, workspace.supervisorOrchestrationPrompt])

  useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const d = await workspaceDefaultSupervisorOrchestrationPrompt(workspace.id)
        if (!cancelled) setDefaultPromptMd(d)
      } catch {
        if (!cancelled) setDefaultPromptMd('')
      }
    })()
    return () => {
      cancelled = true
    }
  }, [workspace.id, delegateableMemberCount])

  const saveSupervisorPrompt = async (nextText: string): Promise<boolean> => {
    setPromptSaving(true)
    setPromptError('')
    try {
      const rec = await workspaceUpdate(workspace.id, { supervisorOrchestrationPrompt: nextText.trim() })
      setPromptDraft(rec.supervisorOrchestrationPrompt ?? '')
      onWorkspaceUpdated?.(rec)
      return true
    } catch (e) {
      setPromptError(String(e))
      return false
    } finally {
      setPromptSaving(false)
    }
  }

  const closeDeleteDialog = () => {
    if (deleteSubmitting) return
    setDeleteFlow(null)
  }

  const runDelete = async () => {
    if (!deleteFlow) return
    const { id } = deleteFlow.record
    setDeleteSubmitting(true)
    try {
      await Promise.resolve(onDeleteResource(id))
      setDeleteFlow(null)
    } catch {
      /* 父级 setError 并 rethrow */
    } finally {
      setDeleteSubmitting(false)
    }
  }

  const openPreview = async (r: WorkspaceResourceRecord) => {
    setPreviewError('')
    setPreviewBusy(true)
    try {
      if (isImagePreviewResource(r.mime, r.fileName)) {
        const path = await workspaceResourceAbsolutePath(workspaceId, r.relPath)
        try {
          const src = await loadLocalMediaPreview(path, r.mime)
          setImagePreview({ fileName: r.fileName, src })
        } catch {
          await openLocalFile(path)
        }
        return
      }
      if (isHtmlResource(r.mime, r.fileName)) {
        const path = await workspaceResourceAbsolutePath(workspaceId, r.relPath)
        await openLocalFile(path)
        return
      }
      if (isTextLikeResource(r.mime, r.fileName)) {
        try {
          const content = await workspaceReadResourceText(workspaceId, r.relPath)
          setTextPreview({ fileName: r.fileName, content })
        } catch {
          const path = await workspaceResourceAbsolutePath(workspaceId, r.relPath)
          await openLocalFile(path)
        }
        return
      }
      const path = await workspaceResourceAbsolutePath(workspaceId, r.relPath)
      try {
        await openLocalFile(path)
      } catch (openErr) {
        setFallbackPreview({
          fileName: r.fileName,
          mime: r.mime,
          localPath: path,
          openError: String(openErr),
        })
      }
    } catch (e) {
      setPreviewError(String(e))
    } finally {
      setPreviewBusy(false)
    }
  }

  return (
    <div className="task-center-panel workspace-panel">
      <div className="team-supervisor-prompt-block">
        <div className="team-supervisor-prompt-label">主智能体协调提示词</div>
        <p className="workspace-hint" style={{ margin: 0 }}>
          写入团队会话中「主智能体角色」段落的 Markdown。
          {delegateableMemberCount === 0
            ? ' 当前无可委派成员，加入成员后才会注入该段。'
            : ' 留空则使用应用默认（会随成员数更新）；保存非空内容后将固定使用你的文案。'}
        </p>
        {promptError ? (
          <div className="skills-feedback error agent-feedback inline team-drawer-error">
            <span>{promptError}</span>
          </div>
        ) : null}
        <textarea
          className="team-supervisor-prompt-textarea"
          value={promptDraft}
          onChange={(e) => setPromptDraft(e.target.value)}
          placeholder={
            defaultPromptMd
              ? '留空 = 使用下方「载入当前默认」同款策略（未写入本条时不展示全文）'
              : '暂无可注入默认（需团队内除主智能体外另有成员）'
          }
          spellCheck={false}
          aria-label="主智能体协调提示词"
        />
        <div className="team-supervisor-prompt-actions">
          <button
            type="button"
            className="outline-button"
            disabled={promptSaving}
            onClick={() => void saveSupervisorPrompt(promptDraft)}
          >
            {promptSaving ? '保存中…' : '保存'}
          </button>
          <button
            type="button"
            className="outline-button"
            disabled={promptSaving || !defaultPromptMd}
            onClick={() => setPromptDraft(defaultPromptMd)}
            title="将当前应用默认填入编辑区（需再点保存才会写入团队）"
          >
            载入当前默认
          </button>
          <button
            type="button"
            className="outline-button"
            disabled={promptSaving || !(workspace.supervisorOrchestrationPrompt ?? '').trim()}
            onClick={() => void saveSupervisorPrompt('')}
            title="清除自定义，恢复为应用默认"
          >
            恢复应用默认
          </button>
          <button
            type="button"
            className="workspace-inline-link"
            onClick={() => setPromptExpandOpen(true)}
          >
            <AppIcon name="panel" size={14} />
            <span>放大编辑</span>
          </button>
        </div>
      </div>

      <div className="workspace-panel-head">
        <strong className="workspace-panel-title">项目资料</strong>
        <button
          type="button"
          className="workspace-inline-link"
          onClick={() => {
            if (!uploadRef.current) {
              onPickFile(uploadRef)
              return
            }
            uploadRef.current.click()
          }}
        >
          <AppIcon name="upload" size={14} />
          <span>上传</span>
        </button>
        <input ref={uploadRef} type="file" hidden onChange={onUpload} />
      </div>
      {previewError ? (
        <div className="skills-feedback error agent-feedback inline team-drawer-error">
          <span>{previewError}</span>
        </div>
      ) : null}
      {resources.length === 0 ? (
        <p className="workspace-hint">
          暂无资料。上传的文件会进入 <code>teams/&lt;id&gt;/docs</code>，供成员引用。
        </p>
      ) : (
        <ul className="workspace-resource-list">
          {resources.map((r) => (
            <li key={r.id} className="workspace-resource-item">
              <button
                type="button"
                className="workspace-resource-main"
                disabled={previewBusy || deleteSubmitting}
                onClick={() => void openPreview(r)}
              >
                <AppIcon name="attachment" size={14} />
                <span className="workspace-resource-name">{r.fileName}</span>
                <span className="workspace-resource-size">{formatKb(r.size)}</span>
              </button>
              <button
                type="button"
                className="icon-button subtle workspace-resource-delete"
                aria-label="删除此资料"
                disabled={previewBusy || deleteSubmitting}
                onClick={(event) => {
                  event.stopPropagation()
                  setDeleteFlow({ record: r, step: 1 })
                }}
              >
                <AppIcon name="trash" size={16} />
              </button>
            </li>
          ))}
        </ul>
      )}

      {deleteFlow ? (
        <div className="confirm-dialog-overlay" role="presentation" onClick={closeDeleteDialog}>
          <div
            className="confirm-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby={
              deleteFlow.step === 1 ? 'workspace-resource-delete-s1' : 'workspace-resource-delete-s2'
            }
            onClick={(event) => event.stopPropagation()}
          >
            {deleteFlow.step === 1 ? (
              <>
                <h3 id="workspace-resource-delete-s1">删除资料</h3>
                <p>
                  确定要删除「{deleteFlow.record.fileName.trim() || '此文件'}」吗？
                </p>
                <div className="confirm-dialog-actions">
                  <button type="button" className="outline-button" onClick={closeDeleteDialog} disabled={deleteSubmitting}>
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
                <h3 id="workspace-resource-delete-s2">再次确认</h3>
                <p>
                  删除后无法恢复，数据库记录与 <code>teams/&lt;id&gt;/docs</code> 下的文件都会移除。确定继续吗？
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

      {textPreview ? (
        <div
          className="workspace-resource-preview-backdrop"
          role="presentation"
          onClick={() => setTextPreview(null)}
        >
          <div
            className="workspace-resource-preview-sheet"
            role="dialog"
            aria-modal="true"
            aria-label={textPreview.fileName}
            onClick={(event) => event.stopPropagation()}
          >
            <div className="workspace-resource-preview-head">
              <span className="workspace-resource-preview-title">{textPreview.fileName}</span>
              <button
                type="button"
                className="icon-button subtle workspace-resource-preview-close"
                aria-label="关闭预览"
                onClick={() => setTextPreview(null)}
              >
                <AppIcon name="close" size={16} />
              </button>
            </div>
            <pre className="workspace-resource-preview-body">{textPreview.content}</pre>
          </div>
        </div>
      ) : null}

      {fallbackPreview ? (
        <div
          className="workspace-resource-preview-backdrop"
          role="presentation"
          onClick={() => setFallbackPreview(null)}
        >
          <div
            className="workspace-resource-preview-sheet"
            role="dialog"
            aria-modal="true"
            aria-label={fallbackPreview.fileName}
            onClick={(event) => event.stopPropagation()}
          >
            <div className="workspace-resource-preview-head">
              <span className="workspace-resource-preview-title">{fallbackPreview.fileName}</span>
              <button
                type="button"
                className="icon-button subtle workspace-resource-preview-close"
                aria-label="关闭"
                onClick={() => setFallbackPreview(null)}
              >
                <AppIcon name="close" size={16} />
              </button>
            </div>
            <p className="workspace-resource-preview-fallback-msg">
              {fallbackPreview.openError
                ? `未能自动打开：${fallbackPreview.openError}`
                : `此类型（${fallbackPreview.mime || '未知'}）不在应用内预览，将使用系统默认程序打开。`}
            </p>
            <div className="workspace-resource-preview-actions">
              <button
                type="button"
                className="primary-cta"
                onClick={() =>
                  void openLocalFile(fallbackPreview.localPath).catch((e) =>
                    setFallbackPreview((prev) =>
                      prev ? { ...prev, openError: String(e) } : prev,
                    ),
                  )
                }
              >
                在系统中打开
              </button>
              <button type="button" className="outline-button" onClick={() => setFallbackPreview(null)}>
                关闭
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {imagePreview ? (
        <ImagePreviewModal
          alt={imagePreview.fileName}
          src={imagePreview.src}
          onClose={() => setImagePreview(null)}
        />
      ) : null}

      {promptExpandOpen ? (
        <div
          className="workspace-resource-preview-backdrop team-supervisor-prompt-modal"
          role="presentation"
          onClick={() => setPromptExpandOpen(false)}
        >
          <div
            className="workspace-resource-preview-sheet team-supervisor-prompt-sheet"
            role="dialog"
            aria-modal="true"
            aria-labelledby="team-supervisor-prompt-modal-title"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="workspace-resource-preview-head">
              <span className="workspace-resource-preview-title" id="team-supervisor-prompt-modal-title">
                主智能体协调提示词
              </span>
              <button
                type="button"
                className="icon-button subtle workspace-resource-preview-close"
                aria-label="关闭"
                onClick={() => setPromptExpandOpen(false)}
              >
                <AppIcon name="close" size={16} />
              </button>
            </div>
            <textarea
              className="team-supervisor-prompt-textarea"
              value={promptDraft}
              onChange={(e) => setPromptDraft(e.target.value)}
              spellCheck={false}
            />
            <div className="team-supervisor-prompt-modal-actions">
              <button
                type="button"
                className="outline-button"
                disabled={promptSaving}
                onClick={() => setPromptExpandOpen(false)}
              >
                取消
              </button>
              <button
                type="button"
                className="primary-cta"
                disabled={promptSaving}
                onClick={() => {
                  void saveSupervisorPrompt(promptDraft).then((ok) => {
                    if (ok) setPromptExpandOpen(false)
                  })
                }}
              >
                {promptSaving ? '保存中…' : '保存并关闭'}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  )
}

export default TeamResourcesPanel
