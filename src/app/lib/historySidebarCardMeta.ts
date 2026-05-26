import type { IconName } from '../../components/AppIcon'
import { formatHistorySessionListTime } from '../../lib/historySidebarBuckets'
import type { HistoryStatus } from '../../types'
import { formatHistoryAgeLabel, getStatusTone } from './appFormatting'

export type HistorySidebarCardMeta = {
  icon: IconName
  tone: string
  label: string
}

/** 侧栏会话条：进行中用相对文案，已完成/异常用日历时间文案 */
export function getHistorySidebarCardMeta(status: HistoryStatus, updatedAt: number): HistorySidebarCardMeta {
  if (status === 'running') {
    return {
      icon: 'refresh',
      tone: getStatusTone(status),
      label: formatHistoryAgeLabel(status, updatedAt),
    }
  }

  const tone = getStatusTone(status)
  if (status === 'done') {
    return {
      icon: 'check',
      tone,
      label: formatHistorySessionListTime(updatedAt),
    }
  }

  return {
    icon: 'stop',
    tone,
    label: formatHistorySessionListTime(updatedAt),
  }
}
