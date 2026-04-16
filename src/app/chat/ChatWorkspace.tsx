import type {
  ChangeEvent,
  ClipboardEvent,
  CSSProperties,
  KeyboardEvent,
  MutableRefObject,
  PointerEvent as ReactPointerEvent,
  RefObject,
  WheelEvent as ReactWheelEvent,
} from 'react'
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { AppIcon, type IconName } from '../../components/AppIcon'
import { ComposerAttachmentStrip } from '../../components/ComposerAttachmentStrip'
import { SessionContextBadge } from '../../components/SessionContextBadge'
import type {
  AgentBuilderDraft,
  ConversationAgentSnapshot,
  ConversationTurn,
  HistoryItem,
  PersistedChatAttachment,
  ProviderConfig,
  SubmitShortcut,
} from '../../types'
import {
  clampNumber,
  DEFAULT_COMPOSER_HEIGHT,
  getAgentColor,
  MAX_COMPOSER_HEIGHT,
  MIN_COMPOSER_HEIGHT,
  shouldSubmitWithShortcut,
  STARTER_CHIPS,
} from '../lib'
import { ImagePreviewModal } from './TurnAndTools'
import { useChatTurnWindow } from './useChatTurnWindow'
import { useSessionContextWindow } from '../../hooks/useSessionContextWindow'
import { VirtualizedChatTurns, type VirtualizedChatTurnsHandle } from './VirtualizedChatTurns'

function streamingTurnLayoutRevision(turn: ConversationTurn | undefined): number {
  if (!turn) {
    return 0
  }
  let n = turn.answer.length + turn.thinking.length + turn.toolCalls.length + turn.activity.length
  const segs = turn.responseSegments
  if (segs) {
    for (const s of segs) {
      n += s.type === 'text' ? s.text.length : 1
    }
  }
  return n
}

export type SidebarButtonProps = {
  active: boolean
  icon: IconName
  label: string
  onClick: () => void
}

export function SidebarButton({ active, icon, label, onClick }: SidebarButtonProps) {
  return (
    <button type="button" className={`sidebar-button ${active ? 'active' : ''}`} onClick={onClick}>
      <AppIcon name={icon} size={20} />
      <span>{label}</span>
    </button>
  )
}

export type ChatViewProps = {
  activeHistoryId: string
  activeHistoryItem: HistoryItem | null
  agentBuilderActionBusyId: string
  agentBuilderActionError: string
  agentBuilderActionNotice: string
  agentBuilderActionTargetId: string
  attachmentError: string
  attachmentInputRef: RefObject<HTMLInputElement | null>
  attachmentUploading: boolean
  composerAttachments: PersistedChatAttachment[]
  composerClearRef: MutableRefObject<(() => void) | null>
  composerDraftBackupRef: MutableRefObject<string>
  error: string
  globalBusy: boolean
  runningHistoryIds: string[]
  /** 已发起 `streamPiPrompt`（主对话流）；用于与任务意图等前置阶段区分 */
  streamingHistoryIds: string[]
  onAbort: () => void
  onComposerAttachmentInputChange: (event: ChangeEvent<HTMLInputElement>) => void
  onComposerClearAttachments: () => void
  onComposerPaste: (event: ClipboardEvent<HTMLTextAreaElement>) => void
  onComposerPickAttachment: () => void
  onComposerRemoveAttachment: (attachmentId: string) => void
  onComposerClearAttachmentError: () => void
  onCreateAgentDraft: (draft: AgentBuilderDraft, actionId: string) => Promise<void> | void
  onSubmit: (composerText: string) => void | Promise<void>
  selectedAgent: ConversationAgentSnapshot | null
  showExecutionRail: boolean
  showThinkingProcess: boolean
  submitShortcut: SubmitShortcut
  runtimeReady: boolean
  /** 非空表示已确认 PI 不可用；`null` 且 `!runtimeReady` 表示仍在检测 */
  runtimeBlockingReason: string | null
  /** 当前 session 对应的 provider 配置（仅 maxContextTokens），用于上下文窗口统计 */
  sessionContextProviderConfig: Pick<ProviderConfig, 'maxContextTokens'> | null
}

export function ChatView({
  activeHistoryId,
  activeHistoryItem,
  agentBuilderActionBusyId,
  agentBuilderActionError,
  agentBuilderActionNotice,
  agentBuilderActionTargetId,
  attachmentError,
  attachmentInputRef,
  attachmentUploading,
  composerAttachments,
  composerClearRef,
  composerDraftBackupRef,
  error,
  globalBusy,
  runningHistoryIds,
  streamingHistoryIds,
  onAbort,
  onComposerAttachmentInputChange,
  onComposerClearAttachments,
  onComposerPaste,
  onComposerPickAttachment,
  onComposerRemoveAttachment,
  onComposerClearAttachmentError,
  onCreateAgentDraft,
  onSubmit,
  selectedAgent,
  showExecutionRail,
  showThinkingProcess,
  submitShortcut,
  runtimeReady,
  runtimeBlockingReason,
  sessionContextProviderConfig,
}: ChatViewProps) {
  /** 非受控：避免每键入一字就重渲染整页消息列表（长会话 Markdown 极重） */
  const composerTextareaRef = useRef<HTMLTextAreaElement | null>(null)

  const { state: sessionContextState, loading: sessionContextLoading } = useSessionContextWindow({
    sessionId: activeHistoryId,
    providerConfig: sessionContextProviderConfig,
    turns: activeHistoryItem?.turns,
  })

  const [copiedTurnId, setCopiedTurnId] = useState('')
  const [copiedPromptTurnId, setCopiedPromptTurnId] = useState('')
  const [previewImage, setPreviewImage] = useState<{ src: string; alt: string } | null>(null)
  const [composerHeight, setComposerHeight] = useState(DEFAULT_COMPOSER_HEIGHT)
  const [isComposerResizing, setIsComposerResizing] = useState(false)
  const [showScrollToLatest, setShowScrollToLatest] = useState(false)
  /** 与流式生成状态配合：空输入时主按钮显示「处理中」，有输入时再显示可发送 */
  const [composerHasTypedContent, setComposerHasTypedContent] = useState(false)
  const resizeStateRef = useRef<{ startHeight: number; startY: number } | null>(null)
  const workspaceScrollRef = useRef<HTMLDivElement | null>(null)
  const virtualListRef = useRef<VirtualizedChatTurnsHandle | null>(null)
  /** 是否在智能体流式输出时自动滚到底；用户滚离底部后置 false，仅下次发送消息时恢复 */
  const followLatestOutputRef = useRef(true)
  /** 下一次 scroll 来自虚拟列表程序化滚底时跳过「用户离底」判定；无 scroll 时由 microtask 清掉 */
  const programmaticScrollPendingRef = useRef(false)

  useLayoutEffect(() => {
    composerClearRef.current = () => {
      const el = composerTextareaRef.current
      if (el) {
        el.value = ''
      }
      composerDraftBackupRef.current = ''
    }
    return () => {
      composerDraftBackupRef.current = composerTextareaRef.current?.value ?? ''
      composerClearRef.current = null
    }
  }, [composerClearRef, composerDraftBackupRef])
  const turns = activeHistoryItem?.turns ?? []
  const { visibleTurns, visibleRangeStart, hasMoreAbove, loadMoreAbove } = useChatTurnWindow(
    turns,
    activeHistoryId,
    workspaceScrollRef,
  )
  const activeTurnId = turns.at(-1)?.id ?? ''
  const isHomeState = !activeHistoryItem
  const sessionRunning = Boolean(activeHistoryItem && runningHistoryIds.includes(activeHistoryItem.id))
  const sessionStreamActive = Boolean(
    activeHistoryItem && streamingHistoryIds.includes(activeHistoryItem.id),
  )
  const sessionStreaming = sessionRunning
  const lastTurn = turns.at(-1)

  const syncComposerTypedPresence = useCallback(() => {
    const el = composerTextareaRef.current
    const raw = el?.value ?? ''
    setComposerHasTypedContent(raw.trim().length > 0)
  }, [])
  /** 纯数值：与 `turns` 引用解耦，复制状态变化时 revision 不变则子树不跟滚 */
  const streamingLayoutRevision =
    sessionRunning && lastTurn ? streamingTurnLayoutRevision(lastTurn) : 0

  /** 进入会话时恢复「跟到底」；具体滚动由 VirtualizedChatTurns 的 session 切换 / 首轮 layout 负责，避免与子组件重复 scrollToIndex 造成布局抖动 */
  useLayoutEffect(() => {
    if (isHomeState || !activeHistoryItem) {
      return
    }
    followLatestOutputRef.current = true
  }, [activeHistoryId, activeHistoryItem, isHomeState])

  /** 首页空闲时预取 Markdown 分包，减轻首轮发送后懒加载 chunk 的主线程卡顿 */
  useEffect(() => {
    if (!isHomeState) {
      return
    }
    void import('../../components/MarkdownRenderer')
  }, [isHomeState])

  useEffect(() => {
    setCopiedTurnId('')
    setCopiedPromptTurnId('')
    setPreviewImage(null)
  }, [activeHistoryId])

  useLayoutEffect(() => {
    syncComposerTypedPresence()
  }, [activeHistoryId, syncComposerTypedPresence])

  useEffect(() => {
    syncComposerTypedPresence()
  }, [sessionStreaming, syncComposerTypedPresence])

  useEffect(() => {
    const handlePointerMove = (event: PointerEvent) => {
      const resizeState = resizeStateRef.current
      if (!resizeState) {
        return
      }

      /* 拖拽条在输入区顶部：向上拖放大输入区、向下拖缩小 */
      const nextHeight = clampNumber(
        resizeState.startHeight - (event.clientY - resizeState.startY),
        MIN_COMPOSER_HEIGHT,
        MAX_COMPOSER_HEIGHT,
      )
      setComposerHeight(nextHeight)
    }

    const stopResize = () => {
      resizeStateRef.current = null
      setIsComposerResizing(false)
    }

    window.addEventListener('pointermove', handlePointerMove)
    window.addEventListener('pointerup', stopResize)
    window.addEventListener('pointercancel', stopResize)

    return () => {
      window.removeEventListener('pointermove', handlePointerMove)
      window.removeEventListener('pointerup', stopResize)
      window.removeEventListener('pointercancel', stopResize)
    }
  }, [])

  useEffect(() => {
    if (!isHomeState) {
      return
    }

    workspaceScrollRef.current?.scrollTo({ top: 0, behavior: 'auto' })
  }, [isHomeState])

  const syncScrollToLatestVisibility = useCallback(() => {
    const viewport = workspaceScrollRef.current
    if (!viewport || isHomeState) {
      setShowScrollToLatest(false)
      return
    }

    const distanceToBottom = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight
    setShowScrollToLatest(distanceToBottom > 180)
  }, [isHomeState])

  const markAutoScroll = useCallback(() => {
    programmaticScrollPendingRef.current = true
    queueMicrotask(() => {
      if (programmaticScrollPendingRef.current) {
        programmaticScrollPendingRef.current = false
        syncScrollToLatestVisibility()
      }
    })
  }, [syncScrollToLatestVisibility])

  const handleMessagesScroll = useCallback(() => {
    const viewport = workspaceScrollRef.current
    if (!viewport || isHomeState) {
      setShowScrollToLatest(false)
      return
    }
    const distanceToBottom = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight
    if (programmaticScrollPendingRef.current) {
      programmaticScrollPendingRef.current = false
      setShowScrollToLatest(distanceToBottom > 180)
      return
    }
    if (distanceToBottom > 100) {
      followLatestOutputRef.current = false
    }
    setShowScrollToLatest(distanceToBottom > 180)
  }, [isHomeState])

  const handleMessagesWheel = useCallback((event: ReactWheelEvent<HTMLDivElement>) => {
    if (event.deltaY < 0) {
      followLatestOutputRef.current = false
    }
  }, [])

  useEffect(() => {
    syncScrollToLatestVisibility()
  }, [syncScrollToLatestVisibility, activeHistoryId, activeTurnId, turns.length])

  const handleCopyAnswer = async (turnId: string, answer: string) => {
    if (!answer) {
      return
    }

    await navigator.clipboard.writeText(answer)
    setCopiedTurnId(turnId)
    window.setTimeout(() => setCopiedTurnId((current) => (current === turnId ? '' : current)), 1600)
  }

  const handleCopyPrompt = async (turnId: string, prompt: string) => {
    if (!prompt) {
      return
    }

    await navigator.clipboard.writeText(prompt)
    setCopiedPromptTurnId(turnId)
    window.setTimeout(() => setCopiedPromptTurnId((current) => (current === turnId ? '' : current)), 1600)
  }

  const handleComposerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (!shouldSubmitWithShortcut(event, submitShortcut)) {
      return
    }

    event.preventDefault()
    const text = composerTextareaRef.current?.value ?? ''
    followLatestOutputRef.current = true
    void onSubmit(text)
  }

  const handleMarkdownImageClick = (src: string, alt: string) => {
    setPreviewImage({ src, alt })
  }

  const handleScrollToLatest = () => {
    if (virtualListRef.current) {
      virtualListRef.current.scrollToLatest('instant')
      return
    }
    const viewport = workspaceScrollRef.current
    if (!viewport) {
      return
    }
    viewport.scrollTo({ top: viewport.scrollHeight, behavior: 'smooth' })
  }

  const handleComposerResizeStart = (event: ReactPointerEvent<HTMLDivElement>) => {
    resizeStateRef.current = {
      startHeight: composerHeight,
      startY: event.clientY,
    }
    setIsComposerResizing(true)
  }

  const composerPlaceholder = activeHistoryItem ? '继续对话…' : "说吧，我听着呢。"
  const composerInlineStyle = {
    '--composer-textarea-height': `${composerHeight}px`,
  } as CSSProperties

  const workspaceFooter = (
    <div className="workspace-footer-stack">
      {error && activeHistoryItem ? <div className="error-banner">{error}</div> : null}

      <div className={`workspace-composer-shell ${isComposerResizing ? 'is-resizing' : ''}`}>
        <form
          className={`composer-card ${activeHistoryItem && showScrollToLatest ? 'composer-card--scroll-btn' : ''}`}
          style={composerInlineStyle}
          onSubmit={(event) => {
            event.preventDefault()
            const el = composerTextareaRef.current
            const text = el?.value ?? ''
            followLatestOutputRef.current = true
            void onSubmit(text)
            requestAnimationFrame(() => {
              syncComposerTypedPresence()
            })
          }}
        >
          <input
            ref={attachmentInputRef}
            type="file"
            className="composer-file-input"
            multiple
            onChange={onComposerAttachmentInputChange}
          />
          {activeHistoryItem && showScrollToLatest ? (
            <button
              type="button"
              className="scroll-to-latest-button"
              onClick={handleScrollToLatest}
              aria-label="滚动到最新消息"
              title="滚动到最新消息"
            >
              <AppIcon name="arrow-down" size={18} />
            </button>
          ) : null}
          <div
            className="composer-resize-handle"
            role="separator"
            aria-label="拖动调整输入框高度"
            aria-orientation="horizontal"
            onPointerDown={handleComposerResizeStart}
          >
            <span />
          </div>
          <textarea
            ref={composerTextareaRef}
            defaultValue={composerDraftBackupRef.current}
            onInput={syncComposerTypedPresence}
            onKeyDown={handleComposerKeyDown}
            onPaste={(event) => {
              if (attachmentError) {
                onComposerClearAttachmentError()
              }
              void onComposerPaste(event)
            }}
            placeholder={composerPlaceholder}
            rows={4}
          />
          <ComposerAttachmentStrip
            attachments={composerAttachments}
            uploading={attachmentUploading}
            onRemove={onComposerRemoveAttachment}
            onClear={onComposerClearAttachments}
          />
          <div className="composer-toolbar">
            <div className="composer-toolbar-left">
              <button
                type="button"
                className="ghost-icon-button"
                aria-label="附件"
                onClick={() => {
                  if (attachmentError) {
                    onComposerClearAttachmentError()
                  }
                  onComposerPickAttachment()
                }}
                disabled={attachmentUploading || !selectedAgent}
              >
                <AppIcon name="attachment" size={18} />
              </button>
              <button type="button" className="ghost-icon-button" aria-label="技能">
                <AppIcon name="spark" size={18} />
              </button>
              {selectedAgent ? (
                <div className="agent-chip">
                  <span className="agent-chip-dot" style={{ backgroundColor: getAgentColor(selectedAgent) }} />
                  <span>{selectedAgent.name}</span>
                </div>
              ) : null}
            </div>
            <div className="composer-toolbar-right">
              {activeHistoryItem ? (
                <SessionContextBadge state={sessionContextState} loading={sessionContextLoading} />
              ) : null}
              {!runtimeReady ? (
                <button type="submit" className="submit-button" disabled aria-label="运行时未就绪">
                  <span className="composer-runtime-loading">⏳</span>
                </button>
              ) : sessionStreaming && !composerHasTypedContent ? (
                <button
                  type="button"
                  className="stop-button"
                  onClick={onAbort}
                  aria-label="立即停止当前生成"
                  title="立即停止当前生成"
                >
                  <AppIcon name="stop" size={16} />
                </button>
              ) : (
                <button
                  type="submit"
                  className="submit-button"
                  aria-label={sessionStreaming ? '发送并中断当前回复' : '发送'}
                  title={sessionStreaming ? '发送新消息（将中断当前回复）' : undefined}
                >
                  <AppIcon name="arrow-up" size={17} />
                </button>
              )}
            </div>
          </div>
        </form>
        {isHomeState ? (
          <div className="starter-chip-row">
            {STARTER_CHIPS.map((chip) => (
              <button
                key={chip}
                type="button"
                className="starter-chip"
                onClick={() => {
                  const el = composerTextareaRef.current
                  if (el) {
                    el.value = chip
                    el.focus()
                    syncComposerTypedPresence()
                  }
                }}
              >
                {chip}
              </button>
            ))}
          </div>
        ) : null}
        {attachmentError ? <div className="composer-attachment-error">{attachmentError}</div> : null}
        <div className="composer-footnote">
          {!runtimeReady
            ? runtimeBlockingReason ?? 'PI 运行时正在初始化，请稍候...'
            : sessionStreaming
              ? ''
              : globalBusy
                ? '其他会话也在执行中；当前会话仍可继续发送。'
                : ``}
        </div>
      </div>
    </div>
  )

  return (
    <div className={`workspace ${isHomeState ? 'workspace-home-state' : 'workspace-thread-state'}`}>
      {isHomeState ? (
        <div ref={workspaceScrollRef} className="workspace-home-cluster">
          <section className="new-task-home">
            <div className="home-brand-block">
              <div className="home-brand-mark">今天想让 NineClaw 帮你处理什么？</div>
            </div>
          </section>
          {workspaceFooter}
        </div>
      ) : (
        <>
          <div className="workspace-scroll workspace-thread-messages-host">
            <div
              ref={workspaceScrollRef}
              className="workspace-messages-scroller"
              onScroll={handleMessagesScroll}
              onWheel={handleMessagesWheel}
            >
              <VirtualizedChatTurns
                ref={virtualListRef}
                scrollParentRef={workspaceScrollRef}
                turns={visibleTurns}
                sessionTurns={turns}
                visibleRangeStart={visibleRangeStart}
                activeHistoryId={activeHistoryId}
                activeTurnId={activeTurnId}
                sessionRunning={sessionRunning}
                sessionStreamActive={sessionStreamActive}
                showExecutionRail={showExecutionRail}
                showThinkingProcess={showThinkingProcess}
                selectedAgent={selectedAgent}
                agentBuilderActionBusyId={agentBuilderActionBusyId}
                agentBuilderActionError={agentBuilderActionError}
                agentBuilderActionNotice={agentBuilderActionNotice}
                agentBuilderActionTargetId={agentBuilderActionTargetId}
                copiedTurnId={copiedTurnId}
                copiedPromptTurnId={copiedPromptTurnId}
                onCopyAnswer={handleCopyAnswer}
                onCopyPrompt={handleCopyPrompt}
                onCreateAgentDraft={onCreateAgentDraft}
                onImageClick={handleMarkdownImageClick}
                hasMoreOlder={hasMoreAbove}
                onLoadOlderTurns={loadMoreAbove}
                followLatestOutputRef={followLatestOutputRef}
                streamingLayoutRevision={streamingLayoutRevision}
                markAutoScroll={markAutoScroll}
              />
            </div>
          </div>
          {workspaceFooter}
        </>
      )}

      {previewImage ? (
        <ImagePreviewModal
          alt={previewImage.alt}
          src={previewImage.src}
          onClose={() => setPreviewImage(null)}
        />
      ) : null}
    </div>
  )
}

