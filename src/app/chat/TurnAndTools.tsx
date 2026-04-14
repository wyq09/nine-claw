import { lazy, Suspense, useMemo, useState } from 'react'
import type { MouseEvent, ReactNode } from 'react'
import { Check, Copy } from 'lucide-react'
import { AppIcon } from '../../components/AppIcon'
import { InlineMediaAttachmentList } from '../../components/InlineMediaAttachmentList'
import ReplyCardStack from '../../components/ReplyCardStack'
import { extractInlineMediaAttachments, normalizeMarkdownImageSources } from '../../lib/inlineMedia'
import { openExternalUrl } from '../../lib/piClient'
import { resolveReplyCardItems } from '../../lib/replyCardFormat'
import type { AgentBuilderDraft, ConversationTurn, TokenUsage, ToolCallEntry } from '../../types'
import { LazyDetails } from './LazyDetails'
import { TurnThinkingBlock } from './TurnThinkingBlock'
import {
  formatAgentExecutionModeLabel,
  formatDurationLabel,
  getElapsedMs,
  hasUsageMetrics,
  parseAgentBuilderDraft,
  stripAgentBuilderBlock,
  TURN_PLACEHOLDER_NO_OUTPUT,
  TokenUsageDetailPill,
  useLiveNow,
} from '../lib'

const MarkdownRenderer = lazy(() => import('../../components/MarkdownRenderer'))

export function TurnResponseBody({
  turn,
  agentBuilderActionBusyId,
  agentBuilderActionError,
  agentBuilderActionNotice,
  agentBuilderActionTargetId,
  streamLive,
  preparing = false,
  activeTurnId,
  onCreateAgentDraft,
  showExecutionRail,
  showThinkingProcess,
  onImageClick,
  copyAnswerControlSlot,
}: {
  turn: ConversationTurn
  agentBuilderActionBusyId: string
  agentBuilderActionError: string
  agentBuilderActionNotice: string
  agentBuilderActionTargetId: string
  /** 主对话流已建立，可接收 delta */
  streamLive: boolean
  /** 仍在任务意图 / 连接等前置阶段，未进入主对话流 */
  preparing?: boolean
  activeTurnId: string
  onCreateAgentDraft: (draft: AgentBuilderDraft, actionId: string) => Promise<void> | void
  showExecutionRail: boolean
  showThinkingProcess: boolean
  onImageClick: (src: string, alt: string) => void
  /** 含文件附件时：复制按钮放入附件工具行，由父级传入 */
  copyAnswerControlSlot?: ReactNode
}) {
  const toolById = new Map(turn.toolCalls.map((t) => [t.toolCallId, t]))
  const segments = turn.responseSegments
  const isActiveStreamingTurn = streamLive && turn.id === activeTurnId
  const runningToolCount = turn.toolCalls.filter((toolCall) => toolCall.state === 'running').length

  if (segments && segments.length > 0) {
    const renderBlocks: Array<
      | { type: 'text'; text: string; index: number }
      | { type: 'tools'; toolCalls: ToolCallEntry[]; index: number }
    > = []
    let groupedToolCalls: ToolCallEntry[] = []
    let groupedToolStartIndex = -1

    const flushGroupedToolCalls = () => {
      if (groupedToolCalls.length === 0) {
        return
      }
      renderBlocks.push({
        type: 'tools',
        toolCalls: groupedToolCalls,
        index: groupedToolStartIndex,
      })
      groupedToolCalls = []
      groupedToolStartIndex = -1
    }

    segments.forEach((segment, index) => {
      if (segment.type === 'text') {
        flushGroupedToolCalls()
        renderBlocks.push({ type: 'text', text: segment.text, index })
        return
      }

      if (!showExecutionRail) {
        return
      }

      const toolCall = toolById.get(segment.toolCallId)
      if (!toolCall) {
        flushGroupedToolCalls()
        return
      }

      if (groupedToolCalls.length === 0) {
        groupedToolStartIndex = index
      }
      groupedToolCalls.push(toolCall)
    })

    flushGroupedToolCalls()

    let lastTextSegmentIndex = -1
    for (let i = renderBlocks.length - 1; i >= 0; i -= 1) {
      if (renderBlocks[i]?.type === 'text') {
        lastTextSegmentIndex = renderBlocks[i].index
        break
      }
    }

    const firstToolsBlockIndex = renderBlocks.findIndex(
      (b) => b.type === 'tools' && b.toolCalls.length > 0,
    )
    const thinkingEmbeddedInRail =
      showThinkingProcess && turn.thinking.trim() && firstToolsBlockIndex >= 0

    let lastTextIndexWithAttachments = -1
    for (let i = renderBlocks.length - 1; i >= 0; i -= 1) {
      const b = renderBlocks[i]
      if (
        b?.type === 'text' &&
        extractInlineMediaAttachments(b.text).attachments.length > 0
      ) {
        lastTextIndexWithAttachments = b.index
        break
      }
    }

    let lastTextIndexWithFileAttachments = -1
    for (let i = renderBlocks.length - 1; i >= 0; i -= 1) {
      const b = renderBlocks[i]
      if (
        b?.type === 'text' &&
        extractInlineMediaAttachments(b.text).attachments.some((a) => a.kind === 'file')
      ) {
        lastTextIndexWithFileAttachments = b.index
        break
      }
    }

    return (
      <div className="turn-response-blocks">
        {showThinkingProcess && turn.thinking.trim() && !thinkingEmbeddedInRail ? (
          <TurnThinkingBlock isStreaming={isActiveStreamingTurn} thinking={turn.thinking} />
        ) : null}
        {renderBlocks.map((block, blockIndex) => {
          if (block.type === 'text') {
            const isStreaming = Boolean(
              isActiveStreamingTurn && block.index === lastTextSegmentIndex,
            )
            if ((!block.text.trim() && !isStreaming) || isTurnPlaceholderNoOutputText(block.text)) {
              return null
            }
            const showUsageBesideAttachments =
              hasUsageMetrics(turn.usage) && block.index === lastTextIndexWithAttachments
            const attachmentCopySlot =
              copyAnswerControlSlot && block.index === lastTextIndexWithFileAttachments
                ? copyAnswerControlSlot
                : undefined
            return (
              <MarkdownBlock
                key={`${turn.id}-t-${block.index}`}
                actionId={`${turn.id}-t-${block.index}`}
                actionBusyId={agentBuilderActionBusyId}
                actionError={agentBuilderActionError}
                actionNotice={agentBuilderActionNotice}
                actionTargetId={agentBuilderActionTargetId}
                content={block.text}
                isStreaming={isStreaming}
                onCreateAgentDraft={onCreateAgentDraft}
                onImageClick={onImageClick}
                usage={turn.usage}
                showUsageBesideAttachments={showUsageBesideAttachments}
                attachmentCopySlot={attachmentCopySlot}
              />
            )
          }

          if (block.toolCalls.length === 0) {
            return null
          }

          return (
            <TurnExecutionRail
              key={`${turn.id}-tools-${block.index}`}
              toolCalls={block.toolCalls}
              thinking={blockIndex === firstToolsBlockIndex ? turn.thinking : ''}
              showThinkingProcess={showThinkingProcess}
              runningToolCount={runningToolCount}
              onImageClick={onImageClick}
            />
          )
        })}
      </div>
    )
  }

  const legacyTools = [...turn.toolCalls].sort((a, b) => a.createdAt - b.createdAt)
  const hasLegacyTools = showExecutionRail && legacyTools.length > 0
  const legacyThinkingInRail = showThinkingProcess && turn.thinking.trim() && hasLegacyTools

  return (
    <>
      {showThinkingProcess && turn.thinking.trim() && !legacyThinkingInRail ? (
        <TurnThinkingBlock isStreaming={isActiveStreamingTurn} thinking={turn.thinking} />
      ) : null}
      {turn.answer && !isTurnPlaceholderNoOutputText(turn.answer) ? (
        <MarkdownBlock
          actionId={`${turn.id}-legacy`}
          actionBusyId={agentBuilderActionBusyId}
          actionError={agentBuilderActionError}
          actionNotice={agentBuilderActionNotice}
          actionTargetId={agentBuilderActionTargetId}
          content={turn.answer}
          isStreaming={streamLive && turn.id === activeTurnId}
          onCreateAgentDraft={onCreateAgentDraft}
          onImageClick={onImageClick}
          usage={turn.usage}
          showUsageBesideAttachments={
            hasUsageMetrics(turn.usage) &&
            extractInlineMediaAttachments(turn.answer).attachments.length > 0
          }
          attachmentCopySlot={
            copyAnswerControlSlot &&
            extractInlineMediaAttachments(turn.answer).attachments.some((a) => a.kind === 'file')
              ? copyAnswerControlSlot
              : undefined
          }
        />
      ) : null}
      {hasLegacyTools ? (
        <TurnExecutionRail
          toolCalls={legacyTools}
          thinking={turn.thinking}
          showThinkingProcess={showThinkingProcess}
          runningToolCount={runningToolCount}
          onImageClick={onImageClick}
        />
      ) : null}
      {!turn.answer && !hasLegacyTools ? (
        preparing && turn.id === activeTurnId ? (
          <TurnPreparingIndicator />
        ) : isActiveStreamingTurn ? (
          <TurnWaitingIndicator startedAt={turn.createdAt} />
        ) : turn.status === 'done' ? (
          <p className="placeholder-copy">本轮已结束，但模型没有返回任何可渲染内容。</p>
        ) : turn.status === 'error' ? (
          <p className="placeholder-copy">本轮执行失败，未产出可渲染内容。</p>
        ) : legacyTools.length > 0 ? (
          <p className="placeholder-copy">本轮主要产出了工具调用结果。</p>
        ) : null
      ) : null}
    </>
  )
}

/** 前置逻辑（如定时任务意图）进行中：轻量提示，避免与主对话流混淆 */
export function TurnPreparingIndicator() {
  return (
    <div className="turn-preparing-indicator" role="status" aria-live="polite">
      <span className="turn-waiting-dots" aria-hidden="true">
        <span />
        <span />
        <span />
      </span>
      <span className="turn-waiting-label">准备中</span>
      <span className="turn-preparing-hint">正在连接模型…</span>
    </div>
  )
}

export function TurnWaitingIndicator({ startedAt }: { startedAt: number }) {
  const now = useLiveNow(true, 500)
  const elapsed = Math.max(0, now - startedAt)

  return (
    <div className="turn-waiting-indicator" role="status" aria-live="polite">
      <span className="turn-waiting-dots" aria-hidden="true">
        <span />
        <span />
        <span />
      </span>
      <span className="turn-waiting-label">处理中</span>
      <span className="turn-waiting-time">已执行 {formatDurationLabel(elapsed)}</span>
    </div>
  )
}

export function MarkdownBlock({
  actionId,
  actionBusyId,
  actionError,
  actionNotice,
  actionTargetId,
  content,
  isStreaming,
  onCreateAgentDraft,
  onImageClick,
  usage,
  showUsageBesideAttachments,
  attachmentCopySlot,
}: {
  actionId: string
  actionBusyId: string
  actionError: string
  actionNotice: string
  actionTargetId: string
  content: string
  isStreaming: boolean
  onCreateAgentDraft: (draft: AgentBuilderDraft, actionId: string) => Promise<void> | void
  onImageClick?: (src: string, alt: string) => void
  usage?: TokenUsage
  /** 为 true 时在有附件时将「总 Token」与附件排在同一行（仅应由每轮最后一段带附件的正文启用） */
  showUsageBesideAttachments?: boolean
  /** 含文件附件时：复制按钮与打开/下载同一行 */
  attachmentCopySlot?: ReactNode
}) {
  const agentDraft = parseAgentBuilderDraft(content)
  const cleanedContent = stripAgentBuilderBlock(content)
  const { contentWithoutAttachments, attachments } = useMemo(
    () => extractInlineMediaAttachments(cleanedContent),
    [cleanedContent],
  )
  const normalizedContent = useMemo(
    () => normalizeMarkdownImageSources(contentWithoutAttachments),
    [contentWithoutAttachments],
  )

  const replyCardItems = useMemo(
    () => resolveReplyCardItems(normalizedContent || '', isStreaming),
    [normalizedContent, isStreaming],
  )

  return (
    <>
      {normalizedContent ? (
        <ReplyCardStack items={replyCardItems} isStreaming={isStreaming} onImageClick={onImageClick} />
      ) : null}
      {attachments.length > 0 ? (
        <InlineMediaAttachmentList
          attachments={attachments}
          copyActionSlot={attachmentCopySlot}
          onImageClick={onImageClick}
          trailingSlot={
            showUsageBesideAttachments && usage && hasUsageMetrics(usage) ? (
              <TokenUsageDetailPill usage={usage} />
            ) : undefined
          }
        />
      ) : null}
      {agentDraft ? (
        <AgentBuilderDraftCard
          actionError={actionTargetId === actionId && actionBusyId !== actionId ? actionError : ''}
          actionId={actionId}
          actionNotice={actionTargetId === actionId && actionBusyId !== actionId ? actionNotice : ''}
          busy={actionBusyId === actionId}
          draft={agentDraft}
          onCreate={onCreateAgentDraft}
        />
      ) : null}
    </>
  )
}

export function MarkdownFallback({ content }: { content: string }) {
  return <div className="markdown-content-fallback">{content || ' '}</div>
}

export function AgentBuilderDraftCard({
  actionError,
  actionId,
  actionNotice,
  busy,
  draft,
  onCreate,
}: {
  actionError: string
  actionId: string
  actionNotice: string
  busy: boolean
  draft: AgentBuilderDraft
  onCreate: (draft: AgentBuilderDraft, actionId: string) => Promise<void> | void
}) {
  return (
    <div className="agent-builder-card">
      <div className="agent-builder-card-head">
        <div>
          <div className="agent-builder-card-kicker">智能体创建草案</div>
          <strong>{draft.name}</strong>
        </div>
        <span className="skill-pill subtle">{formatAgentExecutionModeLabel(draft.executionMode)}</span>
      </div>
      <p className="agent-builder-card-summary">{draft.summary}</p>
      <div className="agent-builder-card-grid">
        <span>默认模型：{draft.defaultProviderId && draft.defaultModel ? `${draft.defaultProviderId} / ${draft.defaultModel}` : '将使用当前聊天模型'}</span>
        <span>挂载技能：{draft.skillIds.length > 0 ? draft.skillIds.join('、') : '无'}</span>
      </div>
      {draft.workspaceNotes ? <div className="agent-builder-card-notes">{draft.workspaceNotes}</div> : null}
      {actionError ? <div className="skills-feedback error">{actionError}</div> : null}
      {actionNotice ? <div className="skills-feedback success">{actionNotice}</div> : null}
      <div className="agent-builder-card-actions">
        <button type="button" className="outline-button primary" onClick={() => void onCreate(draft, actionId)} disabled={busy}>
          {busy ? '创建中…' : '创建到智能体管理'}
        </button>
      </div>
    </div>
  )
}

export function getToolCallStateLabel(state: ToolCallEntry['state']): string {
  if (state === 'running') return '执行中'
  if (state === 'done') return '已完成'
  return '失败'
}

export function getToolGroupState(toolCalls: ToolCallEntry[]): ToolCallEntry['state'] {
  if (toolCalls.some((toolCall) => toolCall.state === 'running')) {
    return 'running'
  }
  if (toolCalls.some((toolCall) => toolCall.state === 'error')) {
    return 'error'
  }
  return 'done'
}

export function formatToolCallText(text: string, fallback: string, pretty = true): string {
  const trimmed = text.trim()
  if (!trimmed) {
    return fallback
  }
  if (!pretty) {
    return text
  }
  try {
    return JSON.stringify(JSON.parse(trimmed), null, 2)
  } catch {
    return trimmed
  }
}

/** 按空行将 thinking 粗分为每轮工具前一段 + 末尾纯思考段（与参考 UI 对齐） */
export function computeExecThoughtSlices(raw: string, toolCount: number): { perRound: string[]; tailOnly: string } {
  const perRound = Array.from({ length: toolCount }, () => '')
  let tailOnly = ''
  const t = raw.trim()
  if (!t || toolCount === 0) {
    return { perRound, tailOnly }
  }

  const parts = t
    .split(/\n{2,}/)
    .map((s) => s.trim())
    .filter(Boolean)
  if (parts.length === 0) {
    return { perRound, tailOnly }
  }

  if (parts.length <= toolCount) {
    for (let i = 0; i < parts.length; i += 1) {
      perRound[i] = parts[i] ?? ''
    }
  } else {
    for (let i = 0; i < toolCount; i += 1) {
      perRound[i] = parts[i] ?? ''
    }
    tailOnly = parts.slice(toolCount).join('\n\n')
  }

  return { perRound, tailOnly }
}

export function isTurnPlaceholderNoOutputText(text: string): boolean {
  return text.trim() === TURN_PLACEHOLDER_NO_OUTPUT
}

export function hasRenderableTurnContent(
  turn: ConversationTurn,
  showExecutionRail: boolean,
  showThinkingProcess: boolean,
): boolean {
  if (showThinkingProcess && turn.thinking.trim()) {
    return true
  }

  if (turn.answer.trim() && !isTurnPlaceholderNoOutputText(turn.answer)) {
    return true
  }

  if (turn.responseSegments?.length) {
    return turn.responseSegments.some((segment) => {
      if (segment.type === 'text') {
        const t = segment.text.trim()
        return t.length > 0 && !isTurnPlaceholderNoOutputText(t)
      }
      return showExecutionRail && turn.toolCalls.some((toolCall) => toolCall.toolCallId === segment.toolCallId)
    })
  }

  return showExecutionRail && turn.toolCalls.length > 0
}

export function shouldRenderAssistantColumn(
  turn: ConversationTurn,
  showExecutionRail: boolean,
  showThinkingProcess: boolean,
  isActiveStreamingTurn: boolean,
): boolean {
  if (isActiveStreamingTurn && !hasRenderableTurnContent(turn, showExecutionRail, showThinkingProcess)) {
    return true
  }
  if (hasRenderableTurnContent(turn, showExecutionRail, showThinkingProcess)) {
    return true
  }
  if (showExecutionRail && turn.toolCalls.length > 0) {
    return true
  }
  if (isTurnPlaceholderNoOutputText(turn.answer)) {
    return false
  }
  if (!turn.answer.trim()) {
    if (turn.status === 'done' || turn.status === 'error') {
      return true
    }
    if (turn.toolCalls.length > 0) {
      return true
    }
    return false
  }
  return true
}

export function isTurnWaitingOnly(
  turn: ConversationTurn,
  isStreaming: boolean,
  showExecutionRail: boolean,
  showThinkingProcess: boolean,
): boolean {
  return isStreaming && !hasRenderableTurnContent(turn, showExecutionRail, showThinkingProcess)
}

export function ToolCallContentBlock({
  content,
  isStreaming,
  onImageClick,
}: {
  content: string
  isStreaming: boolean
  onImageClick?: (src: string, alt: string) => void
}) {
  const { contentWithoutAttachments, attachments } = useMemo(
    () => extractInlineMediaAttachments(content),
    [content],
  )
  const normalizedContent = useMemo(
    () => normalizeMarkdownImageSources(contentWithoutAttachments || content),
    [content, contentWithoutAttachments],
  )

  const handleClick = (event: MouseEvent<HTMLDivElement>) => {
    const target = event.target
    if (!(target instanceof HTMLElement)) {
      return
    }

    const anchor = target.closest('a[href]')
    if (anchor instanceof HTMLAnchorElement && anchor.href) {
      event.preventDefault()
      event.stopPropagation()
      void openExternalUrl(anchor.href)
      return
    }

    if (!onImageClick) {
      return
    }

    const image = target.closest('img')
    if (!(image instanceof HTMLImageElement) || !image.src) {
      return
    }

    event.preventDefault()
    onImageClick(image.src, image.alt)
  }

  return (
    <div className="tool-call-code tool-call-code-stream">
      {normalizedContent ? (
        <div className="markdown-content" onClick={handleClick}>
          <Suspense fallback={<MarkdownFallback content={normalizedContent || ' '} />}>
            <MarkdownRenderer content={normalizedContent || ' '} isStreaming={isStreaming} />
          </Suspense>
        </div>
      ) : null}
      {attachments.length > 0 ? <InlineMediaAttachmentList attachments={attachments} onImageClick={onImageClick} /> : null}
    </div>
  )
}

export function ToolRoundIoPanels({
  toolCall,
  onImageClick,
}: {
  toolCall: ToolCallEntry
  onImageClick?: (src: string, alt: string) => void
}) {
  const isStreaming = toolCall.state === 'running'
  const argsLive = formatToolCallText(toolCall.argsText, '无参数', false)
  const resultLive = formatToolCallText(toolCall.resultText, '暂无输出', false)
  const argsPretty = formatToolCallText(toolCall.argsText, '无参数')
  const resultPretty = formatToolCallText(toolCall.resultText, '暂无输出')
  const [copiedKey, setCopiedKey] = useState<'input' | 'output' | ''>('')

  const handleCopy = async (key: 'input' | 'output', text: string) => {
    await navigator.clipboard.writeText(text)
    setCopiedKey(key)
    window.setTimeout(() => setCopiedKey((current) => (current === key ? '' : current)), 1600)
  }

  return (
    <div className="tool-exec-io-stack">
      <div className="tool-io-panel tool-io-panel-input">
        <div className="tool-io-panel-head">
          <span className="tool-io-label">INPUT</span>
          <button
            type="button"
            className={`tool-io-copy-button ${copiedKey === 'input' ? 'copied' : ''}`}
            onClick={() => void handleCopy('input', argsPretty)}
            aria-label={copiedKey === 'input' ? '已复制' : '复制 INPUT'}
            title={copiedKey === 'input' ? '已复制' : '复制'}
          >
            {copiedKey === 'input' ? <Check size={14} /> : <Copy size={14} />}
          </button>
        </div>
        <div className="tool-io-panel-body">
          <ToolCallContentBlock
            content={isStreaming ? argsLive : argsPretty}
            isStreaming={isStreaming}
            onImageClick={onImageClick}
          />
        </div>
      </div>
      <div className="tool-io-panel tool-io-panel-output">
        <div className="tool-io-panel-head">
          <span className="tool-io-label">RESULT</span>
          <button
            type="button"
            className={`tool-io-copy-button ${copiedKey === 'output' ? 'copied' : ''}`}
            onClick={() => void handleCopy('output', resultPretty)}
            aria-label={copiedKey === 'output' ? '已复制' : '复制 RESULT'}
            title={copiedKey === 'output' ? '已复制' : '复制'}
          >
            {copiedKey === 'output' ? <Check size={14} /> : <Copy size={14} />}
          </button>
        </div>
        <div className="tool-io-panel-body">
          <ToolCallContentBlock
            content={isStreaming ? resultLive : resultPretty}
            isStreaming={isStreaming}
            onImageClick={onImageClick}
          />
        </div>
      </div>
    </div>
  )
}

export function TurnExecutionRail({
  toolCalls,
  thinking,
  showThinkingProcess,
  runningToolCount,
  onImageClick,
}: {
  toolCalls: ToolCallEntry[]
  thinking: string
  showThinkingProcess: boolean
  runningToolCount: number
  onImageClick?: (src: string, alt: string) => void
}) {
  const sorted = useMemo(() => [...toolCalls].sort((a, b) => a.createdAt - b.createdAt), [toolCalls])
  const n = sorted.length
  const thinkingRaw = showThinkingProcess ? thinking : ''
  const { perRound, tailOnly } = useMemo(
    () => computeExecThoughtSlices(thinkingRaw, n),
    [thinkingRaw, n],
  )
  const hasThinkingText = Boolean(thinkingRaw.trim())
  const thinkingRoundCount = n + (tailOnly.trim() ? 1 : 0)
  const groupState = getToolGroupState(sorted)
  const runningInSorted = sorted.filter((t) => t.state === 'running').length

  if (n === 0) {
    return null
  }

  const parallelRunning = runningInSorted > 1

  return (
    <LazyDetails
      className={`assistant-exec-rail tool-call-card ${groupState}${parallelRunning ? ' parallel-running' : ''}`}
      summaryClassName="assistant-exec-rail-summary"
      summary={
        <>
          <div className="assistant-exec-rail-summary-main">
            <span className="assistant-exec-rail-wrench" aria-hidden>
              <AppIcon name="wrench" size={15} />
            </span>
            <span className="assistant-exec-rail-title">
              {hasThinkingText ? `${n} 次工具调用 · 思考 ${thinkingRoundCount} 轮` : `${n} 次工具调用`}
            </span>
          </div>
          <span className="assistant-exec-rail-chevron" aria-hidden>
            <AppIcon name="chevron-down" size={16} />
          </span>
        </>
      }
    >
      <div className="assistant-exec-rail-body">
        <div className="assistant-exec-rounds">
          {sorted.map((tool, i) => {
            const thought = perRound[i]?.trim() ?? ''
            const isStreaming = tool.state === 'running'
            const isParallelRunning = isStreaming && runningToolCount > 1
            return (
              <LazyDetails
                key={tool.id}
                className={`tool-exec-round ${tool.state}`}
                summaryClassName="tool-exec-round-summary"
                summary={
                  <>
                    <span className="tool-exec-round-summary-text">
                      第 {i + 1} 轮
                      {hasThinkingText ? ' · 已思考' : ''}
                      {' · '}
                      <code>{tool.toolName}</code>
                    </span>
                    <span className="tool-exec-round-summary-meta">
                      {isStreaming ? (
                        <span className={`status-pill ${tool.state}`}>{getToolCallStateLabel(tool.state)}</span>
                      ) : null}
                      {isParallelRunning ? (
                        <span className="tool-parallel-pill">并行 {runningToolCount}</span>
                      ) : null}
                      <span className="tool-exec-round-chevron" aria-hidden>
                        <AppIcon name="chevron-down" size={14} />
                      </span>
                    </span>
                  </>
                }
              >
                {(open) =>
                  open ? (
                    <div className="tool-exec-round-body">
                      {thought ? <blockquote className="tool-exec-thought">{thought}</blockquote> : null}
                      <ToolRoundIoPanels toolCall={tool} onImageClick={onImageClick} />
                    </div>
                  ) : null
                }
              </LazyDetails>
            )
          })}
          {hasThinkingText && tailOnly.trim() ? (
            <LazyDetails
              className="tool-exec-round tool-exec-round-tail done"
              summaryClassName="tool-exec-round-summary"
              summary={
                <>
                  <span className="tool-exec-round-summary-text">第 {n + 1} 轮 · 已思考</span>
                  <span className="tool-exec-round-chevron" aria-hidden>
                    <AppIcon name="chevron-down" size={14} />
                  </span>
                </>
              }
            >
              {(open) =>
                open ? (
                  <div className="tool-exec-round-body">
                    <blockquote className="tool-exec-thought">{tailOnly}</blockquote>
                  </div>
                ) : null
              }
            </LazyDetails>
          ) : null}
        </div>
      </div>
    </LazyDetails>
  )
}

export function TurnExecutionDetails({ turn, isStreaming }: { turn: ConversationTurn; isStreaming: boolean }) {
  useLiveNow(isStreaming, 500)
  const totalDuration = getElapsedMs(turn.createdAt, turn.completedAt, isStreaming)

  if (typeof totalDuration !== 'number') {
    return null
  }

  return (
    <div className="assistant-message-toolbar">
      <div className="assistant-message-toolbar-meta">
        <div className="assistant-turn-meta answer-result-meta-inline">
          <span>已思考 {formatDurationLabel(totalDuration)}</span>
        </div>
      </div>
    </div>
  )
}

export function ImagePreviewModal({ alt, src, onClose }: { alt: string; src: string; onClose: () => void }) {
  return (
    <div className="image-preview-backdrop" onClick={onClose} role="presentation">
      <button type="button" className="image-preview-close" onClick={onClose} aria-label="关闭图片预览">
        <AppIcon name="close" size={18} />
      </button>
      <div className="image-preview-dialog" onClick={(event) => event.stopPropagation()} role="dialog" aria-modal="true" aria-label={alt || '图片预览'}>
        <img src={src} alt={alt} className="image-preview-image" />
        {alt ? <div className="image-preview-caption">{alt}</div> : null}
      </div>
    </div>
  )
}
