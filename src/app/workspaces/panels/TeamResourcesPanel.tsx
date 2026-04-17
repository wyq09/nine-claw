import { useRef, type ChangeEvent, type RefObject } from 'react'
import type { WorkspaceResourceRecord } from '../../../types'
import { AppIcon } from '../../../components/AppIcon'

function formatKb(size: number): string {
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  return `${(size / 1024 / 1024).toFixed(1)} MB`
}

export type TeamResourcesPanelProps = {
  resources: WorkspaceResourceRecord[]
  onPickFile: (ref: RefObject<HTMLInputElement | null>) => void
  onUpload: (event: ChangeEvent<HTMLInputElement>) => void
}

export function TeamResourcesPanel({ resources, onPickFile, onUpload }: TeamResourcesPanelProps) {
  const uploadRef = useRef<HTMLInputElement | null>(null)
  return (
    <div className="task-center-panel workspace-panel">
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
      {resources.length === 0 ? (
        <p className="workspace-hint">
          暂无资料。上传的文件会进入 <code>teams/&lt;id&gt;/docs</code>，供成员引用。
        </p>
      ) : (
        <ul className="workspace-resource-list">
          {resources.map((r) => (
            <li key={r.id} className="workspace-resource-item">
              <AppIcon name="attachment" size={14} />
              <span className="workspace-resource-name">{r.fileName}</span>
              <span className="workspace-resource-size">{formatKb(r.size)}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

export default TeamResourcesPanel
