import { useEffect, useRef, useState } from 'react'

export type NumericDraftFieldProps = {
  id?: string
  /** 失焦或未聚焦时展示的权威数值 */
  value: number
  min: number
  max: number
  /** 仅有非空但无法解析为合法整数时（如乱码），失焦用此值回填并提交 */
  fallbackOnBlur: number
  onCommit: (next: number) => void
  'aria-label'?: string
  className?: string
  disabled?: boolean
}

/**
 * 整数输入：编辑中可为空或非数字草稿。
 * - 失焦时空串：**不**回填默认、**不** `onCommit`，保持空直至再次聚焦或与外部 `value` 同步。
 * - 失焦时有合法整数：clamp 并 `onCommit`。
 */
export function NumericDraftField({
  id,
  value,
  min,
  max,
  fallbackOnBlur,
  onCommit,
  'aria-label': ariaLabel,
  className,
  disabled,
}: NumericDraftFieldProps) {
  const [text, setText] = useState(() => String(value))
  const focusedRef = useRef(false)
  /** true：用户放空后刚失焦，在父组件 value 未变时不要从 props 盖住空框 */
  const preserveEmptyRef = useRef(false)
  /** 用户放空失焦那一瞬间父组件传入的 value，用来区分「仍是同一笔设置」与「外部已改」 */
  const valueWhenEmptiedRef = useRef<number | null>(null)

  useEffect(() => {
    if (focusedRef.current) {
      return
    }
    const stillSameStaleFrame =
      preserveEmptyRef.current &&
      valueWhenEmptiedRef.current !== null &&
      value === valueWhenEmptiedRef.current
    if (stillSameStaleFrame) {
      return
    }
    preserveEmptyRef.current = false
    valueWhenEmptiedRef.current = null
    setText(String(value))
  }, [value])

  const commitFromText = () => {
    const trimmed = text.trim()
    if (trimmed === '') {
      preserveEmptyRef.current = true
      valueWhenEmptiedRef.current = value
      setText('')
      return
    }
    preserveEmptyRef.current = false
    valueWhenEmptiedRef.current = null
    const parsed = Number.parseInt(trimmed, 10)
    if (!Number.isFinite(parsed)) {
      const next = fallbackOnBlur
      onCommit(next)
      setText(String(next))
      return
    }
    const clamped = Math.min(max, Math.max(min, parsed))
    onCommit(clamped)
    setText(String(clamped))
  }

  return (
    <input
      id={id}
      type="text"
      inputMode="numeric"
      autoComplete="off"
      aria-label={ariaLabel}
      className={className}
      disabled={disabled}
      value={text}
      onFocus={() => {
        focusedRef.current = true
        preserveEmptyRef.current = false
        valueWhenEmptiedRef.current = null
        setText(String(value))
      }}
      onChange={(event) => setText(event.target.value)}
      onBlur={() => {
        focusedRef.current = false
        commitFromText()
      }}
    />
  )
}
