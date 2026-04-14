import { useVirtualizer } from '@tanstack/react-virtual'
import { Check, Copy } from 'lucide-react'
import {
  forwardRef,
  memo,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from 'react'
import { AppIcon } from '../../components/AppIcon'
import { PromptBubbleContent } from '../../components/PromptBubbleContent'
import { turnHasInlineAttachments, turnHasInlineFileAttachment } from '../../lib/inlineMedia'
import type { AgentBuilderDraft, ConversationAgentSnapshot, ConversationTurn } from '../../types'
import {
  formatChatTurnTimeFull,
  formatChatTurnTimeLabel,
  getElapsedMs,
  hasUsageMetrics,
  TokenUsageDetailPill,
} from '../lib'
import {
  isTurnWaitingOnly,
  shouldRenderAssistantColumn,
  TurnExecutionDetails,
  TurnResponseBody,
} from './TurnAndTools'

/** 同一会话内：仅当与上一条消息间隔 ≥ 此值时才显示居中时间 pill */
const CHAT_TURN_TIME_GAP_MS = 2 * 60 * 60 * 1000

function shouldShowTurnTimeMarker(sessionTurns: ConversationTurn[], globalIndex: number): boolean {
  if (globalIndex <= 0) {
    return true
  }
  const curr = sessionTurns[globalIndex]
  const prev = sessionTurns[globalIndex - 1]
  if (!curr || !prev) {
    return true
  }
  return curr.createdAt - prev.createdAt >= CHAT_TURN_TIME_GAP_MS
}

export type VirtualizedChatTurnsHandle = {
  scrollToLatest: (behavior?: 'smooth' | 'auto' | 'instant') => void
}

export type VirtualizedChatTurnsProps = {
  scrollParentRef: RefObject<HTMLDivElement | null>
  turns: ConversationTurn[]
  /** 当前会话完整轮次（用于与上一条比时间间隔；`turns` 可能仅为窗口切片） */
  sessionTurns: ConversationTurn[]
  /** `turns[0]` 在 `sessionTurns` 中的起始下标 */
  visibleRangeStart: number
  activeHistoryId: string
  activeTurnId: string
  sessionRunning: boolean
  /** 当前会话是否已启动主对话流（`streamPiPrompt`） */
  sessionStreamActive: boolean
  showExecutionRail: boolean
  showThinkingProcess: boolean
  selectedAgent: ConversationAgentSnapshot | null
  agentBuilderActionBusyId: string
  agentBuilderActionError: string
  agentBuilderActionNotice: string
  agentBuilderActionTargetId: string
  copiedTurnId: string
  copiedPromptTurnId: string
  onCopyAnswer: (turnId: string, answer: string) => void
  onCopyPrompt: (turnId: string, prompt: string) => void
  onCreateAgentDraft: (draft: AgentBuilderDraft, actionId: string) => Promise<void> | void
  onImageClick: (src: string, alt: string) => void
  /** 仍有更早轮次未挂载时，接近顶部触发（与 useChatTurnWindow 配合） */
  hasMoreOlder?: boolean
  onLoadOlderTurns?: () => void
  /** 为 false 时不在流式输出过程中自动滚到底（用户手动滚走后由 ChatWorkspace 置位） */
  followLatestOutputRef: RefObject<boolean>
  /** 随最后一轮正文/工具等变化递增，用于在流式生成时触发滚到底 */
  streamingLayoutRevision: number
  /** 程序化滚动前调用，避免 scroll 回调误判为用户离开底部 */
  markAutoScroll: () => void
}

const ChatTurnRow = memo(function ChatTurnRow({
  turn,
  isActive,
  isStreamingTurn,
  isStreamLive,
  isPreparingAssistant,
  isWaitingOnly,
  shouldShowActions,
  selectedAgent,
  agentBuilderActionBusyId,
  agentBuilderActionError,
  agentBuilderActionNotice,
  agentBuilderActionTargetId,
  activeTurnId,
  onCreateAgentDraft,
  showExecutionRail,
  showThinkingProcess,
  onImageClick,
  copiedTurnId,
  copiedPromptTurnId,
  onCopyAnswer,
  onCopyPrompt,
  showTurnTimeRow,
}: {
  turn: ConversationTurn
  showTurnTimeRow: boolean
  isActive: boolean
  isStreamingTurn: boolean
  isStreamLive: boolean
  isPreparingAssistant: boolean
  isWaitingOnly: boolean
  shouldShowActions: boolean
  selectedAgent: ConversationAgentSnapshot | null
  agentBuilderActionBusyId: string
  agentBuilderActionError: string
  agentBuilderActionNotice: string
  agentBuilderActionTargetId: string
  activeTurnId: string
  onCreateAgentDraft: (draft: AgentBuilderDraft, actionId: string) => Promise<void> | void
  showExecutionRail: boolean
  showThinkingProcess: boolean
  onImageClick: (src: string, alt: string) => void
  copiedTurnId: string
  copiedPromptTurnId: string
  onCopyAnswer: (turnId: string, answer: string) => void
  onCopyPrompt: (turnId: string, prompt: string) => void
}) {
  const [timeFullDetail, setTimeFullDetail] = useState(false)
  const timeLabel = timeFullDetail
    ? formatChatTurnTimeFull(turn.createdAt)
    : formatChatTurnTimeLabel(turn.createdAt)
  const usageShownBesideAttachments = useMemo(() => turnHasInlineAttachments(turn), [turn])
  const inlineFileCopy = useMemo(() => turnHasInlineFileAttachment(turn), [turn])
  const showAnswerFooterToken =
    Boolean(turn.usage) && hasUsageMetrics(turn.usage) && !usageShownBesideAttachments
  const showAnswerFooterCopy = !inlineFileCopy
  const showAnswerFooter = shouldShowActions && (showAnswerFooterToken || showAnswerFooterCopy)

  return (
    <article
      id={`chat-turn-${turn.id}`}
      className={`chat-turn ${isActive ? 'active' : ''}`}
    >
      {showTurnTimeRow ? (
        <div className="chat-turn-time-row">
          <button
            type="button"
            className="chat-turn-time-pill"
            aria-pressed={timeFullDetail}
            title={timeFullDetail ? '点击显示简短时间' : '点击显示月日、星期与时刻'}
            onClick={() => setTimeFullDetail((v) => !v)}
          >
            {timeLabel}
          </button>
        </div>
      ) : (
        <span className="visually-hidden">发送时间：{formatChatTurnTimeFull(turn.createdAt)}</span>
      )}
      <div className="prompt-block">
        <div className="prompt-meta">
          <button
            type="button"
            className={`prompt-copy-icon-button ${copiedPromptTurnId === turn.id ? 'copied' : ''}`}
            onClick={() => void onCopyPrompt(turn.id, turn.prompt)}
            aria-label={copiedPromptTurnId === turn.id ? '已复制提问' : '复制提问'}
            title={copiedPromptTurnId === turn.id ? '已复制提问' : '复制提问'}
          >
            {copiedPromptTurnId === turn.id ? <Check size={15} /> : <Copy size={15} />}
          </button>
        </div>
        <PromptBubbleContent content={turn.prompt} onImageClick={onImageClick} />
      </div>

      {shouldRenderAssistantColumn(turn, showExecutionRail, showThinkingProcess, isStreamingTurn) ? (
        <div className="chat-response">
          <div className="assistant-message-shell">
            <div
              className="assistant-avatar"
              style={
                selectedAgent?.accentColor
                  ? {
                      borderColor: `${selectedAgent.accentColor}55`,
                      background: `${selectedAgent.accentColor}22`,
                      color: selectedAgent.accentColor,
                    }
                  : undefined
              }
              aria-hidden
            >
              <AppIcon name="bot" size={20} />
            </div>
            <div className="assistant-message-stack">
              {shouldShowActions ? <TurnExecutionDetails turn={turn} isStreaming={isStreamingTurn} /> : null}
              <div
                className={`answer-result-card answer-result-card-chat${
                  isWaitingOnly ? ' answer-result-card-waiting' : ''
                }`}
              >
                <div className="chat-response-body">
                  <TurnResponseBody
                    turn={turn}
                    agentBuilderActionBusyId={agentBuilderActionBusyId}
                    agentBuilderActionError={agentBuilderActionError}
                    agentBuilderActionNotice={agentBuilderActionNotice}
                    agentBuilderActionTargetId={agentBuilderActionTargetId}
                    streamLive={isStreamLive}
                    preparing={isPreparingAssistant}
                    activeTurnId={activeTurnId}
                    onCreateAgentDraft={onCreateAgentDraft}
                    showExecutionRail={showExecutionRail}
                    showThinkingProcess={showThinkingProcess}
                    onImageClick={onImageClick}
                    copyAnswerControlSlot={
                      shouldShowActions && inlineFileCopy ? (
                        <button
                          type="button"
                          className={`answer-copy-control ${copiedTurnId === turn.id ? 'copied' : ''}`}
                          onClick={() => void onCopyAnswer(turn.id, turn.answer)}
                          aria-label={copiedTurnId === turn.id ? '已复制结果' : '复制结果'}
                          title={copiedTurnId === turn.id ? '已复制结果' : '复制结果'}
                          disabled={!turn.answer}
                        >
                          {copiedTurnId === turn.id ? <Check size={16} /> : <Copy size={16} />}
                          <span className="answer-copy-control-label">
                            {copiedTurnId === turn.id ? '已复制' : '复制'}
                          </span>
                        </button>
                      ) : undefined
                    }
                  />
                </div>
              </div>
              {showAnswerFooter ? (
                <div className="assistant-message-answer-footer">
                  <div className="assistant-answer-footer-actions">
                    {showAnswerFooterToken && turn.usage ? <TokenUsageDetailPill usage={turn.usage} /> : null}
                    {showAnswerFooterCopy ? (
                      <button
                        type="button"
                        className={`answer-copy-control ${copiedTurnId === turn.id ? 'copied' : ''}`}
                        onClick={() => void onCopyAnswer(turn.id, turn.answer)}
                        aria-label={copiedTurnId === turn.id ? '已复制结果' : '复制结果'}
                        title={copiedTurnId === turn.id ? '已复制结果' : '复制结果'}
                        disabled={!turn.answer}
                      >
                        {copiedTurnId === turn.id ? <Check size={16} /> : <Copy size={16} />}
                        <span className="answer-copy-control-label">
                          {copiedTurnId === turn.id ? '已复制' : '复制'}
                        </span>
                      </button>
                    ) : null}
                  </div>
                </div>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}
    </article>
  )
})

export const VirtualizedChatTurns = forwardRef<VirtualizedChatTurnsHandle, VirtualizedChatTurnsProps>(
  function VirtualizedChatTurns(
    {
      scrollParentRef,
      turns,
      sessionTurns,
      visibleRangeStart,
      activeHistoryId,
      activeTurnId,
      sessionRunning,
      sessionStreamActive,
      showExecutionRail,
      showThinkingProcess,
      selectedAgent,
      agentBuilderActionBusyId,
      agentBuilderActionError,
      agentBuilderActionNotice,
      agentBuilderActionTargetId,
      copiedTurnId,
      copiedPromptTurnId,
      onCopyAnswer,
      onCopyPrompt,
      onCreateAgentDraft,
      onImageClick,
      hasMoreOlder = false,
      onLoadOlderTurns,
      followLatestOutputRef,
      streamingLayoutRevision,
      markAutoScroll,
    },
    ref,
  ) {
    const prevHistoryIdRef = useRef(activeHistoryId)
    const firstScrollRef = useRef(true)
    const turnsRef = useRef(turns)
    turnsRef.current = turns

    const virtualizer = useVirtualizer({
      count: turns.length,
      getScrollElement: () => scrollParentRef.current,
      estimateSize: () => 200,
      overscan: 4,
      gap: 16,
      getItemKey: (index) => turns[index]?.id ?? index,
    })

    const hasMoreOlderRef = useRef(hasMoreOlder)
    hasMoreOlderRef.current = hasMoreOlder
    const loadOlderGuardRef = useRef(false)

    useEffect(() => {
      loadOlderGuardRef.current = false
    }, [hasMoreOlder])

    useEffect(() => {
      if (!hasMoreOlder || !onLoadOlderTurns) {
        return
      }
      const el = scrollParentRef.current
      if (!el) {
        return
      }
      const topNear = 140
      const releaseBelow = 240
      const onScroll = () => {
        if (!hasMoreOlderRef.current) {
          return
        }
        if (el.scrollTop < topNear) {
          if (!loadOlderGuardRef.current) {
            loadOlderGuardRef.current = true
            onLoadOlderTurns()
          }
        } else if (el.scrollTop > releaseBelow) {
          loadOlderGuardRef.current = false
        }
      }
      el.addEventListener('scroll', onScroll, { passive: true })
      return () => {
        el.removeEventListener('scroll', onScroll)
      }
    }, [hasMoreOlder, onLoadOlderTurns, scrollParentRef])

    const virtualizerRef = useRef(virtualizer)
    virtualizerRef.current = virtualizer
    const streamFollowScrollRafRef = useRef<number | null>(null)

    useImperativeHandle(
      ref,
      () => ({
        scrollToLatest: (behavior = 'smooth') => {
          const len = turnsRef.current.length
          if (len === 0) {
            return
          }
          markAutoScroll()
          virtualizerRef.current.scrollToIndex(len - 1, { align: 'end', behavior })
        },
      }),
      [markAutoScroll],
    )

    useLayoutEffect(() => {
      const list = turnsRef.current
      if (list.length === 0 || !activeTurnId) {
        return
      }
      const idx = list.findIndex((t) => t.id === activeTurnId)
      if (idx < 0) {
        return
      }
      const switchedSession = prevHistoryIdRef.current !== activeHistoryId
      prevHistoryIdRef.current = activeHistoryId
      const useInstant = switchedSession || firstScrollRef.current
      firstScrollRef.current = false
      markAutoScroll()
      virtualizerRef.current.scrollToIndex(idx, {
        align: 'end',
        behavior: useInstant ? 'instant' : 'smooth',
      })
    }, [activeHistoryId, activeTurnId, markAutoScroll])

    /** 流式阶段每字都会改 layoutRevision；用 rAF 合并 scrollToIndex，避免同步 layout 风暴卡住主线程 */
    useEffect(() => {
      if (!sessionRunning || !followLatestOutputRef.current) {
        return
      }
      if (streamFollowScrollRafRef.current !== null) {
        cancelAnimationFrame(streamFollowScrollRafRef.current)
      }
      streamFollowScrollRafRef.current = requestAnimationFrame(() => {
        streamFollowScrollRafRef.current = null
        if (!followLatestOutputRef.current) {
          return
        }
        const len = turnsRef.current.length
        if (len === 0) {
          return
        }
        markAutoScroll()
        virtualizerRef.current.scrollToIndex(len - 1, { align: 'end', behavior: 'instant' })
      })
      return () => {
        if (streamFollowScrollRafRef.current !== null) {
          cancelAnimationFrame(streamFollowScrollRafRef.current)
          streamFollowScrollRafRef.current = null
        }
      }
    }, [sessionRunning, streamingLayoutRevision, followLatestOutputRef, markAutoScroll])

    if (turns.length === 0) {
      return <div className="chat-message-list chat-message-list--empty" />
    }

    const items = virtualizer.getVirtualItems()

    return (
      <div className="chat-message-list chat-message-list--virtual">
        <div
          className="chat-message-list-virtual-inner"
          style={{
            height: virtualizer.getTotalSize(),
            position: 'relative',
            width: '100%',
          }}
        >
          {items.map((vi) => {
            const item = turns[vi.index]
            if (!item) {
              return null
            }
            const isStreamingTurn = sessionRunning && item.id === activeTurnId
            const isStreamLive = sessionStreamActive && item.id === activeTurnId
            const isPreparingAssistant = isStreamingTurn && !sessionStreamActive
            const isWaitingOnly = isTurnWaitingOnly(item, isStreamingTurn, showExecutionRail, showThinkingProcess)
            const shouldShowActions =
              !isWaitingOnly &&
              Boolean(
                item.answer ||
                  getElapsedMs(item.createdAt, item.completedAt, isStreamingTurn) ||
                  hasUsageMetrics(item.usage),
              )
            const globalIndex = visibleRangeStart + vi.index
            const showTurnTimeRow = shouldShowTurnTimeMarker(sessionTurns, globalIndex)

            return (
              <div
                key={vi.key}
                data-index={vi.index}
                ref={virtualizer.measureElement}
                className="chat-turn-virtual-row"
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  transform: `translateY(${vi.start}px)`,
                }}
              >
                <ChatTurnRow
                  turn={item}
                  showTurnTimeRow={showTurnTimeRow}
                  isActive={item.id === activeTurnId}
                  isStreamingTurn={isStreamingTurn}
                  isStreamLive={isStreamLive}
                  isPreparingAssistant={isPreparingAssistant}
                  isWaitingOnly={isWaitingOnly}
                  shouldShowActions={shouldShowActions}
                  selectedAgent={selectedAgent}
                  agentBuilderActionBusyId={agentBuilderActionBusyId}
                  agentBuilderActionError={agentBuilderActionError}
                  agentBuilderActionNotice={agentBuilderActionNotice}
                  agentBuilderActionTargetId={agentBuilderActionTargetId}
                  activeTurnId={activeTurnId}
                  onCreateAgentDraft={onCreateAgentDraft}
                  showExecutionRail={showExecutionRail}
                  showThinkingProcess={showThinkingProcess}
                  onImageClick={onImageClick}
                  copiedTurnId={copiedTurnId}
                  copiedPromptTurnId={copiedPromptTurnId}
                  onCopyAnswer={onCopyAnswer}
                  onCopyPrompt={onCopyPrompt}
                />
              </div>
            )
          })}
        </div>
      </div>
    )
  },
)
