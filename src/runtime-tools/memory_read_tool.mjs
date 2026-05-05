import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

export function createMemoryReadParameters(Type) {
  return Type.Object({})
}

export function createMemoryReadTool(deps) {
  return {
    name: 'memory_read',
    label: 'Memory Read',
    description:
      "Read the current agent's MEMORY.md file.",
    promptSnippet:
      "Read the current agent's MEMORY.md file.",
    promptGuidelines: [
      'Use memory_read to inspect the explicit user memory markdown file for the current agent.',
      'Use this before memory_update when you need to preserve or refine existing memory content.',
    ],
    parameters: createMemoryReadParameters(deps.Type),
    async execute() {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim()
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim()
      if (!proxyBase || !token) {
        return { content: [{ type: 'text', text: 'Error: proxy not configured' }] }
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/read`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({}),
      })
      const { data } = await parseProxyJsonResponse(resp)
      return { content: [{ type: 'text', text: JSON.stringify(data, null, 2) }], details: data }
    },
  }
}
