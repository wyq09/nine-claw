import { useEffect, useMemo, useState } from 'react'
import { createPortal } from 'react-dom'
import { AppIcon } from '../../components/AppIcon'
import type { AgentWorkspaceBundle, AgentWorkspaceFile } from '../../types'
import {
  readAgentWorkspaceBundle,
  readAgentWorkspaceFile,
  writeAgentWorkspaceFile,
} from '../../lib/piClient'

const FILE_SPECS = [
  {
    fileName: 'ROLE.md',
    label: 'ROLE.md',
    description: '定义这个智能体的职责边界、语气和默认工作方式。',
  },
  {
    fileName: 'TOOLS.md',
    label: 'TOOLS.md',
    description: '记录它偏好的工具、避坑规则和升级处理边界。',
  },
  {
    fileName: 'DECISIONS.md',
    label: 'DECISIONS.md',
    description: '沉淀需要长期遵守的拍板结果和稳定默认策略。',
  },
] as const

type ManagedFileName = (typeof FILE_SPECS)[number]['fileName']
type FileDraftMap = Record<ManagedFileName, string>

const EMPTY_DRAFTS: FileDraftMap = {
  'ROLE.md': '',
  'TOOLS.md': '',
  'DECISIONS.md': '',
}

type AgentSystemFilesEditorProps = {
  agentId: string
}

function buildInitialFileMap(bundle: AgentWorkspaceBundle | null): Record<ManagedFileName, AgentWorkspaceFile | null> {
  return {
    'ROLE.md': bundle?.files.find((file) => file.name === 'ROLE.md') ?? null,
    'TOOLS.md': bundle?.files.find((file) => file.name === 'TOOLS.md') ?? null,
    'DECISIONS.md': bundle?.files.find((file) => file.name === 'DECISIONS.md') ?? null,
  }
}

export function AgentSystemFilesEditor({ agentId }: AgentSystemFilesEditorProps) {
  const [bundle, setBundle] = useState<AgentWorkspaceBundle | null>(null)
  const [drafts, setDrafts] = useState<FileDraftMap>(EMPTY_DRAFTS)
  const [loading, setLoading] = useState(false)
  const [savingFile, setSavingFile] = useState<ManagedFileName | ''>('')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [expandedFile, setExpandedFile] = useState<ManagedFileName | null>(null)

  const fileMap = useMemo(() => buildInitialFileMap(bundle), [bundle])

  useEffect(() => {
    if (!agentId.trim()) {
      setBundle(null)
      setDrafts(EMPTY_DRAFTS)
      setLoading(false)
      setSavingFile('')
      setError('')
      setNotice('')
      setExpandedFile(null)
      return
    }

    let cancelled = false
    const loadBundle = async () => {
      setLoading(true)
      setError('')
      setNotice('')
      try {
        const nextBundle = await readAgentWorkspaceBundle(agentId)
        const nextFileMap = buildInitialFileMap(nextBundle)
        const lazyFiles = Object.values(nextFileMap).filter((file): file is AgentWorkspaceFile => Boolean(file?.lazyFetch && file.exists))
        let mergedBundle = nextBundle
        if (lazyFiles.length > 0) {
          const loadedFiles = await Promise.all(
            lazyFiles.map((file) =>
              readAgentWorkspaceFile({
                agentId,
                relativePath: file.relativePath,
              }),
            ),
          )
          const loadedByPath = new Map(loadedFiles.map((file) => [file.relativePath, file]))
          mergedBundle = {
            ...nextBundle,
            files: nextBundle.files.map((file) => loadedByPath.get(file.relativePath) ?? file),
          }
        }
        if (cancelled) {
          return
        }
        setBundle(mergedBundle)
        setDrafts({
          'ROLE.md': mergedBundle.files.find((file) => file.name === 'ROLE.md')?.content ?? '',
          'TOOLS.md': mergedBundle.files.find((file) => file.name === 'TOOLS.md')?.content ?? '',
          'DECISIONS.md': mergedBundle.files.find((file) => file.name === 'DECISIONS.md')?.content ?? '',
        })
      } catch (loadError) {
        if (!cancelled) {
          setBundle(null)
          setDrafts(EMPTY_DRAFTS)
          setError(loadError instanceof Error ? loadError.message : String(loadError))
        }
      } finally {
        if (!cancelled) {
          setLoading(false)
        }
      }
    }

    void loadBundle()
    return () => {
      cancelled = true
    }
  }, [agentId])

  const updateDraft = (fileName: ManagedFileName, value: string) => {
    setDrafts((current) => ({ ...current, [fileName]: value }))
    if (error) {
      setError('')
    }
    if (notice) {
      setNotice('')
    }
  }

  const saveFile = async (fileName: ManagedFileName) => {
    const target = fileMap[fileName]
    if (!agentId.trim() || !target || target.readOnly || savingFile) {
      return
    }

    setSavingFile(fileName)
    setError('')
    setNotice('')
    try {
      const nextBundle = await writeAgentWorkspaceFile({
        agentId,
        relativePath: target.relativePath,
        content: drafts[fileName],
      })
      const mergedBundle = {
        ...nextBundle,
        files: nextBundle.files.map((file) =>
          file.name === fileName ? { ...file, content: drafts[fileName], lazyFetch: false, exists: true } : file,
        ),
      }
      setBundle(mergedBundle)
      setNotice(`${fileName} 已保存`)
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError))
    } finally {
      setSavingFile('')
    }
  }

  return (
    <>
      <div className="agent-section">
        <div className="agent-section-header">
          <div>
            <strong>系统文件配置</strong>
            <p>直接编辑智能体工作区里的 `ROLE.md`、`TOOLS.md`、`DECISIONS.md`，保存后运行时会按这些文件生效。</p>
          </div>
          {agentId.trim() ? (
            <button type="button" className="outline-button" disabled={loading || Boolean(savingFile)} onClick={() => setBundle(null)}>
              <AppIcon name="refresh" size={16} />
              <span>{loading ? '刷新中…' : '刷新'}</span>
            </button>
          ) : null}
        </div>

        {!agentId.trim() ? (
          <div className="agent-workspace-hint">
            <strong>先创建智能体，再配置这三个工作区文件。</strong>
            <span>新建状态下还没有对应的 agent home，保存创建后这里会自动开放编辑。</span>
          </div>
        ) : null}

        {agentId.trim() && error ? (
          <div className="skills-feedback error agent-feedback inline">
            <strong>读取失败</strong>
            <span>{error}</span>
          </div>
        ) : null}

        {agentId.trim() && notice ? (
          <div className="skills-feedback success agent-feedback inline">
            <strong>已更新</strong>
            <span>{notice}</span>
          </div>
        ) : null}

        {agentId.trim() ? (
          <div className="agent-system-files-grid">
            {FILE_SPECS.map((spec) => {
              const file = fileMap[spec.fileName]
              const draft = drafts[spec.fileName]
              const saving = savingFile === spec.fileName
              const pristine = (file?.content ?? '') === draft
              return (
                <article key={spec.fileName} className="agent-system-file-card">
                  <div className="agent-system-file-head">
                    <div>
                      <strong>{spec.label}</strong>
                      <p>{spec.description}</p>
                    </div>
                    <span className={`agent-workspace-status ${file?.exists ? 'ok' : 'missing'}`}>
                      {loading
                        ? '加载中…'
                        : file?.readOnly
                          ? '只读'
                          : file?.exists
                            ? '可编辑'
                            : '保存后创建'}
                    </span>
                  </div>

                  <label className="input-field agent-field-full">
                    <span>{spec.label}</span>
                    <textarea
                      className="agent-system-file-textarea"
                      value={draft}
                      onChange={(event) => updateDraft(spec.fileName, event.target.value)}
                      rows={8}
                      spellCheck={false}
                      disabled={loading || file?.readOnly}
                      placeholder={loading ? '正在读取文件内容…' : `这里会写入 ${spec.label}`}
                    />
                  </label>

                  <div className="agent-system-file-actions">
                    <span className="agent-system-file-meta">
                      {file?.relativePath ?? `agents/${agentId}/${spec.fileName}`}
                    </span>
                    <div className="confirm-dialog-actions agent-system-file-buttons">
                      <button
                        type="button"
                        className="outline-button"
                        disabled={loading || file?.readOnly}
                        onClick={() => setExpandedFile(spec.fileName)}
                      >
                        <AppIcon name="panel" size={16} />
                        <span>放大编辑</span>
                      </button>
                      <button
                        type="button"
                        className="outline-button"
                        disabled={loading || file?.readOnly || pristine || Boolean(savingFile)}
                        onClick={() => void saveFile(spec.fileName)}
                      >
                        {saving ? '保存中…' : '保存'}
                      </button>
                    </div>
                  </div>
                </article>
              )
            })}
          </div>
        ) : null}
      </div>

      {expandedFile
        ? createPortal(
            <div className="confirm-dialog-overlay agent-prompt-editor-overlay" role="presentation" onClick={() => setExpandedFile(null)}>
              <div
                className="confirm-dialog agent-prompt-editor-dialog"
                role="dialog"
                aria-modal="true"
                aria-labelledby="agent-system-file-editor-title"
                onClick={(event) => event.stopPropagation()}
              >
                <div className="agent-prompt-editor-head">
                  <div>
                    <strong id="agent-system-file-editor-title">{expandedFile} 放大编辑器</strong>
                    <p>这里的修改会直接写回当前智能体工作区里的 {expandedFile}。</p>
                  </div>
                  <button
                    type="button"
                    className="icon-button subtle"
                    onClick={() => setExpandedFile(null)}
                    aria-label={`关闭 ${expandedFile} 放大编辑器`}
                  >
                    <AppIcon name="close" size={18} />
                  </button>
                </div>
                <label className="input-field agent-field-full agent-prompt-editor-field">
                  <span>{expandedFile}</span>
                  <textarea
                    className="agent-prompt-editor-textarea"
                    autoFocus
                    value={drafts[expandedFile]}
                    onChange={(event) => updateDraft(expandedFile, event.target.value)}
                    spellCheck={false}
                    rows={20}
                  />
                </label>
                <div className="confirm-dialog-actions">
                  <button type="button" className="outline-button" onClick={() => setExpandedFile(null)}>
                    完成
                  </button>
                  <button
                    type="button"
                    className="outline-button primary"
                    disabled={Boolean(savingFile)}
                    onClick={() => void saveFile(expandedFile)}
                  >
                    {savingFile === expandedFile ? '保存中…' : '保存'}
                  </button>
                </div>
              </div>
            </div>,
            document.body,
          )
        : null}
    </>
  )
}
