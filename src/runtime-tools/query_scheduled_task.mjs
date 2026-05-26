import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

export function createQueryScheduledTaskParameters(Type) {
  return Type.Object({
    status: Type.Optional(
      Type.Union([Type.Literal('active'), Type.Literal('paused'), Type.Literal('deleted')], {
        description: 'Filter by task status. If omitted, returns all non-deleted tasks.',
      }),
    ),
    schedule_type: Type.Optional(
      Type.Union([
        Type.Literal('interval'),
        Type.Literal('daily_time'),
        Type.Literal('weekly_time'),
        Type.Literal('monthly_time'),
        Type.Literal('once_at'),
      ], {
        description: 'Filter by schedule type.',
      }),
    ),
  })
}

export function createQueryScheduledTaskTool(deps) {
  return {
    name: 'query_scheduled_task',
    label: 'Query Scheduled Tasks',
    description:
      'Query the list of scheduled tasks belonging to the current agent. ' +
      'Returns task titles, schedule info, status, last/next run times.',
    promptSnippet: 'List the agent\'s scheduled tasks from the task center.',
    promptGuidelines: [
      'Use query_scheduled_task to check existing tasks before creating new ones.',
      'Returns tasks for the current agent only.',
      'Filter by status or schedule_type if you only need a subset.',
    ],
    parameters: createQueryScheduledTaskParameters(deps.Type),
    async execute(_toolCallId, input) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim()
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim()
      if (!proxyBase || !token) {
        return { content: [{ type: 'text', text: 'Error: proxy not configured' }] }
      }
      const body = {}
      if (input.status) body.status = input.status
      if (input.schedule_type) body.scheduleType = input.schedule_type
      const resp = await deps.fetchImpl(`${proxyBase}/task/${token}/list`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      })
      const { data } = await parseProxyJsonResponse(resp)
      return { content: [{ type: 'text', text: JSON.stringify(data, null, 2) }], details: data }
    },
  }
}
