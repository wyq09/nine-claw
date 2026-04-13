import { useCallback, useMemo, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { ToastContext, type ToastApi } from '../toast/toastContext'

type ToastVariant = 'success' | 'error'

type ToastEntry = { id: string; variant: ToastVariant; message: string }

const DEFAULT_DURATION_MS = 5500

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastEntry[]>([])

  const dismiss = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id))
  }, [])

  const push = useCallback(
    (variant: ToastVariant, message: string, durationMs: number) => {
      const id = crypto.randomUUID()
      setToasts((prev) => [...prev, { id, variant, message }])
      window.setTimeout(() => dismiss(id), durationMs)
    },
    [dismiss],
  )

  const api = useMemo<ToastApi>(
    () => ({
      success: (message, durationMs = DEFAULT_DURATION_MS) => push('success', message, durationMs),
      error: (message, durationMs = DEFAULT_DURATION_MS) => push('error', message, durationMs),
    }),
    [push],
  )

  const portal =
    typeof document !== 'undefined'
      ? createPortal(
          <div className="toast-host" aria-live="polite" aria-relevant="additions text">
            {toasts.map((t) => (
              <div key={t.id} className={`toast-item toast-item--${t.variant}`} role="status">
                {t.variant === 'success' ? (
                  <>
                    <span className="toast-item-title">操作成功</span>
                    <span className="toast-item-body">{t.message}</span>
                  </>
                ) : (
                  <>
                    <span className="toast-item-title">操作失败</span>
                    <span className="toast-item-body">{t.message}</span>
                  </>
                )}
              </div>
            ))}
          </div>,
          document.body,
        )
      : null

  return (
    <ToastContext.Provider value={api}>
      {children}
      {portal}
    </ToastContext.Provider>
  )
}
