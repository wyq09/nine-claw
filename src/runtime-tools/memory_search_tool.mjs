import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

export function createMemorySearchParameters(Type) {
  return Type.Object({
    query: Type.String({
      minLength: 1,
      description: "Search query to find relevant memories using semantic similarity.",
    }),
    limit: Type.Optional(
      Type.Integer({
        minimum: 1,
        maximum: 50,
        description: "Maximum number of results to return. Default is 10.",
      }),
    ),
    threshold: Type.Optional(
      Type.Number({
        minimum: 0,
        maximum: 1,
        description: "Minimum similarity score threshold (0-1). Results below this threshold are excluded.",
      }),
    ),
    tags: Type.Optional(
      Type.Array(Type.String(), {
        description: "Optional tags to filter results. Only memories matching these tags will be returned.",
      }),
    ),
  });
}

export function createMemorySearchTool(deps) {
  return {
    name: "memory_search",
    label: "Memory Search",
    description:
      "Search the vector memory store using semantic similarity. " +
      "Use it to retrieve previously stored facts, decisions, or context.",
    promptSnippet:
      "Search memories by query with optional tag filtering and similarity threshold.",
    promptGuidelines: [
      "Use memory_search to find previously stored information relevant to the current task.",
      "Provide a descriptive query that captures what you are looking for.",
      "Use tags to narrow results to a specific category or domain.",
      "Adjust threshold to control result precision: higher values return fewer but more relevant results.",
    ],
    parameters: createMemorySearchParameters(deps.Type),
    async execute(_toolCallId, input, _signal, _onUpdate, ctx) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim();
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim();
      if (!proxyBase || !token) {
        return { content: [{ type: "text", text: "Error: proxy not configured" }] };
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/search`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          query: input.query,
          limit: input.limit ?? 10,
          threshold: input.threshold ?? null,
          tags: input.tags ?? [],
          workspace_id: ctx?.workspace_id || null,
        }),
      });
      const { data } = await parseProxyJsonResponse(resp);
      return { content: [{ type: "text", text: JSON.stringify(data, null, 2) }], details: data };
    },
  };
}
