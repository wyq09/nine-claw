import { lazy, Suspense, useMemo } from 'react'
import type { MouseEvent } from 'react'
import { AgentAvatar } from '../../components/AgentAvatar'
import { normalizeMarkdownImageSources } from '../../lib/inlineMedia'
import { openExternalUrl } from '../../lib/piClient'

const MarkdownRenderer = lazy(() => import('../../components/MarkdownRenderer'))

export type ParsedDelegateToolPayload = {
  role: string
  task: string
}

export type ParsedDelegateToolResult = {
  agentName: string
  durationLabel: string
  body: string
}

export function parseDelegateToolPayload(argsText: string): ParsedDelegateToolPayload | null {
  const trimmed = argsText.trim()
  if (!trimmed) return null
  try {
    const parsed = JSON.parse(trimmed) as Record<string, unknown>
    const role = typeof parsed.role === 'string' ? parsed.role.trim() : ''
    const task = typeof parsed.task === 'string' ? parsed.task.trim() : ''
    if (!role && !task) return null
    return { role, task }
  } catch {
    return null
  }
}

export function parseDelegateToolResult(resultText: string): ParsedDelegateToolResult | null {
  const trimmed = resultText.trim()
  if (!trimmed) return null
  const match = trimmed.match(/^\[Agent:\s*(.+?)\s*\|\s*Time:\s*([^\]]+)\]\s*([\s\S]*)$/)
  if (!match) return null
  const [, agentNameRaw, durationLabelRaw, bodyRaw] = match
  const agentName = agentNameRaw.trim()
  const durationLabel = durationLabelRaw.trim()
  const body = bodyRaw.trim()
  if (!agentName || !body) return null
  return { agentName, durationLabel, body }
}

export function isDelegateToolName(toolName: string): boolean {
  const normalized = toolName.trim().toLowerCase()
  return normalized === 'agent_delegate' || normalized === 'agent_spawn'
}

export function DelegateToolResultCard({
  agentName,
  durationLabel,
  task,
  body,
  open = true,
  onToggleOpen,
  onImageClick,
}: {
  agentName: string
  durationLabel: string
  task: string
  body: string
  open?: boolean
  onToggleOpen?: () => void
  onImageClick?: (src: string, alt: string) => void
}) {
  const normalizedBody = useMemo(() => normalizeMarkdownImageSources(body), [body])

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
    <article className="delegate-tool-result-card">
      <header className="delegate-tool-result-head">
        <div className="delegate-tool-result-identity">
          <div className="delegate-tool-result-avatar-wrap">
            <AgentAvatar name={agentName} className="delegate-tool-result-avatar" size={18} />
            <span className="delegate-tool-result-online-dot" aria-hidden />
          </div>
          <div className="delegate-tool-result-meta">
            <div className="delegate-tool-result-name-row">
              <strong className="delegate-tool-result-name">{agentName}</strong>
              <span className="delegate-tool-result-status">已完成</span>
            </div>
            <div className="delegate-tool-result-subline">
              {durationLabel ? <span>{durationLabel}</span> : null}
              {task ? <span className="delegate-tool-result-subline-dot">•</span> : null}
              {task ? <span className="delegate-tool-result-task-inline">{task}</span> : null}
            </div>
          </div>
        </div>
        <button
          type="button"
          className={`delegate-tool-result-link${open ? ' is-open' : ''}`}
          onClick={onToggleOpen}
          aria-expanded={open}
          aria-label={open ? '折叠结果' : '展开结果'}
        >
          结果 &gt;
        </button>
      </header>
      {open ? (
        <div className="delegate-tool-result-body theme-markdown-surface" onClick={handleClick}>
          <div className="markdown-content">
            <Suspense fallback={<div className="markdown-content-fallback">{normalizedBody || ' '}</div>}>
              <MarkdownRenderer content={normalizedBody || ' '} isStreaming={false} />
            </Suspense>
          </div>
        </div>
      ) : null}
    </article>
  )
}
