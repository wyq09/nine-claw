import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

export function createMemoryListParameters(Type) {
  return Type.Object({
    limit: Type.Optional(
      Type.Integer({
        minimum: 1,
        maximum: 500,
        description: 'Maximum number of K/V entries to return. Default is 100.',
      }),
    ),
  })
}

export function createMemoryListTool(deps) {
  return {
    name: 'memory_list',
    label: 'Memory List',
    description: 'List K/V memory entries in the current workspace.',
    promptSnippet: 'List K/V memory entries.',
    promptGuidelines: [
      'Use memory_list to inspect available K/V memory keys before calling memory_get.',
    ],
    parameters: createMemoryListParameters(deps.Type),
    async execute(_toolCallId, input) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim()
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim()
      if (!proxyBase || !token) {
        return { content: [{ type: 'text', text: 'Error: proxy not configured' }] }
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/list`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          limit: input.limit ?? 100,
        }),
      })
      const { data } = await parseProxyJsonResponse(resp)
      return { content: [{ type: 'text', text: JSON.stringify(data, null, 2) }], details: data }
    },
  }
}
