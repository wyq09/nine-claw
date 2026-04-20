import { useEffect, useState, type ReactNode } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import type { ArtifactsTreeEntry, WorkspaceRecord } from '../../../types'
import { AppIcon } from '../../../components/AppIcon'
import { ImagePreviewModal } from '../../chat/TurnAndTools'
import {
  loadLocalMediaPreview,
  openLocalFile,
  workspaceArtifactAbsolutePath,
  workspaceListArtifactsEntries,
  workspaceReadArtifactText,
  workspaceResolveArtifactsRoot,
  workspaceUpdate,
} from '../../../lib/piClient'

function guessMimeFromFileName(fileName: string): string {
  const ext = fileName.split('.').pop()?.toLowerCase() ?? ''
  const map: Record<string, string> = {
    png: 'image/png',
    jpg: 'image/jpeg',
    jpeg: 'image/jpeg',
    gif: 'image/gif',
    webp: 'image/webp',
    svg: 'image/svg+xml',
    md: 'text/markdown',
    txt: 'text/plain',
    json: 'application/json',
    html: 'text/html',
    csv: 'text/csv',
    xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  }
  return map[ext] ?? 'application/octet-stream'
}

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

function formatKb(size: number): string {
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  return `${(size / 1024 / 1024).toFixed(1)} MB`
}

export type TeamArtifactsPanelProps = {
  workspace: WorkspaceRecord
  onWorkspaceUpdated?: (record: WorkspaceRecord) => void
  onError: (message: string) => void
}

export function TeamArtifactsPanel({ workspace, onWorkspaceUpdated, onError }: TeamArtifactsPanelProps) {
  const [pathDraft, setPathDraft] = useState(workspace.artifactsRoot ?? '')
  const [resolvedRoot, setResolvedRoot] = useState('')
  const [pathSaving, setPathSaving] = useState(false)
  const [treeByPath, setTreeByPath] = useState<Record<string, ArtifactsTreeEntry[]>>({})
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(['']))
  const [treeBusy, setTreeBusy] = useState(false)
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
    setPathDraft(workspace.artifactsRoot ?? '')
  }, [workspace.id, workspace.artifactsRoot])

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const p = await workspaceResolveArtifactsRoot(workspace.id)
        if (!cancelled) setResolvedRoot(p)
      } catch {
        if (!cancelled) setResolvedRoot('')
      }
    })()
    return () => {
      cancelled = true
    }
  }, [workspace.id, workspace.artifactsRoot, workspace.updatedAt])

  useEffect(() => {
    let cancelled = false
    setTreeByPath({})
    setExpanded(new Set(['']))
    setTreeBusy(true)
    workspaceListArtifactsEntries(workspace.id, null)
      .then((list) => {
        if (!cancelled) setTreeByPath({ '': list })
      })
      .catch((e) => {
        if (!cancelled) onError(String(e))
      })
      .finally(() => {
        if (!cancelled) setTreeBusy(false)
      })
    return () => {
      cancelled = true
    }
  }, [workspace.id, workspace.artifactsRoot, workspace.updatedAt, onError])

  const toggleDir = async (rel: string) => {
    if (expanded.has(rel)) {
      setExpanded((prev) => {
        const n = new Set(prev)
        n.delete(rel)
        return n
      })
      return
    }
    setTreeBusy(true)
    try {
      const list = await workspaceListArtifactsEntries(workspace.id, rel || null)
      setTreeByPath((p) => ({ ...p, [rel]: list }))
      setExpanded((prev) => new Set(prev).add(rel))
    } catch (e) {
      onError(String(e))
    } finally {
      setTreeBusy(false)
    }
  }

  const openArtifactFile = async (entry: ArtifactsTreeEntry) => {
    if (entry.isDir) return
    const mime = guessMimeFromFileName(entry.name)
    setPreviewError('')
    setPreviewBusy(true)
    try {
      if (isImagePreviewResource(mime, entry.name)) {
        const path = await workspaceArtifactAbsolutePath(workspace.id, entry.relPath)
        try {
          const src = await loadLocalMediaPreview(path, mime)
          setImagePreview({ fileName: entry.name, src })
        } catch {
          await openLocalFile(path)
        }
        return
      }
      if (isTextLikeResource(mime, entry.name)) {
        try {
          const content = await workspaceReadArtifactText(workspace.id, entry.relPath)
          setTextPreview({ fileName: entry.name, content })
        } catch {
          const path = await workspaceArtifactAbsolutePath(workspace.id, entry.relPath)
          await openLocalFile(path)
        }
        return
      }
      const path = await workspaceArtifactAbsolutePath(workspace.id, entry.relPath)
      try {
        await openLocalFile(path)
      } catch (openErr) {
        setFallbackPreview({
          fileName: entry.name,
          mime,
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

  const onSavePath = async () => {
    setPathSaving(true)
    setPreviewError('')
    try {
      const w = await workspaceUpdate(workspace.id, { artifactsRoot: pathDraft.trim() })
      onWorkspaceUpdated?.(w)
    } catch (e) {
      onError(String(e))
    } finally {
      setPathSaving(false)
    }
  }

  const pickArtifactsFolder = async () => {
    setPreviewError('')
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: '选择项目成果根目录',
      })
      if (typeof selected === 'string' && selected.trim()) {
        setPathDraft(selected.trim())
      }
    } catch (e) {
      onError(String(e))
    }
  }

  const renderTreeLevel = (parentRel: string, depth: number): ReactNode => {
    const entries = treeByPath[parentRel]
    if (!entries?.length && parentRel === '' && treeBusy) {
      return <p className="workspace-hint team-artifacts-tree-loading">加载中…</p>
    }
    if (!entries?.length) {
      return parentRel === '' ? <p className="workspace-hint">目录为空。后续任务产物将写入此目录。</p> : null
    }
    return (
      <ul className={`team-artifacts-tree-list depth-${depth}`}>
        {entries.map((e) => (
          <li key={e.relPath} className="team-artifacts-tree-item">
            {e.isDir ? (
              <>
                <button
                  type="button"
                  className="team-artifacts-tree-row folder"
                  disabled={treeBusy}
                  onClick={() => void toggleDir(e.relPath)}
                >
                  <span className="team-artifacts-tree-chevron" aria-hidden>
                    {expanded.has(e.relPath) ? (
                      <AppIcon name="chevron-down" size={14} />
                    ) : (
                      <span className="team-artifacts-chevron-right">›</span>
                    )}
                  </span>
                  <AppIcon name="folder" size={14} />
                  <span className="team-artifacts-tree-label">{e.name}</span>
                </button>
                {expanded.has(e.relPath) ? (
                  <div className="team-artifacts-tree-nested">{renderTreeLevel(e.relPath, depth + 1)}</div>
                ) : null}
              </>
            ) : (
              <button
                type="button"
                className="team-artifacts-tree-row file"
                disabled={previewBusy}
                onClick={() => void openArtifactFile(e)}
              >
                <span className="team-artifacts-tree-chevron" aria-hidden />
                <AppIcon name="attachment" size={14} />
                <span className="team-artifacts-tree-label">{e.name}</span>
                {e.size != null ? <span className="team-artifacts-tree-size">{formatKb(e.size)}</span> : null}
              </button>
            )}
          </li>
        ))}
      </ul>
    )
  }

  return (
    <div className="task-center-panel workspace-panel team-artifacts-panel">
      <div className="workspace-panel-head">
        <strong className="workspace-panel-title">项目成果</strong>
      </div>
      <div className="team-artifacts-path-block">
        <label className="team-artifacts-path-label">成果根目录（可选）</label>
        <div className="team-artifacts-path-row">
          <input
            className="workspaces-input"
            placeholder="留空 = 使用应用目录下 teams/&lt;团队ID&gt;/artifacts"
            value={pathDraft}
            onChange={(ev) => setPathDraft(ev.target.value)}
          />
          <button type="button" className="outline-button team-artifacts-browse-path" onClick={() => void pickArtifactsFolder()}>
            浏览…
          </button>
          <button type="button" className="primary-cta team-artifacts-save-path" disabled={pathSaving} onClick={() => void onSavePath()}>
            {pathSaving ? '保存中…' : '保存'}
          </button>
        </div>
        <p className="team-artifacts-resolved-path">
          当前解析路径：<code>{resolvedRoot || '…'}</code>
        </p>
      </div>
      {previewError ? (
        <div className="skills-feedback error agent-feedback inline team-drawer-error">
          <span>{previewError}</span>
        </div>
      ) : null}
      <div className="team-artifacts-split">
        <div className="team-artifacts-tree-pane">{renderTreeLevel('', 0)}</div>
        <div className="team-artifacts-preview-pane">
          <p className="team-artifacts-preview-hint">点击文件预览；Office / 二进制等将使用系统应用打开。</p>
        </div>
      </div>

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
            onClick={(ev) => ev.stopPropagation()}
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
            onClick={(ev) => ev.stopPropagation()}
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
                : `此类型（${fallbackPreview.mime || '未知'}）将使用系统程序打开。`}
            </p>
            <div className="workspace-resource-preview-actions">
              <button
                type="button"
                className="primary-cta"
                onClick={() =>
                  void openLocalFile(fallbackPreview.localPath).catch((err) =>
                    setFallbackPreview((prev) => (prev ? { ...prev, openError: String(err) } : prev)),
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
    </div>
  )
}

export default TeamArtifactsPanel
