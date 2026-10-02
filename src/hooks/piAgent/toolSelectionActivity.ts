import { listen } from '@tauri-apps/api/event'
import type { ActivityState } from '../../types'

/**
 * `pi://tool-selection` 事件载荷（独立于 pi://stream 的 Tauri 事件）：
 * 每轮对话开始前，后端决定本轮向模型暴露哪些工具，并把决策广播到前端。
 */
export interface ToolSelectionEventPayload {
  sessionId: string | null
  /** legacy：白名单原样放行；jev：Jev 模型裁剪；fallback：Jev 失败后回退白名单。 */
  source: 'legacy' | 'jev' | 'fallback'
  mode: 'legacy' | 'jev'
  requestedModel: string
  resolvedModel: string | null
  profile: 'code' | 'chat' | 'default' | null
  profileSource: 'jev' | 'keyword' | null
  enabledTools: string[]
  prunedTools: string[]
  reason: string
  latencyMs: number
  usage: { inputTokens: number; outputTokens: number } | null
}

export interface ToolSelectionActivity {
  title: string
  detail: string
}

type ActivityAppender = (
  historyId: string,
  turnId: string,
  label: string,
  detail: string,
  state: ActivityState,
) => void

/** 纯格式化：把工具路由决策压成一条活动流展示（与「本轮能力装配」同一形态）。 */
export function formatToolSelectionActivity(payload: ToolSelectionEventPayload): ToolSelectionActivity {
  const pruned = payload.prunedTools.filter(Boolean)
  if (payload.source === 'legacy' && pruned.length === 0) {
    return { title: '本轮工具路由', detail: '模式：原有' }
  }

  const parts: string[] = [`模式：${payload.mode === 'jev' ? 'jev' : '原有'}`]
  if (payload.profile) {
    parts.push(`画像：${payload.profile}${payload.profileSource ? `（${payload.profileSource}）` : ''}`)
  }
  parts.push(`启用 ${payload.enabledTools.length} 个工具`)
  if (pruned.length > 0) {
    parts.push(`裁剪：${pruned.join('、')}`)
  }
  parts.push(`耗时 ${payload.latencyMs}ms`)
  if (payload.source === 'fallback' && payload.reason.trim() !== '') {
    parts.push(`回退：${payload.reason.trim()}`)
  }
  return { title: '本轮工具路由', detail: parts.join('；') }
}

/**
 * 事件落地：把决策追加到对应会话当前轮次的活动流。
 * sessionId 为空时回退到当前激活会话；仍解析不到轮次则静默丢弃（如历史会话补放）。
 */
export function appendToolSelectionActivity(
  payload: ToolSelectionEventPayload,
  activeHistoryId: string,
  currentTurnIds: Map<string, string>,
  appendActivity: ActivityAppender,
): void {
  const historyId = payload.sessionId || activeHistoryId
  const turnId = historyId ? (currentTurnIds.get(historyId) ?? '') : ''
  if (!historyId || !turnId) {
    return
  }
  const { title, detail } = formatToolSelectionActivity(payload)
  appendActivity(historyId, turnId, title, detail, 'done')
}

export function subscribeToolSelectionEvent(
  onEvent: (payload: ToolSelectionEventPayload) => void,
): Promise<() => void> {
  return listen<ToolSelectionEventPayload>('pi://tool-selection', (event) => {
    onEvent(event.payload)
  })
}
