import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

export function createQueryScheduledTaskInfoParameters(Type) {
  return Type.Object({
    task_id: Type.String({
      minLength: 1,
      description: 'The ID of the scheduled task to query.',
    }),
  })
}

export function createQueryScheduledTaskInfoTool(deps) {
  return {
    name: 'query_scheduled_task_info',
    label: 'Query Scheduled Task Info',
    description:
      'Get detailed information about a single scheduled task by its ID. ' +
      'Returns full task details including schedule config, status, delivery settings, and run history.',
    promptSnippet: 'Get detailed info for a specific scheduled task.',
    promptGuidelines: [
      'Use query_scheduled_task_info when you need full details about a specific task.',
      'Requires the exact task_id (obtainable from query_scheduled_task results).',
    ],
    parameters: createQueryScheduledTaskInfoParameters(deps.Type),
    async execute(_toolCallId, input) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim()
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim()
      if (!proxyBase || !token) {
        return { content: [{ type: 'text', text: 'Error: proxy not configured' }] }
      }
      const resp = await deps.fetchImpl(`${proxyBase}/task/${token}/detail`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ taskId: input.task_id }),
      })
      const { data } = await parseProxyJsonResponse(resp)
      return { content: [{ type: 'text', text: JSON.stringify(data, null, 2) }], details: data }
    },
  }
}
