import { useCallback, useDeferredValue, useEffect, useMemo, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import {
  appLogExportAll,
  appLogList,
  appLogOpenDir,
  appLogRead,
  type AppLogsOverview,
} from '../../lib/appLogClient'
import { AppIcon } from '../AppIcon'
import { JsonTreeNode, type JsonValue } from './LlmLogPreview'

const LINE_RE = /^\[([^\]]+)\] \[([^\]]+)\] \[([^\]]+)\] (.*)$/
const INITIAL_VISIBLE_LINES = 400
const VISIBLE_LINE_STEP = 400

type ParsedLine =
  | {
      kind: 'structured'
      raw: string
      ts: string
      level: string
      target: string
      message: string
    }
  | { kind: 'raw'; raw: string }

function parseLogLine(line: string): ParsedLine {
  const m = line.match(LINE_RE)
  if (m) {
    return {
      kind: 'structured',
      raw: line,
      ts: m[1],
      level: m[2].trim().toUpperCase(),
      target: m[3],
      message: m[4],
    }
  }
  return { kind: 'raw', raw: line }
}

function tryParseMessageJson(message: string): JsonValue | null {
  const t = message.trim()
  if (!t.startsWith('{') && !t.startsWith('[')) {
    return null
  }
  try {
    return JSON.parse(t) as JsonValue
  } catch {
    return null
  }
}

function normalizeLevel(level: string) {
  return level.trim().toUpperCase().replace(/\s+/g, '')
}

function lineMatchesLevel(parsed: ParsedLine, filter: string): boolean {
  if (filter === 'all') {
    return true
  }
  if (parsed.kind !== 'structured') {
    return true
  }
  const l = normalizeLevel(parsed.level)
  if (filter === 'warn' && l.startsWith('WARN')) {
    return true
  }
  if (filter === 'error' && l.startsWith('ERROR')) {
    return true
  }
  if (filter === 'info' && l.startsWith('INFO')) {
    return true
  }
  if (filter === 'debug' && l.startsWith('DEBUG')) {
    return true
  }
  if (filter === 'trace' && l.startsWith('TRACE')) {
    return true
  }
  return false
}

function formatKb(bytes: number) {
  return Math.max(1, Math.round(bytes / 1024))
}

export function ApplicationLogsPanel() {
  const [overview, setOverview] = useState<AppLogsOverview | null>(null)
  const [selectedFile, setSelectedFile] = useState('')
  const [content, setContent] = useState('')
  const [contentTruncated, setContentTruncated] = useState(false)
  const [listBusy, setListBusy] = useState(false)
  const [readBusy, setReadBusy] = useState(false)
  const [error, setError] = useState('')
  const [levelFilter, setLevelFilter] = useState<string>('all')
  const [search, setSearch] = useState('')
  const [exportNotice, setExportNotice] = useState('')
  const [visibleLineLimit, setVisibleLineLimit] = useState(INITIAL_VISIBLE_LINES)
  const deferredSearch = useDeferredValue(search)

  const refreshList = useCallback(async () => {
    setListBusy(true)
    setExportNotice('')
    setError('')
    try {
      const o = await appLogList()
      setOverview(o)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setOverview(null)
    } finally {
      setListBusy(false)
    }
  }, [])

  useEffect(() => {
    void refreshList()
  }, [refreshList])

  useEffect(() => {
    if (!overview?.files.length) {
      setSelectedFile('')
      setContent('')
      setContentTruncated(false)
      return
    }
    const next =
      selectedFile && overview.files.some((f) => f.name === selectedFile)
        ? selectedFile
        : overview.files[0].name
    if (next !== selectedFile) {
      setSelectedFile(next)
    }
  }, [overview, selectedFile])

  useEffect(() => {
    setVisibleLineLimit(INITIAL_VISIBLE_LINES)
  }, [selectedFile, levelFilter, deferredSearch])

  useEffect(() => {
    if (!selectedFile) {
      return
    }
    let cancelled = false
    setReadBusy(true)
    setError('')
    void (async () => {
      try {
        const result = await appLogRead(selectedFile)
        if (!cancelled) {
          setContent(result.content)
          setContentTruncated(result.truncated)
        }
      } catch (err) {
        if (!cancelled) {
          setContent('')
          setContentTruncated(false)
          setError(err instanceof Error ? err.message : String(err))
        }
      } finally {
        if (!cancelled) {
          setReadBusy(false)
        }
      }
    })()
    return () => {
      cancelled = true
    }
  }, [selectedFile])

  const lines = useMemo(() => content.replace(/\r\n/g, '\n').split('\n'), [content])

  const filteredLines = useMemo(() => {
    const q = deferredSearch.trim().toLowerCase()
    return lines
      .map((line) => line.trimEnd())
      .filter((line) => {
        if (!line) {
          return false
        }
        const parsed = parseLogLine(line)
        if (!lineMatchesLevel(parsed, levelFilter)) {
          return false
        }
        if (!q) {
          return true
        }
        return line.toLowerCase().includes(q)
      })
  }, [lines, levelFilter, deferredSearch])

  const visibleLines = useMemo(
    () => filteredLines.slice(Math.max(0, filteredLines.length - visibleLineLimit)),
    [filteredLines, visibleLineLimit],
  )
  const hiddenLineCount = Math.max(0, filteredLines.length - visibleLines.length)

  const totalKb = overview ? formatKb(overview.totalBytes) : 0
  const fileCount = overview?.files.length ?? 0
  const selectedFileMeta = overview?.files.find((file) => file.name === selectedFile) ?? null
  const subtitle =
    overview == null
      ? '加载中…'
      : `${fileCount} 个文件 · ${totalKb} KB · ${overview.dir || '本地日志目录'}`

  return (
    <div className="app-log-panel">
      <div className="app-log-panel-header">
        <div>
          <h3 className="app-log-panel-title">应用日志</h3>
          <p className="app-log-panel-subtitle settings-note">{subtitle}</p>
        </div>
        <div className="app-log-panel-actions">
          <button
            type="button"
            className="outline-button"
            disabled={listBusy}
            onClick={() => void refreshList()}
          >
            <AppIcon name="refresh" size={16} />
            {listBusy ? '刷新中…' : '刷新'}
          </button>
          <button
            type="button"
            className="outline-button"
            onClick={() =>
              void appLogOpenDir().catch((err) =>
                setError(err instanceof Error ? err.message : String(err)),
              )
            }
          >
            <AppIcon name="folder" size={16} />
            打开目录
          </button>
          <button
            type="button"
            className="outline-button primary"
            disabled={!overview?.files.length}
            onClick={() =>
              void (async () => {
                const dest = await open({ directory: true, multiple: false })
                if (typeof dest !== 'string' || !dest) {
                  return
                }
                try {
                  const n = await appLogExportAll(dest)
                  setError('')
                  setExportNotice(`已导出 ${n} 个日志文件到所选目录。`)
                } catch (err) {
                  setError(err instanceof Error ? err.message : String(err))
                }
              })()
            }
          >
            <AppIcon name="download" size={16} />
            导出全部
          </button>
        </div>
      </div>

      {error ? (
        <p className="settings-note error" role="alert">
          {error}
        </p>
      ) : null}
      {exportNotice ? <p className="settings-note">{exportNotice}</p> : null}

      <div className="app-log-workspace">
        <aside className="app-log-sidebar" aria-label="日志筛选">
          <div className="app-log-side-section">
            <div className="app-log-side-title">
              <strong>日志文件</strong>
              <span>{overview?.dir || '正在读取目录'}</span>
            </div>
            <div className="app-log-file-list" role="listbox" aria-label="日志文件">
              {overview?.files.length ? (
                overview.files.map((file) => (
                  <button
                    key={file.name}
                    type="button"
                    className={`app-log-file-item ${selectedFile === file.name ? 'active' : ''}`}
                    onClick={() => setSelectedFile(file.name)}
                    role="option"
                    aria-selected={selectedFile === file.name}
                  >
                    <span>{file.name}</span>
                    <small>{formatKb(file.sizeBytes)} KB</small>
                  </button>
                ))
              ) : (
                <div className="app-log-sidebar-empty">暂无日志文件</div>
              )}
            </div>
          </div>

          <div className="app-log-side-section">
            <div className="app-log-side-title">
              <strong>筛选</strong>
              <span>当前文件内过滤，不修改日志文件。</span>
            </div>
            <label className="input-field">
              <span>日志级别</span>
              <select
                className="app-log-select app-log-select-level"
                value={levelFilter}
                onChange={(e) => setLevelFilter(e.target.value)}
                aria-label="按级别筛选"
              >
                <option value="all">全部级别</option>
                <option value="error">ERROR</option>
                <option value="warn">WARN</option>
                <option value="info">INFO</option>
                <option value="debug">DEBUG</option>
                <option value="trace">TRACE</option>
              </select>
            </label>
            <label className="input-field">
              <span>搜索内容</span>
              <div className="app-log-search">
                <AppIcon name="search" size={16} />
                <input
                  type="search"
                  className="app-log-search-input"
                  placeholder="关键词、target、JSON 字段…"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                />
              </div>
            </label>
          </div>
        </aside>

        <div className="app-log-viewport">
          <div className="app-log-viewport-meta">
            <span>
              {selectedFileMeta ? `${selectedFileMeta.name} · ${formatKb(selectedFileMeta.sizeBytes)} KB` : '未选择日志文件'}
            </span>
            <span>
              显示 {visibleLines.length} / {filteredLines.length} 行
              {readBusy ? ' · 读取中…' : deferredSearch !== search ? ' · 筛选中…' : null}
            </span>
          </div>
          {contentTruncated ? (
            <p className="app-log-truncated-note">
              大文件默认只加载末尾约 512 KB，便于快速浏览。完整内容请用「打开目录」或「导出全部」。
            </p>
          ) : null}
          <div className="app-log-lines" role="log">
            {hiddenLineCount > 0 ? (
              <button
                type="button"
                className="outline-button app-log-load-more"
                onClick={() => setVisibleLineLimit((limit) => limit + VISIBLE_LINE_STEP)}
              >
                显示更早的 {Math.min(hiddenLineCount, VISIBLE_LINE_STEP)} 行（还有 {hiddenLineCount} 行）
              </button>
            ) : null}
            {visibleLines.length === 0 ? (
              <div className="app-log-empty">
                {readBusy ? '加载中…' : '没有匹配的日志行。尝试调整筛选条件或刷新。'}
              </div>
            ) : (
              visibleLines.map((line, index) => {
                const parsed = parseLogLine(line)
                if (parsed.kind === 'raw') {
                  return (
                    <div key={`${index}-raw`} className="app-log-line app-log-line-raw">
                      <span className="app-log-line-text">{parsed.raw}</span>
                    </div>
                  )
                }
                const jsonVal = tryParseMessageJson(parsed.message)
                const tone = parsed.level.startsWith('ERROR')
                  ? 'error'
                  : parsed.level.startsWith('WARN')
                    ? 'warn'
                    : 'info'
                return (
                  <div key={`${index}-s`} className={`app-log-line app-log-line-structured tone-${tone}`}>
                    <div className="app-log-line-head">
                      <span className="app-log-line-ts">{parsed.ts}</span>
                      <span className={`app-log-line-lvl lvl-${tone}`}>{parsed.level}</span>
                      <span className="app-log-line-target">{parsed.target}</span>
                    </div>
                    {jsonVal != null ? (
                      <div className="app-log-line-json">
                        <JsonTreeNode value={jsonVal} depth={0} defaultExpanded={false} />
                      </div>
                    ) : (
                      <div className="app-log-line-msg">{parsed.message}</div>
                    )}
                  </div>
                )
              })
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
