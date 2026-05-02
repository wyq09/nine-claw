export function createMemoryDeleteParameters(Type) {
  return Type.Object({
    memory_id: Type.String({
      minLength: 1,
      description: "ID of the memory entry to delete.",
    }),
  });
}

export function createMemoryDeleteTool(deps) {
  return {
    name: "memory_delete",
    label: "Memory Delete",
    description:
      "Delete a specific memory entry by its ID. " +
      "Use it to remove outdated or incorrect memories from the vector store.",
    promptSnippet:
      "Delete a memory entry by its unique ID.",
    promptGuidelines: [
      "Use memory_delete to remove memories that are outdated, incorrect, or no longer relevant.",
      "Deletion is permanent -- consider whether the memory should be updated instead.",
      "Always confirm the memory_id before deleting to avoid accidental removal.",
    ],
    parameters: createMemoryDeleteParameters(deps.Type),
    async execute(_toolCallId, input, _signal, _onUpdate, ctx) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim();
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim();
      if (!proxyBase || !token) {
        return { content: [{ type: "text", text: "Error: proxy not configured" }] };
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/delete`, {
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
