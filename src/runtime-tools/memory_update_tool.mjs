export function createMemoryUpdateParameters(Type) {
  return Type.Object({
    title: Type.String({
      minLength: 1,
      description: "Title of the memory entry.",
    }),
    content: Type.String({
      minLength: 1,
      description: "Content/body of the memory entry.",
    }),
    tags: Type.Optional(
      Type.Array(Type.String(), {
        description: "Optional tags for categorizing the memory.",
      }),
    ),
    memory_id: Type.Optional(
      Type.String({
        description: "Optional existing memory ID to update. If omitted, a new memory is created.",
      }),
    ),
  });
}

export function createMemoryUpdateTool(deps) {
  return {
    name: "memory_update",
    label: "Memory Update",
    description:
      "Create or update a memory entry in the vector memory store. " +
      "Use it to persist important facts, decisions, or context for later retrieval.",
    promptSnippet:
      "Create or update a memory entry with a title, content, and optional tags.",
    promptGuidelines: [
      "Use memory_update to store important facts, decisions, or context that should persist across sessions.",
      "Provide a concise title and detailed content for the memory.",
      "Use tags to categorize memories for easier filtering during search.",
      "Pass memory_id only when updating an existing memory entry.",
    ],
    parameters: createMemoryUpdateParameters(deps.Type),
    async execute(_toolCallId, input, _signal, _onUpdate, ctx) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim();
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim();
      if (!proxyBase || !token) {
        return { content: [{ type: "text", text: "Error: proxy not configured" }] };
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/update`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          title: input.title,
          content: input.content,
          tags: input.tags ?? [],
          memory_id: input.memory_id ?? null,
          workspace_id: ctx?.workspace_id || null,
        }),
      });
      const data = await resp.json();
      return { content: [{ type: "text", text: JSON.stringify(data, null, 2) }] };
    },
  };
}
