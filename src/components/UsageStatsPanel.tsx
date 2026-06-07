import { Download, RefreshCw } from 'lucide-react'
import { type ReactNode, useEffect, useMemo, useState } from 'react'
import { listTokenUsageRecords } from '../lib/piClient'
import {
  computeDailyBarHeight,
  formatDailyChartDayLabel,
  usageDailyChartMinWidth,
} from '../lib/usageStatsChart'
import type { TokenUsageRecord } from '../types'

type UsageBucket = {
  label: string
  messageCount: number
  inputTokens: number
  outputTokens: number
  cacheTokens: number
  totalTokens: number
}

type DatePreset = 'all' | '1d' | '7d' | '30d'

const PRESET_MS: Record<Exclude<DatePreset, 'all'>, number> = {
  '1d': 86400000,
  '7d': 7 * 86400000,
  '30d': 30 * 86400000,
}

const PRESET_LABELS: Record<DatePreset, string> = {
  all: '全部',
  '1d': '1d',
  '7d': '7d',
  '30d': '30d',
}

function formatInteger(value: number): string {
  return new Intl.NumberFormat('zh-CN').format(Math.round(value))
}

function formatCompact(value: number): string {
  const abs = Math.abs(value)
  if (abs >= 1_000_000_000) {
    return `${(value / 1_000_000_000).toFixed(2)}B`
  }
  if (abs >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(2)}M`
  }
  if (abs >= 1_000) {
    return `${(value / 1_000).toFixed(2)}K`
  }
  return formatInteger(value)
}

/** 表格主数字：大数用「万」，贴近 shadcn 参考里的展示习惯 */
function formatTokensTable(value: number): string {
  const abs = Math.abs(value)
  if (abs >= 10_000) {
    const wan = value / 10_000
    const s = wan >= 100 ? wan.toFixed(0) : wan.toFixed(1)
    return `${s}万`
  }
  if (abs >= 1_000) {
    return `${(value / 1_000).toFixed(2)}K`
  }
  return formatInteger(value)
}

function normalizeDayLabel(timestamp: number): string {
  const date = new Date(timestamp)
  const year = date.getFullYear()
  const month = `${date.getMonth() + 1}`.padStart(2, '0')
  const day = `${date.getDate()}`.padStart(2, '0')
  return `${year}-${month}-${day}`
}

function recordSortTime(record: TokenUsageRecord): number {
  return record.turnCompletedAt ?? record.usageTimestamp ?? record.turnCreatedAt
}

function filterRecordsByPreset(records: TokenUsageRecord[], preset: DatePreset): TokenUsageRecord[] {
  if (preset === 'all') {
    return records
  }
  const cutoff = Date.now() - PRESET_MS[preset]
  return records.filter((record) => recordSortTime(record) >= cutoff)
}

function formatDataRangeLabel(records: TokenUsageRecord[]): string {
  if (records.length === 0) {
    return '暂无数据'
  }
  const times: number[] = []
  for (const record of records) {
    times.push(record.turnCreatedAt, recordSortTime(record))
  }
  const min = Math.min(...times)
  const max = Math.max(...times)
  const fmt = new Intl.DateTimeFormat('zh-CN', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
  return `${fmt.format(new Date(min))} — ${fmt.format(new Date(max))}`
}

function escapeCsvCell(raw: string): string {
  if (/[",\n\r]/.test(raw)) {
    return `"${raw.replace(/"/g, '""')}"`
  }
  return raw
}

function exportUsageRecordsCsv(records: TokenUsageRecord[]): void {
  const headers = [
    'turnId',
    'sessionId',
    'turnCreatedAt',
    'turnCompletedAt',
    'agentId',
    'agentName',
    'api',
    'provider',
    'model',
    'inputTokens',
    'outputTokens',
    'cacheReadTokens',
    'cacheWriteTokens',
    'totalTokens',
    'recordedAt',
  ] as const

  const lines = [headers.join(',')]
  for (const record of records) {
    const row = headers.map((key) => {
      const value = record[key]
      if (value === null || value === undefined) {
        return ''
      }
      return escapeCsvCell(String(value))
    })
    lines.push(row.join(','))
  }

  const blob = new Blob([`\uFEFF${lines.join('\n')}`], { type: 'text/csv;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = `nineclaw-usage-${new Date().toISOString().slice(0, 10)}.csv`
  anchor.click()
  URL.revokeObjectURL(url)
}

function buildUsageBuckets(
  records: TokenUsageRecord[],
  keySelector: (record: TokenUsageRecord) => string,
): UsageBucket[] {
  const map = new Map<string, UsageBucket>()

  for (const record of records) {
    const label = keySelector(record)
    const current = map.get(label) ?? {
      label,
      messageCount: 0,
      inputTokens: 0,
      outputTokens: 0,
      cacheTokens: 0,
      totalTokens: 0,
    }

    current.messageCount += 1
    current.inputTokens += record.inputTokens
    current.outputTokens += record.outputTokens
    current.cacheTokens += record.cacheReadTokens + record.cacheWriteTokens
    current.totalTokens += record.totalTokens
    map.set(label, current)
  }

  return [...map.values()].sort((left, right) => right.totalTokens - left.totalTokens)
}

function UsageSummaryStat({
  label,
  valueCompact,
  valueExact,
}: {
  label: string
  valueCompact: string
  valueExact: string
}) {
  const showBoth = valueCompact !== valueExact
  return (
    <div className="usage-summary-stat">
      <span className="usage-summary-stat-label">{label}</span>
      <strong className="usage-summary-stat-value" title={valueExact}>
        {valueExact}
      </strong>
      {showBoth ? <span className="usage-summary-stat-compact">{valueCompact}</span> : null}
    </div>
  )
}

function ShadcnTableCard({
  title,
  description,
  rowCount,
  children,
}: {
  title: string
  description?: string
  rowCount: number
  children: ReactNode
}) {
  return (
    <div className="rounded-xl border border-border bg-card text-card-foreground shadow-sm">
      <div className="flex flex-wrap items-start justify-between gap-2 border-b border-border px-4 py-3">
        <div className="min-w-0 space-y-1">
          <h3 className="text-sm font-semibold leading-none tracking-tight text-card-foreground">{title}</h3>
          {description ? <p className="text-xs text-muted-foreground">{description}</p> : null}
        </div>
        <span className="shrink-0 rounded-md border border-border bg-muted/50 px-2 py-0.5 text-xs tabular-nums text-muted-foreground">
          {rowCount} 项
        </span>
      </div>
      <div className="relative max-h-[min(480px,52vh)] w-full overflow-auto">{children}</div>
    </div>
  )
}

function UsageBreakdownShadcnTable({
  title,
  description,
  nameHeader,
  rows,
}: {
  title: string
  description?: string
  nameHeader: string
  rows: UsageBucket[]
}) {
  return (
    <ShadcnTableCard title={title} description={description} rowCount={rows.length}>
      <table className="w-full caption-bottom text-sm">
        <thead>
          <tr className="border-b border-border">
            <th className="sticky top-0 z-1 h-11 bg-card px-4 text-left align-middle text-xs font-medium tracking-wide text-muted-foreground">
              {nameHeader}
            </th>
            <th className="sticky top-0 z-1 h-11 bg-card px-4 text-right align-middle text-xs font-medium tracking-wide text-muted-foreground">
              消息
            </th>
            <th className="sticky top-0 z-1 h-11 bg-card px-4 text-right align-middle text-xs font-medium tracking-wide text-muted-foreground">
              总量
            </th>
            <th className="sticky top-0 z-1 h-11 bg-card px-4 text-right align-middle text-xs font-medium tracking-wide text-muted-foreground">
              输入
            </th>
            <th className="sticky top-0 z-1 h-11 bg-card px-4 text-right align-middle text-xs font-medium tracking-wide text-muted-foreground">
              输出
            </th>
            <th className="sticky top-0 z-1 h-11 bg-card px-4 text-right align-middle text-xs font-medium tracking-wide text-muted-foreground">
              缓存
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 ? (
            <tr>
              <td colSpan={6} className="px-4 py-10 text-center text-sm text-muted-foreground">
                暂无数据
              </td>
            </tr>
          ) : (
            rows.map((row) => (
              <tr
                key={row.label}
                className="border-b border-border/50 transition-colors last:border-0 hover:bg-muted/40"
              >
                <td className="px-4 py-3 align-middle font-medium text-foreground">{row.label}</td>
                <td className="px-4 py-3 text-right align-middle tabular-nums text-foreground/90">
                  {formatInteger(row.messageCount)}
                </td>
                <td className="px-4 py-3 text-right align-middle tabular-nums text-foreground/90">
                  {formatTokensTable(row.totalTokens)}
                </td>
                <td className="px-4 py-3 text-right align-middle tabular-nums text-foreground/90">
                  {formatTokensTable(row.inputTokens)}
                </td>
                <td className="px-4 py-3 text-right align-middle tabular-nums text-foreground/90">
                  {formatTokensTable(row.outputTokens)}
                </td>
                <td className="px-4 py-3 text-right align-middle tabular-nums text-foreground/90">
                  {formatTokensTable(row.cacheTokens)}
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </ShadcnTableCard>
  )
}

function UsageDailyShadcnSection({ rows }: { rows: UsageBucket[] }) {
  const maxValue = rows.reduce((max, row) => Math.max(max, row.totalTokens), 0)
  const chronological = [...rows].sort((left, right) => left.label.localeCompare(right.label))

  return (
    <div className="usage-daily-card rounded-xl border border-border bg-card text-card-foreground shadow-sm">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border px-4 py-3">
        <div className="space-y-1">
          <h3 className="text-sm font-semibold leading-none tracking-tight">按天</h3>
          <p className="text-xs text-muted-foreground">趋势与明细 · {rows.length} 天</p>
        </div>
      </div>
      {rows.length === 0 ? (
        <div className="flex min-h-[200px] items-center justify-center px-4 text-sm text-muted-foreground">
          暂无可展示的趋势数据
        </div>
      ) : (
        <div className="usage-daily-body space-y-0 p-4 pt-3">
          <div className="usage-daily-chart-wrap">
            <div
              className="usage-daily-chart"
              style={{ minWidth: usageDailyChartMinWidth(chronological.length) }}
              role="img"
              aria-label="按天 Token 趋势"
            >
              {chronological.map((row) => {
                const barPx = computeDailyBarHeight(row.totalTokens, maxValue)
                return (
                  <div key={row.label} className="usage-daily-chart-column">
                    <span className="usage-daily-chart-value">{formatTokensTable(row.totalTokens)}</span>
                    <div className="usage-daily-chart-track">
                      <div className="usage-daily-chart-bar" style={{ height: barPx }} />
                    </div>
                    <span className="usage-daily-chart-date">{formatDailyChartDayLabel(row.label)}</span>
                  </div>
                )
              })}
            </div>
          </div>
          <div className="usage-daily-table-wrap relative max-h-[min(360px,45vh)] w-full overflow-auto rounded-lg border border-border/80">
            <table className="w-full caption-bottom text-sm">
              <thead>
                <tr className="border-b border-border">
                  <th className="sticky top-0 z-1 h-10 bg-card px-3 text-left align-middle text-xs font-medium tracking-wide text-muted-foreground">
                    日期
                  </th>
                  <th className="sticky top-0 z-1 h-10 bg-card px-3 text-right align-middle text-xs font-medium tracking-wide text-muted-foreground">
                    消息
                  </th>
                  <th className="sticky top-0 z-1 h-10 bg-card px-3 text-right align-middle text-xs font-medium tracking-wide text-muted-foreground">
                    总量
                  </th>
                  <th className="sticky top-0 z-1 h-10 bg-card px-3 text-right align-middle text-xs font-medium tracking-wide text-muted-foreground">
                    输入
                  </th>
                  <th className="sticky top-0 z-1 h-10 bg-card px-3 text-right align-middle text-xs font-medium tracking-wide text-muted-foreground">
                    输出
                  </th>
                  <th className="sticky top-0 z-1 h-10 bg-card px-3 text-right align-middle text-xs font-medium tracking-wide text-muted-foreground">
                    缓存
                  </th>
                </tr>
              </thead>
              <tbody>
                {chronological
                  .slice()
                  .reverse()
                  .map((row) => (
                  <tr
                    key={row.label}
                    className="border-b border-border/50 transition-colors last:border-0 hover:bg-muted/40"
                  >
                    <td className="px-3 py-2.5 align-middle text-foreground">{row.label}</td>
                    <td className="px-3 py-2.5 text-right align-middle tabular-nums text-foreground/90">
                      {formatInteger(row.messageCount)}
                    </td>
                    <td className="px-3 py-2.5 text-right align-middle tabular-nums text-foreground/90">
                      {formatTokensTable(row.totalTokens)}
                    </td>
                    <td className="px-3 py-2.5 text-right align-middle tabular-nums text-foreground/90">
                      {formatTokensTable(row.inputTokens)}
                    </td>
                    <td className="px-3 py-2.5 text-right align-middle tabular-nums text-foreground/90">
                      {formatTokensTable(row.outputTokens)}
                    </td>
                    <td className="px-3 py-2.5 text-right align-middle tabular-nums text-foreground/90">
                      {formatTokensTable(row.cacheTokens)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  )
}

export function UsageStatsPanel() {
  const [records, setRecords] = useState<TokenUsageRecord[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [datePreset, setDatePreset] = useState<DatePreset>('all')

  const loadRecords = async () => {
    setLoading(true)
    setError('')
    try {
      const next = await listTokenUsageRecords()
      setRecords(next)
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : String(loadError))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    void loadRecords()
  }, [])

  const filteredRecords = useMemo(() => filterRecordsByPreset(records, datePreset), [records, datePreset])

  const rangeLabel = useMemo(() => formatDataRangeLabel(filteredRecords), [filteredRecords])

  const summary = useMemo(() => {
    return filteredRecords.reduce(
      (acc, record) => {
        acc.messages += 1
        acc.inputTokens += record.inputTokens
        acc.outputTokens += record.outputTokens
        acc.cacheTokens += record.cacheReadTokens + record.cacheWriteTokens
        acc.totalTokens += record.totalTokens
        return acc
      },
      {
        messages: 0,
        inputTokens: 0,
        outputTokens: 0,
        cacheTokens: 0,
        totalTokens: 0,
      },
    )
  }, [filteredRecords])

  const modelRows = useMemo(
    () => buildUsageBuckets(filteredRecords, (record) => record.model?.trim() || '未标记模型'),
    [filteredRecords],
  )
  const agentRows = useMemo(
    () => buildUsageBuckets(filteredRecords, (record) => record.agentName?.trim() || '主聊天'),
    [filteredRecords],
  )
  const dailyRows = useMemo(() => {
    const buckets = buildUsageBuckets(filteredRecords, (record) =>
      normalizeDayLabel(record.turnCompletedAt ?? record.usageTimestamp ?? record.turnCreatedAt),
    )
    return [...buckets].sort((left, right) => left.label.localeCompare(right.label))
  }, [filteredRecords])

  const presets = (Object.keys(PRESET_LABELS) as DatePreset[]).map((key) => ({
    key,
    label: PRESET_LABELS[key],
  }))

  return (
    <div className="usage-dashboard flex flex-col gap-6">
      <div className="usage-dashboard-controls flex flex-col gap-3 rounded-xl border border-border bg-card/40 px-4 py-3 shadow-sm sm:flex-row sm:items-center sm:justify-between">
        <div className="flex min-w-0 flex-col gap-1 sm:flex-row sm:items-center sm:gap-3">
          <span className="text-xs font-medium tabular-nums text-muted-foreground">{rangeLabel}</span>
          <div className="usage-date-presets" role="group" aria-label="统计时间范围">
            {presets.map(({ key, label }) => {
              const active = datePreset === key
              return (
                <button
                  key={key}
                  type="button"
                  className={`usage-date-preset ${active ? 'active' : ''}`}
                  aria-pressed={active}
                  onClick={() => setDatePreset(key)}
                >
                  {label}
                </button>
              )
            })}
          </div>
        </div>
        <div className="usage-dashboard-actions flex flex-wrap items-center justify-end gap-2">
          <button
            type="button"
            className="outline-button usage-dashboard-refresh inline-flex items-center justify-center gap-2"
            onClick={() => void loadRecords()}
            disabled={loading}
          >
            <RefreshCw className={`size-4 ${loading ? 'animate-spin' : ''}`} aria-hidden />
            <span>{loading ? '刷新中…' : '刷新'}</span>
          </button>
          <button
            type="button"
            onClick={() => exportUsageRecordsCsv(filteredRecords)}
            disabled={filteredRecords.length === 0}
            className="inline-flex items-center justify-center gap-2 rounded-md border border-border bg-card px-3 py-2 text-sm font-medium text-foreground transition-colors hover:bg-muted/50 disabled:pointer-events-none disabled:opacity-40"
          >
            <Download className="size-4 shrink-0 opacity-80" aria-hidden />
            导出 CSV
          </button>
        </div>
      </div>

      {error ? <div className="skills-feedback error">{error}</div> : null}

      {loading && records.length === 0 ? (
        <div className="usage-dashboard-skeleton flex flex-col gap-3" aria-hidden>
          <div className="usage-skeleton usage-skeleton--hero rounded-xl" />
          <div className="usage-skeleton-row grid grid-cols-1 gap-3 sm:grid-cols-3">
            <div className="usage-skeleton h-16 rounded-xl" />
            <div className="usage-skeleton h-16 rounded-xl" />
            <div className="usage-skeleton h-16 rounded-xl" />
          </div>
        </div>
      ) : (
        <section className="usage-summary-panel">
          <div className="usage-summary-primary">
            <span className="usage-summary-primary-label">累计 Token</span>
            <strong className="usage-summary-primary-value">{formatCompact(summary.totalTokens)}</strong>
            <span className="usage-summary-primary-meta">{formatInteger(summary.messages)} 条回复（当前范围）</span>
          </div>
          <div className="usage-summary-divider" aria-hidden />
          <div className="usage-summary-breakdown">
            <UsageSummaryStat
              label="输入"
              valueCompact={formatCompact(summary.inputTokens)}
              valueExact={formatInteger(summary.inputTokens)}
            />
            <UsageSummaryStat
              label="输出"
              valueCompact={formatCompact(summary.outputTokens)}
              valueExact={formatInteger(summary.outputTokens)}
            />
            <UsageSummaryStat
              label="缓存"
              valueCompact={formatCompact(summary.cacheTokens)}
              valueExact={formatInteger(summary.cacheTokens)}
            />
          </div>
        </section>
      )}

      <div className="flex flex-col gap-6">
        <UsageBreakdownShadcnTable
          title="按模型统计"
          description="按模型聚合当前筛选范围内的用量"
          nameHeader="模型"
          rows={modelRows}
        />
        <UsageBreakdownShadcnTable
          title="按智能体统计"
          description="按智能体名称聚合当前筛选范围内的用量"
          nameHeader="智能体"
          rows={agentRows}
        />
      </div>

      <UsageDailyShadcnSection rows={dailyRows} />
    </div>
  )
}
