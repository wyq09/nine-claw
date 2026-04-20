import { useEffect, useState } from 'react'
import { LlmTracePanel } from './LlmTracePanel'

/** 独立窗口入口：读取 URL `?ws=<workspaceId>&ses=<sessionId>`，全屏渲染 LlmTracePanel。 */
export function LlmTraceStandaloneApp() {
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

  if (!workspaceId) {
    return (
      <div style={{ padding: 32, color: '#9a9aa5', fontFamily: 'system-ui' }}>
        缺少 <code>ws</code> 参数。示例：<code>#/llm-trace?ws=&lt;workspace_id&gt;</code>
      </div>
    )
  }

  return (
    <LlmTracePanel
      workspaceId={workspaceId}
      sessionId={sessionId}
      open
      onClose={() => {}}
      standalone
    />
  )
}

export default LlmTraceStandaloneApp
