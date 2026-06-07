export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue }

type ParsedLogEntry =
  | {
      lineNumber: number
      raw: string
      tone: 'plain'
      type: 'text'
    }
  | {
      lineNumber: number
      raw: string
      tone: 'info' | 'warn' | 'error'
      type: 'json'
      value: JsonValue
      summary: string
    }

type LlmLogPreviewProps = {
  busy: boolean
  file: string | null
  tail: string
}

function isRecord(value: JsonValue): value is { [key: string]: JsonValue } {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function normalizePreviewLines(tail: string): string[] {
  return tail
    .replace(/\r\n/g, '\n')
    .split('\n')
    .map((line) => line.trimEnd())
    .filter((line) => line.trim().length > 0)
}

function asShortText(value: unknown, limit = 48): string | null {
  if (typeof value !== 'string') {
    return null
  }
  const trimmed = value.trim()
  if (!trimmed) {
    return null
  }
  return trimmed.length > limit ? `${trimmed.slice(0, limit)}...` : trimmed
}

function jsonTone(value: JsonValue): 'info' | 'warn' | 'error' {
  if (!isRecord(value)) {
    return 'info'
  }
  if (typeof value.error === 'string' && value.error.trim()) {
    return 'error'
  }
  const level = `${value.level ?? value.status ?? value.phase ?? value.kind ?? ''}`.toLowerCase()
  if (/(error|fatal|failed|failure)/.test(level)) {
    return 'error'
  }
  if (/(warn|warning)/.test(level)) {
    return 'warn'
  }
  return 'info'
}

function summarizeJsonValue(value: JsonValue): string {
  if (Array.isArray(value)) {
    return `JSON 数组 · ${value.length} 项`
  }
  if (!isRecord(value)) {
    return `JSON 值 · ${String(value)}`
  }

  const parts: string[] = []
  const time = asShortText(value.timestamp) || asShortText(value.startedAt) || asShortText(value.endedAt)
  const kind =
    asShortText(value.kind) ||
    asShortText(value.phase) ||
    asShortText(value.type) ||
    asShortText(value.event) ||
    'JSON 记录'
  const caller = asShortText(value.callerAgentName) || asShortText(value.callerAgentId)
  const target = asShortText(value.targetAgentName) || asShortText(value.targetAgentId)
  const model = asShortText(value.model)
  const error = asShortText(value.error, 80)

  if (time) {
    parts.push(time)
  }
  parts.push(kind)
  if (caller && target) {
    parts.push(`${caller} -> ${target}`)
  } else if (caller) {
    parts.push(caller)
  }
  if (model) {
    parts.push(model)
  }
  if (error) {
    parts.push(error)
  }

  return parts.join(' · ')
}

export function parseLlmLogPreviewTail(tail: string): ParsedLogEntry[] {
  return normalizePreviewLines(tail).map((line, index) => {
    try {
      const parsed = JSON.parse(line) as JsonValue
      return {
        lineNumber: index + 1,
        raw: line,
        tone: jsonTone(parsed),
        type: 'json' as const,
        value: parsed,
        summary: summarizeJsonValue(parsed),
      }
    } catch {
      return {
        lineNumber: index + 1,
        raw: line,
        tone: 'plain' as const,
        type: 'text' as const,
      }
    }
  })
}

function renderJsonPrimitive(value: JsonValue) {
  if (value === null) {
    return <span className="llm-log-json-value null">null</span>
  }
  if (typeof value === 'string') {
    return <span className="llm-log-json-value string">"{value}"</span>
  }
  if (typeof value === 'number') {
    return <span className="llm-log-json-value number">{value}</span>
  }
  if (typeof value === 'boolean') {
    return <span className="llm-log-json-value boolean">{String(value)}</span>
  }
  return null
}

export function JsonTreeNode({
  label,
  value,
  depth,
  defaultExpanded,
}: {
  label?: string
  value: JsonValue
  depth: number
  defaultExpanded?: boolean
}) {
  if (!Array.isArray(value) && !isRecord(value)) {
    return (
      <div className="llm-log-json-row">
        {label ? <span className="llm-log-json-key">{label}</span> : null}
        <span className="llm-log-json-sep">:</span>
        {renderJsonPrimitive(value)}
      </div>
    )
  }

  const isArray = Array.isArray(value)
  const entries = isArray
    ? value.map((item, index) => [`[${index}]`, item] as const)
    : Object.entries(value)

  const summary = isArray ? `数组(${entries.length})` : `对象(${entries.length})`
  const open = defaultExpanded ?? depth < 1

  return (
    <details className="llm-log-json-node" open={open}>
      <summary className="llm-log-json-summary">
        {label ? <span className="llm-log-json-key">{label}</span> : null}
        {label ? <span className="llm-log-json-sep">:</span> : null}
        <span className="llm-log-json-kind">{summary}</span>
      </summary>
      <div className="llm-log-json-children">
        {entries.map(([childKey, childValue]) => (
          <JsonTreeNode
            key={`${label || 'root'}-${childKey}`}
            label={childKey}
            value={childValue}
            depth={depth + 1}
            defaultExpanded={defaultExpanded}
          />
        ))}
      </div>
    </details>
  )
}

function toneLabel(tone: ParsedLogEntry['tone']) {
  switch (tone) {
    case 'error':
      return 'ERROR'
    case 'warn':
      return 'WARN'
    case 'info':
      return 'JSON'
    default:
      return 'TEXT'
  }
}

export function LlmLogPreview({ busy, file, tail }: LlmLogPreviewProps) {
  const entries = parseLlmLogPreviewTail(tail)
  const emptyText = busy ? '加载中…' : tail || '目录里还没有可预览的日志内容。'

  return (
    <div className="llm-log-preview-shell">
      {file ? (
        <p className="settings-note llm-log-preview-file" title={file}>
          {file}
        </p>
      ) : null}
      <div className="llm-log-preview-meta">显示 {entries.length} / {entries.length} 行</div>
      {entries.length === 0 ? (
        <div className="llm-log-preview-empty">{emptyText}</div>
      ) : (
        <div className="llm-log-preview-list">
          {entries.map((entry) => (
            <div
              key={`${entry.lineNumber}-${entry.raw.slice(0, 24)}`}
              className={`llm-log-preview-entry tone-${entry.tone}`}
            >
              <div className="llm-log-preview-entry-head">
                <span className="llm-log-preview-line">#{entry.lineNumber}</span>
                <span className={`llm-log-preview-pill tone-${entry.tone}`}>{toneLabel(entry.tone)}</span>
                <span className="llm-log-preview-summary">
                  {entry.type === 'json' ? entry.summary : entry.raw}
                </span>
              </div>

              {entry.type === 'json' ? (
                <div className="llm-log-preview-entry-body">
                  <JsonTreeNode value={entry.value} depth={0} />
                  <details className="llm-log-raw-block">
                    <summary>原始 JSON</summary>
                    <pre className="llm-log-raw-pre">{entry.raw}</pre>
                  </details>
                </div>
              ) : null}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

export default LlmLogPreview
