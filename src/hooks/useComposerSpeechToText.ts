import { useCallback, useEffect, useMemo, useRef, useState, type RefObject } from 'react'

type SpeechRec = SpeechRecognition
type SpeechRecCtor = new () => SpeechRec

function getSpeechRecognitionCtor(): SpeechRecCtor | null {
  if (typeof window === 'undefined') {
    return null
  }
  const w = window as Window & {
    SpeechRecognition?: SpeechRecCtor
    webkitSpeechRecognition?: SpeechRecCtor
  }
  return w.SpeechRecognition ?? w.webkitSpeechRecognition ?? null
}

export function isBrowserSpeechToTextSupported(): boolean {
  return getSpeechRecognitionCtor() !== null
}

function insertAtCaret(el: HTMLTextAreaElement, text: string) {
  const start = el.selectionStart
  const end = el.selectionEnd
  const value = el.value
  const next = `${value.slice(0, start)}${text}${value.slice(end)}`
  el.value = next
  const caret = start + text.length
  try {
    el.setSelectionRange(caret, caret)
  } catch {
    /* selection 在部分状态下不可用 */
  }
  el.focus()
  el.dispatchEvent(new Event('input', { bubbles: true }))
}

function resolveSpeechLang(): string {
  const lang = navigator.language?.trim()
  if (lang) {
    return lang
  }
  return 'zh-CN'
}

function mapSpeechErrorMessage(code: string): string | null {
  switch (code) {
    case 'aborted':
      return null
    case 'not-allowed':
      return '未获得麦克风权限，请在系统设置 → 隐私与安全性 → 麦克风中允许 NineClaw。'
    case 'no-speech':
      return '未检测到语音，请靠近麦克风后重试。'
    case 'audio-capture':
      return '无法访问麦克风，请检查设备或权限。'
    case 'network':
      return '语音识别服务暂时不可用（网络）。'
    case 'service-not-allowed':
      return '当前环境不允许使用语音识别（请确认系统已开启听写/语音识别）。'
    default:
      return `语音识别出错：${code}`
  }
}

export type UseComposerSpeechToTextOptions = {
  textareaRef: RefObject<HTMLTextAreaElement | null>
  onError: (message: string) => void
}

export function useComposerSpeechToText({ textareaRef, onError }: UseComposerSpeechToTextOptions) {
  const [listening, setListening] = useState(false)
  const recognitionRef = useRef<SpeechRec | null>(null)
  const onErrorRef = useRef(onError)

  useEffect(() => {
    onErrorRef.current = onError
  }, [onError])

  const supported = useMemo(() => isBrowserSpeechToTextSupported(), [])

  const detachRecognition = useCallback((r: SpeechRec) => {
    r.onresult = null
    r.onerror = null
    r.onend = null
  }, [])

  const abortPrevious = useCallback(() => {
    const r = recognitionRef.current
    recognitionRef.current = null
    if (!r) {
      return
    }
    detachRecognition(r)
    try {
      r.abort()
    } catch {
      try {
        r.stop()
      } catch {
        /* ignore */
      }
    }
  }, [detachRecognition])

  const stopListening = useCallback(() => {
    const r = recognitionRef.current
    recognitionRef.current = null
    if (!r) {
      setListening(false)
      return
    }
    detachRecognition(r)
    try {
      r.stop()
    } catch {
      try {
        r.abort()
      } catch {
        /* ignore */
      }
    }
    setListening(false)
  }, [detachRecognition])

  const startListening = useCallback(() => {
    const Ctor = getSpeechRecognitionCtor()
    const el = textareaRef.current
    if (!Ctor || !el) {
      onErrorRef.current('无法访问输入框或未找到语音识别能力。')
      return
    }

    abortPrevious()

    const recognition = new Ctor()
    recognitionRef.current = recognition
    recognition.lang = resolveSpeechLang()
    recognition.continuous = false
    recognition.interimResults = false
    recognition.maxAlternatives = 1

    recognition.onresult = (event: SpeechRecognitionEvent) => {
      let chunk = ''
      for (let i = event.resultIndex; i < event.results.length; i += 1) {
        const row = event.results[i]
        if (row.isFinal) {
          chunk += row[0]?.transcript ?? ''
        }
      }
      const trimmed = chunk.trim()
      if (!trimmed) {
        return
      }
      const needsSpace =
        el.selectionStart > 0 &&
        el.value.length > 0 &&
        !/\s$/.test(el.value.slice(0, el.selectionStart)) &&
        !/^\s/.test(trimmed)
      insertAtCaret(el, `${needsSpace ? ' ' : ''}${trimmed}`)
    }

    recognition.onerror = (event: SpeechRecognitionErrorEvent) => {
      recognitionRef.current = null
      setListening(false)
      const msg = mapSpeechErrorMessage(event.error)
      if (msg) {
        onErrorRef.current(msg)
      }
    }

    recognition.onend = () => {
      recognitionRef.current = null
      setListening(false)
    }

    try {
      recognition.start()
      setListening(true)
    } catch {
      recognitionRef.current = null
      setListening(false)
      onErrorRef.current('无法启动语音识别，请检查麦克风权限或稍后重试。')
    }
  }, [abortPrevious, textareaRef])

  const toggle = useCallback(() => {
    if (listening) {
      stopListening()
      return
    }
    if (!supported) {
      onErrorRef.current('当前内核未暴露网页语音识别（部分环境不可用）。')
      return
    }
    startListening()
  }, [listening, startListening, stopListening, supported])

  useEffect(
    () => () => {
      abortPrevious()
      setListening(false)
    },
    [abortPrevious],
  )

  return { listening, toggle, supported }
}
