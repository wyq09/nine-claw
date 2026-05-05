import { useCallback, useEffect, useMemo, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import {
  appLogExportAll,
  appLogList,
  appLogOpenDir,
  appLogRead,
  type AppLogsOverview,
} from '../../lib/appLogClient'
import { JsonTreeNode, type JsonValue } from './LlmLogPreview'

const LINE_RE = /^\[([^\]]+)\] \[([^\]]+)\] \[([^\]]+)\] (.*)$/

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
  const [listBusy, setListBusy] = useState(false)
  const [readBusy, setReadBusy] = useState(false)
  const [error, setError] = useState('')
  const [levelFilter, setLevelFilter] = useState<string>('all')
  const [search, setSearch] = useState('')
  const [exportNotice, setExportNotice] = useState('')

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
    if (!selectedFile) {
      return
    }
    let cancelled = false
    setReadBusy(true)
    setError('')
    void (async () => {
      try {
        const text = await appLogRead(selectedFile)
        if (!cancelled) {
          setContent(text)
        }
      } catch (err) {
        if (!cancelled) {
          setContent('')
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
    const q = search.trim().toLowerCase()
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
  }, [lines, levelFilter, search])

  const totalKb = overview ? formatKb(overview.totalBytes) : 0
  const fileCount = overview?.files.length ?? 0
  const subtitle =
    overview == null
      ? '加载中…'
      : `所有运行日志按日期存储在本地文件，共 ${fileCount} 个文件，总计 ${totalKb} KB`

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

      <div className="app-log-panel-toolbar">
        <label className="app-log-select-wrap">
          <span className="visually-hidden">日志文件</span>
          <select
            className="app-log-select app-log-select-file"
            value={selectedFile}
            onChange={(e) => setSelectedFile(e.target.value)}
            disabled={!overview?.files.length || readBusy}
          >
            {overview?.files.length ? (
              overview.files.map((f) => (
                <option key={f.name} value={f.name}>
                  {f.name} ({formatKb(f.sizeBytes)} KB)
                </option>
              ))
            ) : (
              <option value="">暂无日志文件</option>
            )}
          </select>
        </label>
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
        <div className="app-log-search">
          <span className="app-log-search-icon" aria-hidden>
            ⌕
          </span>
          <input
            type="search"
            className="app-log-search-input"
            placeholder="搜索日志内容…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
      </div>

      <div className="app-log-viewport">
        <div className="app-log-viewport-meta">
          显示 {filteredLines.length} / {lines.filter((l) => l.trim()).length} 行
          {readBusy ? ' · 读取中…' : null}
        </div>
        <div className="app-log-lines" role="log">
          {filteredLines.length === 0 ? (
            <div className="app-log-empty">
              {readBusy ? '加载中…' : '没有匹配的日志行。尝试调整筛选条件或刷新。'}
            </div>
          ) : (
            filteredLines.map((line, index) => {
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
                      <JsonTreeNode value={jsonVal} depth={0} />
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
  )
}
