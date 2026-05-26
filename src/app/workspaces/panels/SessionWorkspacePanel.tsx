import { open } from '@tauri-apps/plugin-dialog'
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from 'react'
import { AppIcon } from '../../../components/AppIcon'
import {
  SessionWorkspaceOverlays,
  computeWorkspaceMenuAnchor,
  type ContextMenuState,
  type WorkspaceMenuAnchor,
} from './SessionWorkspaceOverlays'
import { loadLocalMediaPreview } from '../../../lib/piClient'
import {
  sessionWorkspaceAbsolutePath,
  sessionWorkspaceCreateDir,
  sessionWorkspaceCreateFile,
  sessionWorkspaceDelete,
  sessionWorkspaceGet,
  sessionWorkspaceImportFiles,
  sessionWorkspaceListEntries,
  sessionWorkspaceOpenPath,
  sessionWorkspaceReadFile,
  sessionWorkspaceRename,
  sessionWorkspaceResetToTopic,
  sessionWorkspaceRevealPath,
  sessionWorkspaceSwitch,
  type SessionWorkspaceEntry,
  type SessionWorkspacePreviewKind,
  type SessionWorkspaceReadResult,
  type SessionWorkspaceState,
} from '../../../lib/sessionWorkspaceClient'
import {
  formatSessionWorkspaceSize,
  inferSessionWorkspacePreviewKind,
  isTextualPreviewKind,
  shortenWorkspacePath,
  splitCodeLines,
} from './sessionWorkspacePreview'
import './SessionWorkspacePanel.css'

const MarkdownRenderer = lazy(() => import('../../../components/MarkdownRenderer'))

type WorkspaceTab = {
  id: string
  relPath: string
  name: string
  kind: SessionWorkspacePreviewKind
  content?: string | null
  imageSrc?: string
  entries?: SessionWorkspaceEntry[]
  info?: SessionWorkspaceReadResult['info']
  loading?: boolean
  error?: string
}

type InlineNamePrompt = {
  kind: 'file' | 'folder' | 'rename'
  parentRel: string
  entry?: SessionWorkspaceEntry
}

export type SessionWorkspacePanelProps = {
  sessionId: string | null
  modelLabel?: string | null
  onAddToComposer?: (text: string) => void
}

const WIDTH_KEY = 'nineclaw.sessionWorkspacePanel.width'
const COLLAPSED_KEY = 'nineclaw.sessionWorkspacePanel.collapsed'
const TREE_COLLAPSED_KEY = 'nineclaw.sessionWorkspacePanel.treeCollapsed'

function readStoredWidth(): number {
  const raw = window.localStorage.getItem(WIDTH_KEY)
  const n = raw ? Number(raw) : 420
  return Number.isFinite(n) ? Math.min(720, Math.max(320, n)) : 420
}

function parentRelPath(relPath: string): string {
  const index = relPath.lastIndexOf('/')
  return index >= 0 ? relPath.slice(0, index) : ''
}

function fileMimeFromName(fileName: string): string {
  const ext = fileName.split('.').pop()?.toLowerCase() ?? ''
  if (ext === 'svg') return 'image/svg+xml'
  if (ext === 'png') return 'image/png'
  if (ext === 'jpg' || ext === 'jpeg') return 'image/jpeg'
  if (ext === 'gif') return 'image/gif'
  if (ext === 'webp') return 'image/webp'
  return 'application/octet-stream'
}

function entryIcon(entry: SessionWorkspaceEntry) {
  if (entry.isDir) return 'folder'
  return 'attachment'
}

function tabIcon(kind: SessionWorkspacePreviewKind) {
  if (kind === 'folder') return 'folder'
  if (kind === 'markdown') return 'book'
  return 'attachment'
}

export function SessionWorkspacePanel({
  sessionId,
  modelLabel,
  onAddToComposer,
}: SessionWorkspacePanelProps) {
  const [collapsed, setCollapsed] = useState(
    () => window.localStorage.getItem(COLLAPSED_KEY) === '1',
  )
  const [treeCollapsed, setTreeCollapsed] = useState(
    () => window.localStorage.getItem(TREE_COLLAPSED_KEY) === '1',
  )
  const [width, setWidth] = useState(readStoredWidth)
  const [state, setState] = useState<SessionWorkspaceState | null>(null)
  const [treeByPath, setTreeByPath] = useState<Record<string, SessionWorkspaceEntry[]>>({})
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set(['']))
  const [query, setQuery] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [tabs, setTabs] = useState<WorkspaceTab[]>([])
  const [activeTabId, setActiveTabId] = useState('')
  const [workspaceMenuOpen, setWorkspaceMenuOpen] = useState(false)
  const [workspaceMenuAnchor, setWorkspaceMenuAnchor] = useState<WorkspaceMenuAnchor | null>(null)
  const [contextMenu, setContextMenu] = useState<ContextMenuState>(null)
  const [inlineNamePrompt, setInlineNamePrompt] = useState<InlineNamePrompt | null>(null)
  const [inlineNameValue, setInlineNameValue] = useState('')
  const resizeRef = useRef<{ startX: number; startWidth: number } | null>(null)
  const pathPillRef = useRef<HTMLButtonElement>(null)
  const inlineNameInputRef = useRef<HTMLInputElement>(null)

  const refreshRoot = useCallback(async () => {
    if (!sessionId) return
    setLoading(true)
    setError('')
    try {
      const nextState = await sessionWorkspaceGet(sessionId)
      const rootEntries = await sessionWorkspaceListEntries(sessionId, null)
      setState(nextState)
      setTreeByPath({ '': rootEntries })
      setExpanded(new Set(['']))
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [sessionId])

  useEffect(() => {
    setTabs([])
    setActiveTabId('')
    setTreeByPath({})
    setExpanded(new Set(['']))
    setState(null)
    setError('')
    void refreshRoot()
  }, [refreshRoot])

  useEffect(() => {
    window.localStorage.setItem(COLLAPSED_KEY, collapsed ? '1' : '0')
  }, [collapsed])

  useEffect(() => {
    window.localStorage.setItem(TREE_COLLAPSED_KEY, treeCollapsed ? '1' : '0')
  }, [treeCollapsed])

  useEffect(() => {
    window.localStorage.setItem(WIDTH_KEY, String(width))
  }, [width])

  useEffect(() => {
    const onPointerMove = (event: PointerEvent) => {
      const current = resizeRef.current
      if (!current) return
      const next = current.startWidth - (event.clientX - current.startX)
      setWidth(Math.min(720, Math.max(320, next)))
    }
    const onPointerUp = () => {
      resizeRef.current = null
    }
    window.addEventListener('pointermove', onPointerMove)
    window.addEventListener('pointerup', onPointerUp)
    window.addEventListener('pointercancel', onPointerUp)
    return () => {
      window.removeEventListener('pointermove', onPointerMove)
      window.removeEventListener('pointerup', onPointerUp)
      window.removeEventListener('pointercancel', onPointerUp)
    }
  }, [])

  useEffect(() => {
    const close = () => {
      setWorkspaceMenuOpen(false)
      setWorkspaceMenuAnchor(null)
      setContextMenu(null)
    }
    window.addEventListener('click', close)
    return () => window.removeEventListener('click', close)
  }, [])

  useEffect(() => {
    if (!inlineNamePrompt) return
    inlineNameInputRef.current?.focus()
  }, [inlineNamePrompt])

  const activeTab = tabs.find((tab) => tab.id === activeTabId) ?? tabs[0] ?? null

  const setOrAddTab = useCallback((tab: WorkspaceTab) => {
    setTabs((previous) => {
      const exists = previous.some((item) => item.id === tab.id)
      return exists ? previous.map((item) => (item.id === tab.id ? tab : item)) : [...previous, tab]
    })
    setActiveTabId(tab.id)
  }, [])

  const loadDir = useCallback(
    async (relPath: string) => {
      if (!sessionId) return []
      const entries = await sessionWorkspaceListEntries(sessionId, relPath || null)
      setTreeByPath((previous) => ({ ...previous, [relPath]: entries }))
      return entries
    },
    [sessionId],
  )

  const toggleDir = useCallback(
    async (relPath: string) => {
      if (expanded.has(relPath)) {
        setExpanded((previous) => {
          const next = new Set(previous)
          next.delete(relPath)
          return next
        })
        return
      }
      try {
        if (!treeByPath[relPath]) await loadDir(relPath)
        setExpanded((previous) => new Set(previous).add(relPath))
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    },
    [expanded, loadDir, treeByPath],
  )

  const openEntry = useCallback(
    async (entry: SessionWorkspaceEntry) => {
      if (!sessionId) return
      const kind = inferSessionWorkspacePreviewKind(entry.name, entry.isDir)
      const id = entry.relPath || entry.name
      setOrAddTab({ id, relPath: entry.relPath, name: entry.name, kind, loading: true })
      setError('')
      try {
        if (entry.isDir) {
          const entries = await loadDir(entry.relPath)
          setExpanded((previous) => new Set(previous).add(entry.relPath))
          setOrAddTab({ id, relPath: entry.relPath, name: entry.name, kind: 'folder', entries })
          return
        }
        const result = await sessionWorkspaceReadFile(sessionId, entry.relPath)
        const resultKind = result.previewKind ?? kind
        if (resultKind === 'image') {
          const absolute = await sessionWorkspaceAbsolutePath(sessionId, entry.relPath)
          const imageSrc = await loadLocalMediaPreview(absolute, fileMimeFromName(entry.name))
          setOrAddTab({
            id,
            relPath: entry.relPath,
            name: entry.name,
            kind: 'image',
            imageSrc,
            info: result.info,
          })
          return
        }
        setOrAddTab({
          id,
          relPath: entry.relPath,
          name: entry.name,
          kind: resultKind,
          content: result.content,
          info: result.info,
        })
      } catch (err) {
        setOrAddTab({
          id,
          relPath: entry.relPath,
          name: entry.name,
          kind,
          error: err instanceof Error ? err.message : String(err),
        })
      }
    },
    [loadDir, sessionId, setOrAddTab],
  )

  const closeTab = (id: string) => {
    setTabs((previous) => {
      const index = previous.findIndex((tab) => tab.id === id)
      const next = previous.filter((tab) => tab.id !== id)
      if (activeTabId === id) {
        setActiveTabId(next[Math.max(0, index - 1)]?.id ?? next[0]?.id ?? '')
      }
      return next
    })
  }

  const switchFolder = async () => {
    if (!sessionId) return
    const selected = await open({
      directory: true,
      multiple: false,
      title: '选择会话工作区',
    })
    if (typeof selected !== 'string' || !selected.trim()) return
    const nextState = await sessionWorkspaceSwitch(sessionId, selected)
    setState(nextState)
    await refreshRoot()
  }

  const openInlineNamePrompt = useCallback((prompt: InlineNamePrompt, initialName = '') => {
    setInlineNamePrompt(prompt)
    setInlineNameValue(initialName)
  }, [])

  const submitInlineName = useCallback(async () => {
    if (!sessionId || !inlineNamePrompt) return
    const name = inlineNameValue.trim()
    if (!name) return
    setError('')
    try {
      if (inlineNamePrompt.kind === 'file') {
        await sessionWorkspaceCreateFile({ sessionId, parentRel: inlineNamePrompt.parentRel, name })
        await (inlineNamePrompt.parentRel ? loadDir(inlineNamePrompt.parentRel) : refreshRoot())
      } else if (inlineNamePrompt.kind === 'folder') {
        await sessionWorkspaceCreateDir({ sessionId, parentRel: inlineNamePrompt.parentRel, name })
        await (inlineNamePrompt.parentRel ? loadDir(inlineNamePrompt.parentRel) : refreshRoot())
      } else if (inlineNamePrompt.kind === 'rename' && inlineNamePrompt.entry) {
        if (name === inlineNamePrompt.entry.name) {
          setInlineNamePrompt(null)
          setInlineNameValue('')
          return
        }
        const info = await sessionWorkspaceRename({
          sessionId,
          relPath: inlineNamePrompt.entry.relPath,
          newName: name,
        })
        await loadDir(parentRelPath(inlineNamePrompt.entry.relPath))
        setTabs((previous) =>
          previous.map((tab) =>
            tab.relPath === inlineNamePrompt.entry?.relPath
              ? { ...tab, id: info.relPath, relPath: info.relPath, name: info.name, info }
              : tab,
          ),
        )
        setActiveTabId((current) =>
          current === inlineNamePrompt.entry?.relPath ? info.relPath : current,
        )
      }
      setInlineNamePrompt(null)
      setInlineNameValue('')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [inlineNamePrompt, inlineNameValue, loadDir, refreshRoot, sessionId])

  const toggleWorkspaceMenu = useCallback(() => {
    if (workspaceMenuOpen) {
      setWorkspaceMenuOpen(false)
      setWorkspaceMenuAnchor(null)
      return
    }
    const rect = pathPillRef.current?.getBoundingClientRect()
    if (!rect) return
    setWorkspaceMenuAnchor(computeWorkspaceMenuAnchor(rect))
    setWorkspaceMenuOpen(true)
  }, [workspaceMenuOpen])

  const importFilesToWorkspace = useCallback(async () => {
    if (!sessionId) return
    const picked = await open({
      multiple: true,
      directory: false,
      title: '添加文件到工作目录',
    })
    if (picked == null) return
    const sourcePaths = (Array.isArray(picked) ? picked : [picked]).filter(
      (path): path is string => typeof path === 'string' && path.trim().length > 0,
    )
    if (!sourcePaths.length) return
    setError('')
    try {
      const imported = await sessionWorkspaceImportFiles({
        sessionId,
        sourcePaths,
        parentRel: '',
      })
      await refreshRoot()
      const first = imported[0]
      if (first && !first.isDir) {
        await openEntry({
          name: first.name,
          relPath: first.relPath,
          isDir: false,
          size: first.size,
          modifiedMs: first.modifiedMs,
        })
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [openEntry, refreshRoot, sessionId])

  const copyActiveTabContent = useCallback(() => {
    if (!activeTab?.content) return
    void navigator.clipboard.writeText(activeTab.content)
  }, [activeTab])

  const runContextAction = async (action: string, entry: SessionWorkspaceEntry) => {
    if (!sessionId) return
    setContextMenu(null)
    try {
      if (action === 'add') {
        onAddToComposer?.(`请参考会话工作区文件：${entry.relPath}`)
        return
      }
      if (action === 'reveal') {
        await sessionWorkspaceRevealPath(sessionId, entry.relPath)
        return
      }
      if (action === 'copy-rel') {
        await navigator.clipboard.writeText(entry.relPath)
        return
      }
      if (action === 'copy-abs') {
        const absolute = await sessionWorkspaceAbsolutePath(sessionId, entry.relPath, true)
        await navigator.clipboard.writeText(absolute)
        return
      }
      if (action === 'new-file' || action === 'new-folder') {
        const parentRel = entry.isDir ? entry.relPath : parentRelPath(entry.relPath)
        openInlineNamePrompt({
          kind: action === 'new-file' ? 'file' : 'folder',
          parentRel,
        })
        return
      }
      if (action === 'rename') {
        openInlineNamePrompt(
          { kind: 'rename', parentRel: parentRelPath(entry.relPath), entry },
          entry.name,
        )
        return
      }
      if (action === 'delete') {
        if (!window.confirm(`删除 ${entry.name}？`)) return
        await sessionWorkspaceDelete(sessionId, entry.relPath)
        await loadDir(parentRelPath(entry.relPath))
        setTabs((previous) => previous.filter((tab) => tab.relPath !== entry.relPath))
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const renderTreeLevel = (parentRel: string, depth: number) => {
    const entries = treeByPath[parentRel] ?? []
    const visible = query.trim()
      ? entries.filter((entry) => entry.name.toLowerCase().includes(query.trim().toLowerCase()))
      : entries
    if (parentRel === '' && loading) {
      return <p className="session-workspace-empty">加载中…</p>
    }
    if (!visible.length) {
      return parentRel === '' ? <p className="session-workspace-empty">目录为空</p> : null
    }
    return (
      <ul className={`session-workspace-tree depth-${depth}`}>
        {visible.map((entry) => (
          <li key={entry.relPath || entry.name}>
            <button
              type="button"
              className={`session-workspace-tree-row ${entry.isDir ? 'folder' : 'file'} ${
                activeTab?.relPath === entry.relPath ? 'active' : ''
              }`}
              onClick={() => {
                if (entry.isDir) void toggleDir(entry.relPath)
                void openEntry(entry)
              }}
              onContextMenu={(event) => {
                event.preventDefault()
                event.stopPropagation()
                setContextMenu({ x: event.clientX, y: event.clientY, entry })
              }}
            >
              <span className="session-workspace-chevron">
                {entry.isDir ? (
                  expanded.has(entry.relPath) ? (
                    <AppIcon name="chevron-down" size={12} />
                  ) : (
                    <span>›</span>
                  )
                ) : null}
              </span>
              <AppIcon name={entryIcon(entry)} size={14} />
              <span className="session-workspace-tree-label">{entry.name}</span>
              {entry.size != null ? (
                <span className="session-workspace-tree-size">
                  {formatSessionWorkspaceSize(entry.size)}
                </span>
              ) : null}
            </button>
            {entry.isDir && expanded.has(entry.relPath) ? (
              <div className="session-workspace-tree-nested">
                {renderTreeLevel(entry.relPath, depth + 1)}
              </div>
            ) : null}
          </li>
        ))}
      </ul>
    )
  }

  const preview = useMemo(() => {
    if (!activeTab) {
      return <div className="session-workspace-preview-empty">选择一个文件进行预览</div>
    }
    if (activeTab.loading) return <div className="session-workspace-preview-empty">读取中…</div>
    if (activeTab.error) return <div className="session-workspace-preview-error">{activeTab.error}</div>
    if (activeTab.kind === 'folder') {
      const entries = activeTab.entries ?? []
      return (
        <div className="session-workspace-folder-preview">
          {entries.map((entry) => (
            <button key={entry.relPath} type="button" onClick={() => void openEntry(entry)}>
              <AppIcon name={entryIcon(entry)} size={14} />
              <span>{entry.name}</span>
              <small>{formatSessionWorkspaceSize(entry.size)}</small>
            </button>
          ))}
        </div>
      )
    }
    if (activeTab.kind === 'image' && activeTab.imageSrc) {
      return <img className="session-workspace-image-preview" src={activeTab.imageSrc} alt={activeTab.name} />
    }
    if (activeTab.kind === 'markdown') {
      return (
        <div className="session-workspace-markdown theme-markdown-surface markdown-content">
          <Suspense fallback={<div className="session-workspace-preview-empty">渲染中…</div>}>
            <MarkdownRenderer content={activeTab.content ?? ''} isStreaming={false} />
          </Suspense>
        </div>
      )
    }
    if (activeTab.kind === 'html') {
      return (
        <iframe
          className="session-workspace-html-preview"
          sandbox="allow-forms allow-popups allow-scripts"
          srcDoc={activeTab.content ?? ''}
          title={activeTab.name}
        />
      )
    }
    if (activeTab.kind === 'code') {
      return (
        <pre className="session-workspace-code-preview">
          {splitCodeLines(activeTab.content ?? '').map((line) => (
            <span key={line.n} className="session-workspace-code-line">
              <span>{line.n}</span>
              <code>{line.text || ' '}</code>
            </span>
          ))}
        </pre>
      )
    }
    if (isTextualPreviewKind(activeTab.kind)) {
      return <pre className="session-workspace-text-preview">{activeTab.content}</pre>
    }
    return (
      <div className="session-workspace-binary-preview">
        <strong>{activeTab.name}</strong>
        <p>此文件类型暂不内嵌预览，可用系统默认应用打开。</p>
        <button
          type="button"
          className="outline-button"
          onClick={() => sessionId && void sessionWorkspaceOpenPath(sessionId, activeTab.relPath)}
        >
          打开
        </button>
      </div>
    )
  }, [activeTab, openEntry, sessionId])

  if (!sessionId) return null

  const rootLabel = state ? shortenWorkspacePath(state.currentWorkspaceDir) : '会话工作区'
  const canCopyActiveTab =
    !!activeTab &&
    !activeTab.loading &&
    !activeTab.error &&
    activeTab.kind !== 'folder' &&
    activeTab.kind !== 'image' &&
    activeTab.content != null

  return (
    <aside
      className={`session-workspace-panel ${collapsed ? 'collapsed' : ''}`}
      style={{ width: collapsed ? 44 : width }}
    >
      <div
        className="session-workspace-resizer"
        onPointerDown={(event: ReactPointerEvent<HTMLDivElement>) => {
          event.currentTarget.setPointerCapture(event.pointerId)
          resizeRef.current = { startX: event.clientX, startWidth: width }
        }}
        onPointerUp={(event: ReactPointerEvent<HTMLDivElement>) => {
          if (event.currentTarget.hasPointerCapture(event.pointerId)) {
            event.currentTarget.releasePointerCapture(event.pointerId)
          }
          resizeRef.current = null
        }}
      />
      {collapsed ? (
        <button
          type="button"
          className="session-workspace-collapse"
          onClick={() => setCollapsed(false)}
          title="展开会话工作区"
          aria-label="展开会话工作区"
        >
          <AppIcon name="panel" size={17} />
        </button>
      ) : (
        <>
          <header className="session-workspace-head">
            <div className="session-workspace-head-selectors">
              <span className="session-workspace-model-pill" title={modelLabel || undefined}>
                {modelLabel || '当前模型'}
              </span>
              <button
                ref={pathPillRef}
                type="button"
                className="session-workspace-path-pill"
                title={state?.currentWorkspaceDir}
                aria-label="选择工作区目录"
                aria-expanded={workspaceMenuOpen}
                onClick={(event) => {
                  event.stopPropagation()
                  toggleWorkspaceMenu()
                }}
              >
                <AppIcon name="folder" size={12} />
                <span>{rootLabel}</span>
                <AppIcon name="chevron-down" size={12} />
              </button>
            </div>
            <div className="session-workspace-head-actions">
              <button
                type="button"
                className="session-workspace-collapse"
                onClick={() => setCollapsed(true)}
                title="折叠会话工作区"
                aria-label="折叠会话工作区"
              >
                <AppIcon name="panel" size={17} />
              </button>
            </div>
          </header>
          {error ? <div className="session-workspace-error">{error}</div> : null}
          <div className={`session-workspace-body ${treeCollapsed ? 'tree-collapsed' : ''}`}>
            <div className="session-workspace-unified-bar">
              <div className="session-workspace-tree-title">
                <span>工作目录</span>
                <button
                  type="button"
                  className="session-workspace-tree-toggle"
                  onClick={() => setTreeCollapsed((value) => !value)}
                  title={treeCollapsed ? '展开工作目录' : '折叠工作目录'}
                  aria-label={treeCollapsed ? '展开工作目录' : '折叠工作目录'}
                >
                  <AppIcon name="panel" size={15} />
                </button>
              </div>
              {!treeCollapsed ? (
                <div className="session-workspace-tree-actions">
                  <button
                    type="button"
                    className="session-workspace-tree-action"
                    onClick={() => void refreshRoot()}
                    title="刷新"
                    aria-label="刷新工作目录"
                  >
                    <AppIcon name="refresh" size={14} />
                  </button>
                  <button
                    type="button"
                    className="session-workspace-tree-action"
                    onClick={(event) => {
                      event.stopPropagation()
                      void importFilesToWorkspace()
                    }}
                    title="添加文件"
                    aria-label="添加文件"
                  >
                    <AppIcon name="plus-circle" size={14} />
                  </button>
                </div>
              ) : null}
              <div className="session-workspace-tabs">
                {tabs.map((tab) => (
                  <button
                    key={tab.id}
                    type="button"
                    className={tab.id === activeTabId ? 'active' : ''}
                    onClick={() => setActiveTabId(tab.id)}
                    title={tab.name}
                  >
                    <AppIcon name={tabIcon(tab.kind)} size={12} />
                    <span className="session-workspace-tab-label">{tab.name}</span>
                    <span
                      role="button"
                      tabIndex={0}
                      aria-label={`关闭 ${tab.name}`}
                      className="session-workspace-tab-close"
                      onClick={(event) => {
                        event.stopPropagation()
                        closeTab(tab.id)
                      }}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter' || event.key === ' ') closeTab(tab.id)
                      }}
                    >
                      <AppIcon name="close" size={12} />
                    </span>
                  </button>
                ))}
              </div>
            </div>
            <div className="session-workspace-content">
              {!treeCollapsed ? (
                <section className="session-workspace-tree-pane">
                  {inlineNamePrompt ? (
                    <form
                      className="session-workspace-name-prompt"
                      onSubmit={(event) => {
                        event.preventDefault()
                        void submitInlineName()
                      }}
                      onClick={(event) => event.stopPropagation()}
                    >
                      <input
                        ref={inlineNameInputRef}
                        value={inlineNameValue}
                        onChange={(event) => setInlineNameValue(event.currentTarget.value)}
                        placeholder={
                          inlineNamePrompt.kind === 'folder'
                            ? '文件夹名'
                            : inlineNamePrompt.kind === 'rename'
                              ? '新名称'
                              : '文件名'
                        }
                        aria-label={
                          inlineNamePrompt.kind === 'folder'
                            ? '新建文件夹名'
                            : inlineNamePrompt.kind === 'rename'
                              ? '重命名'
                              : '新建文件名'
                        }
                      />
                      <div className="session-workspace-name-prompt-actions">
                        <button type="submit">确定</button>
                        <button
                          type="button"
                          onClick={() => {
                            setInlineNamePrompt(null)
                            setInlineNameValue('')
                          }}
                        >
                          取消
                        </button>
                      </div>
                    </form>
                  ) : null}
                  <div className="session-workspace-tree-search">
                    <AppIcon name="search" size={13} />
                    <input
                      value={query}
                      onChange={(event) => setQuery(event.currentTarget.value)}
                      placeholder="搜索文件名..."
                    />
                  </div>
                  <div className="session-workspace-tree-scroll">{renderTreeLevel('', 0)}</div>
                </section>
              ) : null}
              <section className="session-workspace-preview-pane">
                {canCopyActiveTab ? (
                  <div className="session-workspace-preview-toolbar">
                    <button type="button" onClick={copyActiveTabContent}>
                      复制全文
                    </button>
                  </div>
                ) : null}
                <div className="session-workspace-preview">{preview}</div>
              </section>
            </div>
          </div>
          <SessionWorkspaceOverlays
            state={state}
            workspaceMenuOpen={workspaceMenuOpen}
            workspaceMenuAnchor={workspaceMenuAnchor}
            contextMenu={contextMenu}
            onCloseWorkspaceMenu={() => {
              setWorkspaceMenuOpen(false)
              setWorkspaceMenuAnchor(null)
            }}
            onCloseContextMenu={() => setContextMenu(null)}
            onOpenInFinder={() => void sessionWorkspaceOpenPath(sessionId, '')}
            onSwitchFolder={() => void switchFolder()}
            onResetToTopic={async () => {
              const next = await sessionWorkspaceResetToTopic(sessionId)
              setState(next)
              await refreshRoot()
            }}
            onSwitchRecent={async (path) => {
              const next = await sessionWorkspaceSwitch(sessionId, path)
              setState(next)
              await refreshRoot()
            }}
            onContextAction={runContextAction}
          />
        </>
      )}
    </aside>
  )
}

export default SessionWorkspacePanel
