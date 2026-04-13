import { createContext } from 'react'

export type ToastApi = {
  success: (message: string, durationMs?: number) => void
  error: (message: string, durationMs?: number) => void
}

export const ToastContext = createContext<ToastApi | null>(null)
