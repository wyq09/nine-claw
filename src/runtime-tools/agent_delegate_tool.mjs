export function createAgentDelegateParameters(Type) {
  return Type.Object({
    role: Type.String({
      minLength: 1,
      description:
        "Exact sub-agent identifier: use the real agent id, or the exact display name, of a delegate that your session allowlist already lists (system prompt / team members / collaboration allowlist). Do not invent job titles or generic roles (no 'analyst', 'writer', 'researcher', etc. unless that is literally a configured agent's name).",
    }),
    task: Type.String({
      minLength: 1,
      description:
        "A clear, specific description of what the sub-agent should accomplish. Include all necessary context, requirements, and expected output format.",
    }),
    context: Type.Optional(
      Type.String({
        description:
          "Additional context to pass to the sub-agent, such as relevant background information, constraints, or reference data.",
      }),
    ),
  });
}

const DEFAULT_DELEGATE_RETRY_DELAYS_MS = [200, 700];

function isAbortError(err, signal) {
  return err?.name === "AbortError" || signal?.aborted;
}

export function formatDelegateFetchError(err) {
  const parts = [];
  const message = err?.message || String(err);
  if (message) parts.push(message);

  const cause = err?.cause;
  if (cause) {
    const causeParts = [];
    if (cause.code) causeParts.push(cause.code);
    if (cause.message) causeParts.push(cause.message);
    if (cause.address || cause.port) {
      causeParts.push([cause.address, cause.port].filter(Boolean).join(":"));
    }
    if (causeParts.length > 0) {
      parts.push(`cause: ${causeParts.join(" | ")}`);
    }
  }

  return [...new Set(parts)].join("; ") || "unknown fetch error";
}

async function sleep(ms, signal) {
  if (ms <= 0) return;
  await new Promise((resolve, reject) => {
    const timer = setTimeout(resolve, ms);
    if (signal) {
      const abort = () => {
        clearTimeout(timer);
        reject(Object.assign(new Error("Aborted"), { name: "AbortError" }));
      };
      signal.addEventListener("abort", abort, { once: true });
    }
  });
}

export async function fetchDelegateWithRetry(fetchImpl, url, options, retryDelaysMs = DEFAULT_DELEGATE_RETRY_DELAYS_MS) {
  let lastError;
  for (let attempt = 0; attempt <= retryDelaysMs.length; attempt += 1) {
    try {
      return await fetchImpl(url, options);
    } catch (err) {
      if (isAbortError(err, options?.signal)) {
        throw err;
      }
      lastError = err;
      if (attempt >= retryDelaysMs.length) {
        break;
      }
      await sleep(retryDelaysMs[attempt], options?.signal);
    }
  }
  throw lastError;
}

export function createAgentDelegateTool(deps) {
  return {
    name: "agent_delegate",
    label: "Agent Delegate",
    description:
      "Delegate a task to another agent that is already allowed for this session (team member or explicit allowlist). " +
      "The `role` field must match a real agent id or exact name from that allowlist — never invent generic role names. " +
      "The sub-agent runs with its own system prompt and tools and returns a result.",
    promptSnippet:
      "Delegate by passing the real allowed sub-agent id or name plus a self-contained task.",
    promptGuidelines: [
      "Only reference sub-agents that appear in the current allowlist; if unsure, list members from context or tools first.",
      "Provide clear, self-contained task descriptions — the sub-agent won't see your conversation history.",
      "Include all necessary context in the task or context field so the sub-agent can work independently.",
      "You can delegate to the same sub-agent multiple times if needed.",
    ],
    parameters: createAgentDelegateParameters(deps.Type),
    async execute(_toolCallId, input, signal, _onUpdate, _ctx) {
      const { role, task, context } = input || {};
      if (!role || !role.trim()) {
        return {
          content: [{ type: "text", text: "Error: 'role' is required — provide the sub-agent name or ID." }],
          details: { ok: false, reason: "missing_role" },
        };
      }
      if (!task || !task.trim()) {
        return {
          content: [{ type: "text", text: "Error: 'task' is required — describe what the sub-agent should do." }],
          details: { ok: false, reason: "missing_task" },
        };
      }

      const baseUrl = deps.processApi?.env?.NINECLAW_PROXY_BASE_URL?.trim() || process.env.NINECLAW_PROXY_BASE_URL?.trim();
      const token = deps.processApi?.env?.NINECLAW_PROXY_SESSION_TOKEN?.trim() || process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim();
      if (!baseUrl || !token) {
        return {
          content: [{ type: "text", text: "Agent delegation is not available — proxy not configured." }],
          details: { ok: false, reason: "no_proxy" },
        };
      }

      const url = `${baseUrl}/delegate/${token}/dispatch`;
      const body = { role: role.trim(), task: task.trim(), context: context?.trim() || "" };

      try {
        const response = await fetchDelegateWithRetry(
          deps.fetchImpl,
          url,
          {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(body),
            signal,
          },
          deps.retryDelaysMs,
        );

        if (!response.ok) {
          const errorText = await response.text().catch(() => "");
          return {
            content: [{ type: "text", text: `Delegation failed (HTTP ${response.status}): ${errorText}` }],
            details: { ok: false, reason: "http_error", status: response.status },
          };
        }

        const result = await response.json();
        if (!result.ok) {
          return {
            content: [{ type: "text", text: result.error || "Delegation failed with unknown error." }],
            details: { ok: false, reason: "delegate_error", error: result.error },
          };
        }

        const meta = [];
        if (result.agentName) meta.push(`Agent: ${result.agentName}`);
        if (result.durationMs) meta.push(`Time: ${(result.durationMs / 1000).toFixed(1)}s`);

        const header = meta.length > 0 ? `[${meta.join(" | ")}]\n\n` : "";
        return {
          content: [{ type: "text", text: header + (result.output || "(empty response)") }],
          details: {
            ok: true,
            agentId: result.agentId,
            agentName: result.agentName,
            durationMs: result.durationMs,
          },
        };
      } catch (err) {
        if (err.name === "AbortError" || signal?.aborted) {
          return {
            content: [{ type: "text", text: "Delegation was cancelled." }],
            details: { ok: false, reason: "cancelled" },
          };
        }
        return {
          content: [{ type: "text", text: `Delegation error: ${formatDelegateFetchError(err)}` }],
          details: { ok: false, reason: "fetch_error", error: formatDelegateFetchError(err) },
        };
      }
    },
  };
}
