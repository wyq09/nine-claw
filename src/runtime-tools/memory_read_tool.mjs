export function createMemoryReadParameters(Type) {
  return Type.Object({
    memory_id: Type.String({
      minLength: 1,
      description: "ID of the memory entry to read.",
    }),
  });
}

export function createMemoryReadTool(deps) {
  return {
    name: "memory_read",
    label: "Memory Read",
    description:
      "Read a specific memory entry by its ID. " +
      "Use it when you know the exact memory ID and want to retrieve its full content.",
    promptSnippet:
      "Read a memory entry by its unique ID.",
    promptGuidelines: [
      "Use memory_read when you already have a memory_id from a previous search or update result.",
      "The tool returns the full content, title, tags, and metadata of the memory entry.",
    ],
    parameters: createMemoryReadParameters(deps.Type),
    async execute(_toolCallId, input, _signal, _onUpdate, ctx) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim();
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim();
      if (!proxyBase || !token) {
        return { content: [{ type: "text", text: "Error: proxy not configured" }] };
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/read`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          memory_id: input.memory_id,
          workspace_id: ctx?.workspace_id || null,
        }),
      });
      const data = await resp.json();
      return { content: [{ type: "text", text: JSON.stringify(data, null, 2) }] };
    },
  };
}
