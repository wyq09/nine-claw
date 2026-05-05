import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

export function createMemoryGetParameters(Type) {
  return Type.Object({
    key: Type.String({
      minLength: 1,
      description: 'Key to retrieve from K/V memory.',
    }),
  })
}

export function createMemoryGetTool(deps) {
  return {
    name: 'memory_get',
    label: 'Memory Get',
    description: 'Read a structured K/V memory entry by key.',
    promptSnippet: 'Read a K/V memory value by key.',
    promptGuidelines: [
      'Use memory_get when you know the exact key.',
    ],
    parameters: createMemoryGetParameters(deps.Type),
    async execute(_toolCallId, input) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim()
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim()
      if (!proxyBase || !token) {
        return { content: [{ type: 'text', text: 'Error: proxy not configured' }] }
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/get`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          key: input.key,
        }),
      })
      const { data } = await parseProxyJsonResponse(resp)
      return { content: [{ type: 'text', text: JSON.stringify(data, null, 2) }], details: data }
    },
  }
}
