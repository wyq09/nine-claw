export type ReplyCardTone = 'default' | 'tip' | 'warning'

export type ReplyCardItem = {
  title?: string
  body: string
  tone?: ReplyCardTone
}

/** 与 `src-tauri/src/channels/im_reply_format.rs` 对齐，供各 IM 渠道复用同一套分段规则。 */
const NINECLAW_CARDS_PATTERN = /```nineclaw-cards\s*([\s\S]*?)```/i

function hasUnclosedNineclawCardsFence(text: string): boolean {
  const open = /```\s*nineclaw-cards\s*/i.exec(text)
  if (!open) {
    return false
  }
  const rest = text.slice(open.index + open[0].length)
  return !/```/.test(rest)
}

export function parseNineclawCardsBlock(content: string): ReplyCardItem[] | null {
  const match = content.match(NINECLAW_CARDS_PATTERN)
  if (!match?.[1]) {
    return null
  }

  try {
    const parsed = JSON.parse(match[1].trim()) as { cards?: unknown }
    if (!parsed || typeof parsed !== 'object' || !Array.isArray(parsed.cards)) {
      return null
    }

    const cards: ReplyCardItem[] = []
    for (const entry of parsed.cards) {
      if (!entry || typeof entry !== 'object') {
        continue
      }
      const raw = entry as Record<string, unknown>
      const title = typeof raw.title === 'string' ? raw.title.trim() : ''
      const body = typeof raw.body === 'string' ? raw.body : ''
      const toneRaw = raw.tone
      const tone: ReplyCardTone =
        toneRaw === 'tip' || toneRaw === 'warning' ? toneRaw : 'default'
      if (!body.trim() && !title) {
        continue
      }
      cards.push({
        title: title || undefined,
        body: body.trim() || ' ',
        tone,
      })
    }
    return cards.length > 0 ? cards : null
  } catch {
    return null
  }
}

export function stripNineclawCardsBlock(content: string): string {
  return content.replace(NINECLAW_CARDS_PATTERN, '').trim()
}

export function splitContentByMarkdownH2(content: string): ReplyCardItem[] {
  const text = content.trim()
  if (!text) {
    return [{ body: ' ' }]
  }

  const lines = text.split('\n')
  const cards: ReplyCardItem[] = []
  let currentTitle: string | undefined
  const buf: string[] = []

  const flush = () => {
    const body = buf.join('\n').trim()
    buf.length = 0
    if (!currentTitle && !body) {
      return
    }
    cards.push({ title: currentTitle, body: body || ' ' })
    currentTitle = undefined
  }

  for (const line of lines) {
    const heading = /^## (.+)$/.exec(line)
    if (heading) {
      flush()
      currentTitle = heading[1].trim()
    } else {
      buf.push(line)
    }
  }
  flush()

  return cards.length > 0 ? cards : [{ body: text }]
}

/** 流式中单卡；完成后优先 nineclaw-cards JSON，否则按二级标题拆成多张卡。 */
export function resolveReplyCardItems(content: string, isStreaming: boolean): ReplyCardItem[] {
  const trimmed = content.trim() || ' '

  if (isStreaming || hasUnclosedNineclawCardsFence(content)) {
    return [{ body: trimmed }]
  }

  const fromJson = parseNineclawCardsBlock(content)
  if (fromJson) {
    return fromJson
  }

  const rest = stripNineclawCardsBlock(content)
  return splitContentByMarkdownH2(rest || content)
}
