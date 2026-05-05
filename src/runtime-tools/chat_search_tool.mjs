import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

export function createChatSearchParameters(Type) {
  return Type.Object({
    query: Type.String({
      minLength: 1,
      description: 'Natural language query to search historical chat turns by semantic similarity.',
    }),
    limit: Type.Optional(
      Type.Integer({
        minimum: 1,
        maximum: 100,
        description: 'Maximum number of matches to return. Default is 20.',
      }),
    ),
    time_range_start: Type.Optional(
      Type.Integer({
        description: 'Start of time range filter in milliseconds since epoch. Use to limit results to after this time.',
      }),
    ),
    time_range_end: Type.Optional(
      Type.Integer({
        description: 'End of time range filter in milliseconds since epoch. Use to limit results to before this time.',
      }),
    ),
  })
}

export function createChatSearchTool(deps) {
  return {
    name: 'chat_search',
    label: 'Chat Search',
    description:
      'Search historical chat turns across conversations using semantic similarity. ' +
      'Supports natural language queries like "上次聊买房的事" or "之前讨论的Rust项目". ' +
      'Returns matching turns with session info, timestamps, and relevance scores.',
    promptSnippet: 'Search historical chat messages by natural language query.',
    promptGuidelines: [
      'Use chat_search when you need to find earlier user or assistant messages across conversations.',
      'Queries are semantic — describe what you are looking for naturally, not just keywords.',
      'Use time_range_start/time_range_end to narrow results to a specific time period.',
      'Results include a relevance score; higher scores mean better matches.',
    ],
    parameters: createChatSearchParameters(deps.Type),
    async execute(_toolCallId, input) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim()
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim()
      if (!proxyBase || !token) {
        return { content: [{ type: 'text', text: 'Error: proxy not configured' }] }
      }
      const body = {
        query: input.query,
        limit: input.limit ?? 20,
      }
      if (input.time_range_start != null) body.time_range_start = input.time_range_start
      if (input.time_range_end != null) body.time_range_end = input.time_range_end
      const resp = await fetch(`${proxyBase}/chat/${token}/search`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      })
      const { data } = await parseProxyJsonResponse(resp)
      return { content: [{ type: 'text', text: JSON.stringify(data, null, 2) }], details: data }
    },
  }
}
