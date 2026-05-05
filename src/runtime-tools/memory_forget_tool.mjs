import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

export function createMemoryForgetParameters(Type) {
  return Type.Object({
    key: Type.String({
      minLength: 1,
      description: 'Key to delete from K/V memory.',
    }),
  })
}

export function createMemoryForgetTool(deps) {
  return {
    name: 'memory_forget',
    label: 'Memory Forget',
    description: 'Delete a K/V memory entry by key.',
    promptSnippet: 'Delete a K/V memory value by key.',
    promptGuidelines: [
      'Use memory_forget to remove stale or incorrect K/V memory entries.',
      'Deletion is permanent.',
    ],
    parameters: createMemoryForgetParameters(deps.Type),
    async execute(_toolCallId, input) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim()
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim()
      if (!proxyBase || !token) {
        return { content: [{ type: 'text', text: 'Error: proxy not configured' }] }
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/forget`, {
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
