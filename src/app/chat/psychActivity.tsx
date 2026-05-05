/** 助手提示中使用此包裹心理活动独白，正文外显为对话。 */
export const NC_PSYCH_TAG_HINT =
  '心理活动段落请写在 <nc_psych>……</nc_psych> 或 【心理活动】……【/心理活动】 内（二选一）；勿与模型自带的「推理/思考」块混用。'

export type PsychActivityPart =
  | { kind: 'text'; body: string }
  | { kind: 'psych'; body: string }

const XML_OPEN = /<nc_psych\b[^>]*>/i
const XML_CLOSE_RE = /<\/nc_psych>/i
const CN_OPEN = '【心理活动】'
const CN_CLOSE = '【/心理活动】'

/**
 * 将助手正文拆成正文与「心理活动」段落，已从正文中移除包裹标记。
 */
export function partitionPsychActivityContent(raw: string): PsychActivityPart[] {
  let rest = raw
  const out: PsychActivityPart[] = []

  const pushText = (t: string) => {
    const b = t
    if (!b.trim()) return
    out.push({ kind: 'text', body: b })
  }

  while (rest.length) {
    const xmlMatch = rest.match(XML_OPEN)
    const cnIdx = rest.indexOf(CN_OPEN)

    let useXml = false
    let start = -1
    let openLen = 0

    const xmlIdx = xmlMatch?.index ?? -1
    if (xmlIdx >= 0 && (cnIdx < 0 || xmlIdx <= cnIdx)) {
      useXml = true
      start = xmlIdx
      openLen = xmlMatch![0]?.length ?? 0
    } else if (cnIdx >= 0) {
      start = cnIdx
      openLen = CN_OPEN.length
    } else {
      pushText(rest)
      break
    }

    pushText(rest.slice(0, start))

    const afterOpen = rest.slice(start + openLen)

    if (useXml) {
      const closeMatch = afterOpen.match(XML_CLOSE_RE)
      if (closeMatch?.index !== undefined && closeMatch[0]) {
        const inner = afterOpen.slice(0, closeMatch.index)
        out.push({ kind: 'psych', body: inner })
        rest = afterOpen.slice(closeMatch.index + closeMatch[0].length)
        continue
      }
      /** 流式未完：未到闭合标签则将剩余视作心理活动内容 */
      out.push({ kind: 'psych', body: afterOpen })
      break
    }

    const closeIdx = afterOpen.indexOf(CN_CLOSE)
    if (closeIdx >= 0) {
      const inner = afterOpen.slice(0, closeIdx)
      out.push({ kind: 'psych', body: inner })
      rest = afterOpen.slice(closeIdx + CN_CLOSE.length)
      continue
    }
    out.push({ kind: 'psych', body: afterOpen })
    break
  }

  return out
}

export function ThoughtCloudIcon({ size = 16 }: { size?: number }) {
  const stroke = 1.6
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
      className="turn-psych-icon-svg"
      width={size}
      height={size}
      fill="none"
      viewBox="0 0 24 24"
    >
      <path
        d="M14.5 4.75a6.25 6.25 0 00-11.53 4.43c0 .13.01.25.03.37A4.62 4.62 0 005.25 17.5h13.62a4 4 0 00.38-8 5.76 5.76 0 00-4.75-4.75Z"
        stroke="currentColor"
        strokeWidth={stroke}
        strokeLinejoin="round"
      />
      <circle cx="9" cy="12" r="1" fill="currentColor" opacity="0.72" />
      <circle cx="12" cy="10.5" r="0.9" fill="currentColor" opacity="0.72" />
      <circle cx="15" cy="12" r="1" fill="currentColor" opacity="0.72" />
    </svg>
  )
}

/** 单列心理活动片段（可多段堆叠）；正文为纯文本，避免内嵌 Markdown 误解析 */
export function TurnPsychActivityStrip({ body }: { body: string }) {
  const trimmed = body.trim()
  if (!trimmed) return null
  return (
    <div className="turn-psych-strip" role="note" aria-label="心理活动">
      <span className="turn-psych-icon-slot" aria-hidden>
        <ThoughtCloudIcon />
      </span>
      <p className="turn-psych-text">{trimmed}</p>
    </div>
  )
}
