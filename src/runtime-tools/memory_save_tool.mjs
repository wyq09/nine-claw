import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

export function createMemorySaveParameters(Type) {
  return Type.Object({
    text: Type.String({
      minLength: 1,
      description: 'Content to save to the vector memory store. Will be searchable by semantic similarity.',
    }),
    tags: Type.Optional(
      Type.Array(Type.String(), {
        description: 'Optional tags for categorizing and filtering the saved content.',
      }),
    ),
    metadata: Type.Optional(
      Type.Record(Type.String(), Type.String(), {
        description: 'Optional custom metadata key-value pairs to attach to this memory entry.',
      }),
    ),
  })
}

export function createMemorySaveTool(deps) {
  return {
    name: 'memory_save',
    label: 'Memory Save',
    description:
      'Save content to the vector memory store for later semantic retrieval. ' +
      'Use it to store observations, inferences, user preferences, or any soft information ' +
      'that benefits from semantic search recall. Saved content is immediately searchable via memory_search.',
    promptSnippet: 'Save text to vector memory with optional tags and metadata.',
    promptGuidelines: [
      'Use memory_save for observations and inferences that need semantic retrieval later.',
      'Good for: user mood observations, interest inferences, relationship judgments, contextual notes.',
      'NOT for structured facts with a key — use memory_store (K/V) for those.',
      'Tags help narrow searches — use them for categories like "preference", "observation", "context".',
      'Saved memories are immediately searchable via memory_search.',
    ],
    parameters: createMemorySaveParameters(deps.Type),
    async execute(_toolCallId, input) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim()
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim()
      if (!proxyBase || !token) {
        return { content: [{ type: 'text', text: 'Error: proxy not configured' }] }
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/save`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          text: input.text,
          tags: input.tags ?? [],
          metadata: input.metadata ?? {},
        }),
      })
      const { data } = await parseProxyJsonResponse(resp)
      return { content: [{ type: 'text', text: JSON.stringify(data, null, 2) }], details: data }
    },
  }
}
