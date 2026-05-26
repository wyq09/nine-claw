import { useEffect, useState } from 'react'
import { SessionLlmLogPanel } from './SessionLlmLogPanel'

export function SessionLlmLogStandaloneApp() {
  const [workspaceId, setWorkspaceId] = useState<string>('')
  const [sessionId, setSessionId] = useState<string | null>(null)

  useEffect(() => {
    const parse = () => {
      const hash = window.location.hash || ''
      const qIdx = hash.indexOf('?')
      const search = qIdx >= 0 ? hash.slice(qIdx + 1) : ''
      const params = new URLSearchParams(search)
      setWorkspaceId(params.get('ws') || '')
      const s = params.get('ses')
      setSessionId(s && s.length > 0 ? s : null)
    }
    parse()
    window.addEventListener('hashchange', parse)
    return () => window.removeEventListener('hashchange', parse)
  }, [])

  return (
    <SessionLlmLogPanel
      workspaceId={workspaceId || null}
      sessionId={sessionId}
      open
      onClose={() => {}}
      standalone
    />
  )
}

export default SessionLlmLogStandaloneApp
