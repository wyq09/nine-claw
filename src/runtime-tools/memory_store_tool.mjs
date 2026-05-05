import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

function createMemoryValueSchema(Type) {
  if (typeof Type.Any === 'function') {
    return Type.Any({ description: 'Structured JSON value to store for the key.' })
  }
  return Type.String({ description: 'Structured JSON value to store for the key.' })
}

export function createMemoryStoreParameters(Type) {
  return Type.Object({
    key: Type.String({
      minLength: 1,
      description: 'Unique key for the memory entry.',
    }),
    value: createMemoryValueSchema(Type),
  })
}

export function createMemoryStoreTool(deps) {
  return {
    name: 'memory_store',
    label: 'Memory Store',
    description: 'Store structured data in the workspace K/V memory store.',
    promptSnippet: 'Store a structured value under a key in K/V memory.',
    promptGuidelines: [
      'Use memory_store for stable structured facts you want to retrieve by key later.',
      'Keys should be concise and deterministic.',
      'Values can be plain strings or structured JSON objects.',
      'For categorized user profile facts, prefer keys nc_um.identity/<slug>, nc_um.work/<slug>, nc_um.writing/<slug>, or nc_um.directive/<slug> so they appear grouped in Settings → User Memory.',
    ],
    parameters: createMemoryStoreParameters(deps.Type),
    async execute(_toolCallId, input) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim()
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim()
      if (!proxyBase || !token) {
        return { content: [{ type: 'text', text: 'Error: proxy not configured' }] }
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/store`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          key: input.key,
          value: input.value,
        }),
      })
      const { data } = await parseProxyJsonResponse(resp)
      return { content: [{ type: 'text', text: JSON.stringify(data, null, 2) }], details: data }
    },
  }
}
