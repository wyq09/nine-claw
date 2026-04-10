import { useEffect, useMemo, useState } from 'react'
import { listTokenUsageRecords } from '../lib/piClient'
import type { TokenUsageRecord } from '../types'
import { AppIcon } from './AppIcon'

type UsageBucket = {
  label: string
  messageCount: number
  inputTokens: number
  outputTokens: number
  cacheTokens: number
  totalTokens: number
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

function normalizeDayLabel(timestamp: number): string {
  const date = new Date(timestamp)
  const year = date.getFullYear()
  const month = `${date.getMonth() + 1}`.padStart(2, '0')
  const day = `${date.getDate()}`.padStart(2, '0')
  return `${year}-${month}-${day}`
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

function UsageMetricCard({
  label,
  value,
  detail,
}: {
  label: string
  value: string
  detail: string
}) {
  return (
    <div className="usage-metric-card">
      <span className="usage-metric-label">{label}</span>
      <strong className="usage-metric-value">{value}</strong>
      <span className="usage-metric-detail">{detail}</span>
    </div>
  )
}

function UsageBreakdownTable({
  title,
  rows,
}: {
  title: string
  rows: UsageBucket[]
}) {
  return (
    <section className="usage-table-card">
      <div className="usage-table-head">
        <strong>{title}</strong>
        <span>{rows.length} 项</span>
      </div>
      <div className="usage-table-scroll">
        <table className="usage-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>消息</th>
              <th>总量</th>
              <th>输入</th>
              <th>输出</th>
              <th>缓存</th>
            </tr>
          </thead>
          <tbody>
            {rows.length === 0 ? (
              <tr>
                <td colSpan={6} className="usage-table-empty">
                  暂无数据
                </td>
              </tr>
            ) : (
              rows.map((row) => (
                <tr key={row.label}>
                  <td>{row.label}</td>
                  <td>{formatInteger(row.messageCount)}</td>
                  <td>{formatCompact(row.totalTokens)}</td>
                  <td>{formatCompact(row.inputTokens)}</td>
                  <td>{formatCompact(row.outputTokens)}</td>
                  <td>{formatCompact(row.cacheTokens)}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  )
}

function UsageTrendCard({ rows }: { rows: UsageBucket[] }) {
  const maxValue = rows.reduce((max, row) => Math.max(max, row.totalTokens), 0)

  return (
    <section className="usage-trend-card">
      <div className="usage-table-head">
        <strong>按天趋势</strong>
        <span>{rows.length} 天</span>
      </div>
      {rows.length === 0 ? (
        <div className="usage-trend-empty">暂无可展示的趋势数据</div>
      ) : (
        <div className="usage-trend-bars">
          {rows.map((row) => {
            const height = maxValue > 0 ? Math.max(10, Math.round((row.totalTokens / maxValue) * 100)) : 10
            return (
              <div key={row.label} className="usage-trend-bar-column">
                <span className="usage-trend-value">{formatCompact(row.totalTokens)}</span>
                <div className="usage-trend-bar-track">
                  <div className="usage-trend-bar-fill" style={{ height: `${height}%` }} />
                </div>
                <span className="usage-trend-label">{row.label.slice(5)}</span>
              </div>
            )
          })}
        </div>
      )}
    </section>
  )
}

export function UsageStatsPanel() {
  const [records, setRecords] = useState<TokenUsageRecord[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')

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

  const summary = useMemo(() => {
    return records.reduce(
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
  }, [records])

  const modelRows = useMemo(
    () => buildUsageBuckets(records, (record) => record.model?.trim() || '未标记模型'),
    [records],
  )
  const agentRows = useMemo(
    () => buildUsageBuckets(records, (record) => record.agentName?.trim() || '主聊天'),
    [records],
  )
  const dailyRows = useMemo(() => {
    const buckets = buildUsageBuckets(records, (record) =>
      normalizeDayLabel(record.turnCompletedAt ?? record.usageTimestamp ?? record.turnCreatedAt),
    )
    return [...buckets].sort((left, right) => left.label.localeCompare(right.label))
  }, [records])

  return (
    <div className="usage-dashboard">
      <div className="usage-dashboard-head">
        <div>
          <strong>SQLite 用量统计</strong>
          <p>按消息落库，当前展示的是数据库里的累计回复用量。</p>
        </div>
        <button type="button" className="outline-button" onClick={() => void loadRecords()} disabled={loading}>
          <AppIcon name="refresh" size={16} />
          <span>{loading ? '刷新中…' : '刷新'}</span>
        </button>
      </div>

      {error ? <div className="skills-feedback error">{error}</div> : null}

      <div className="usage-metric-grid">
        <UsageMetricCard label="总 Token" value={formatCompact(summary.totalTokens)} detail={`${formatInteger(summary.messages)} 条回复`} />
        <UsageMetricCard label="输入" value={formatCompact(summary.inputTokens)} detail={formatInteger(summary.inputTokens)} />
        <UsageMetricCard label="输出" value={formatCompact(summary.outputTokens)} detail={formatInteger(summary.outputTokens)} />
        <UsageMetricCard label="缓存" value={formatCompact(summary.cacheTokens)} detail={formatInteger(summary.cacheTokens)} />
      </div>

      <div className="usage-dashboard-grid">
        <UsageBreakdownTable title="按模型统计" rows={modelRows} />
        <UsageBreakdownTable title="按智能体统计" rows={agentRows} />
      </div>

      <UsageTrendCard rows={dailyRows} />

      <UsageBreakdownTable title="按天统计" rows={[...dailyRows].reverse()} />
    </div>
  )
}
