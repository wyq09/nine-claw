import { StrictMode, useEffect } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { ToastProvider } from './components/ToastProvider'
import { LlmTraceStandaloneApp } from './app/workspaces/panels/LlmTraceStandaloneApp'
import { SessionLlmLogStandaloneApp } from './app/workspaces/panels/SessionLlmLogStandaloneApp'

const hash = window.location.hash || ''
const isTraceStandalone = hash.startsWith('#/llm-trace')
const isSessionLogStandalone = hash.startsWith('#/session-llm-log')

function BootReadyMarker() {
  useEffect(() => {
    document.body.classList.add('boot-ready')
    const splash = document.getElementById('boot-splash')
    const timer = window.setTimeout(() => {
      splash?.remove()
    }, 220)

    return () => {
      window.clearTimeout(timer)
    }
  }, [])

  return null
}

createRoot(document.getElementById('root')!, {
  onRecoverableError(error, errorInfo) {
    if (import.meta.env.DEV) {
      console.error('[NineClaw recoverable render error]', error, errorInfo.componentStack)
    }
  },
}).render(
  <StrictMode>
    <BootReadyMarker />
    <ToastProvider>
      {isTraceStandalone ? <LlmTraceStandaloneApp /> : isSessionLogStandalone ? <SessionLlmLogStandaloneApp /> : <App />}
    </ToastProvider>
  </StrictMode>,
)
