import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

export function createMemoryUpdateParameters(Type) {
  return Type.Object({
    content: Type.String({
      minLength: 1,
      description: "Full content to write into the current agent's MEMORY.md file.",
    }),
    mode: Type.Optional(
      Type.Union([
        Type.Literal('replace'),
        Type.Literal('append'),
      ], {
        description: "replace overwrites the file, append adds content to the end. Default is replace.",
      }),
    ),
  })
}

export function createMemoryUpdateTool(deps) {
  return {
    name: 'memory_update',
    label: 'Memory Update',
    description:
      "Update the current agent's MEMORY.md file. Use it to maintain the agent's explicit user memory document.",
    promptSnippet:
      "Write or append content to the current agent's MEMORY.md file.",
    promptGuidelines: [
      'Use memory_update when the task explicitly needs to update the agent memory markdown file.',
      'Prefer memory_read first if you need to inspect the current file before rewriting it.',
      'Use mode=append only for small additive updates; use replace when supplying the full desired content.',
    ],
    parameters: createMemoryUpdateParameters(deps.Type),
    async execute(_toolCallId, input) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim()
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim()
      if (!proxyBase || !token) {
        return { content: [{ type: 'text', text: 'Error: proxy not configured' }] }
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/update`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          content: input.content,
          mode: input.mode ?? 'replace',
        }),
      })
      const { data } = await parseProxyJsonResponse(resp)
      return { content: [{ type: 'text', text: JSON.stringify(data, null, 2) }], details: data }
    },
  }
}
