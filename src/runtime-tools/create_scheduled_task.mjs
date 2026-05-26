import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

export function createCreateScheduledTaskParameters(Type) {
  return Type.Object({
    goal: Type.String({
      minLength: 1,
      description:
        'Task content / goal description. Supports natural language. This is what the agent will execute or remind the user about when the task triggers.',
    }),
    title: Type.Optional(
      Type.String({
        description:
          'Short task title for display. Auto-generated from goal if omitted.',
      }),
    ),
    taskType: Type.Optional(
      Type.Union([Type.Literal('reminder'), Type.Literal('agent_prompt')], {
        description:
          '"reminder" = send a notification only; "agent_prompt" = agent executes the task. Default: "agent_prompt".',
      }),
    ),
    scheduleType: Type.Union(
      [
        Type.Literal('interval'),
        Type.Literal('daily_time'),
        Type.Literal('weekly_time'),
        Type.Literal('monthly_time'),
        Type.Literal('once_at'),
      ],
      {
        description:
          'How the task repeats. "interval" = every N minutes; "daily_time" = at specific times every day; "weekly_time" = on specific weekdays at specific times; "monthly_time" = on specific dates at specific times; "once_at" = one-time at a specific timestamp.',
      },
    ),
    intervalMinutes: Type.Optional(
      Type.Integer({
        minimum: 1,
        maximum: 1440,
        description: 'Required when scheduleType is "interval". Repeat every N minutes.',
      }),
    ),
    dailyTimes: Type.Optional(
      Type.Array(Type.String(), {
        description:
          'Trigger times in "HH:MM" format (24h). Required for daily_time, weekly_time, monthly_time. E.g. ["09:00", "18:30"].',
      }),
    ),
    weeklyDays: Type.Optional(
      Type.Array(Type.Integer(), {
        description:
          'Required for weekly_time. 1=Monday … 7=Sunday. E.g. [1, 3, 5].',
      }),
    ),
    monthlyDays: Type.Optional(
      Type.Array(Type.Integer(), {
        description:
          'Required for monthly_time. Day of month 1-31. E.g. [1, 15].',
      }),
    ),
    runAtMs: Type.Optional(
      Type.Integer({
        description:
          'Required for once_at. UTC timestamp in milliseconds when the task should fire.',
      }),
    ),
    timezone: Type.Optional(
      Type.String({
        description: 'IANA timezone string. Default: "Asia/Shanghai".',
      }),
    ),
    resultInNewSession: Type.Optional(
      Type.Boolean({
        description:
          'If true, each execution result opens in a new chat session. Default: false.',
      }),
    ),
  })
}

export function createCreateScheduledTaskTool(deps) {
  return {
    name: 'create_scheduled_task',
    label: 'Create Scheduled Task',
    description:
      'Create a scheduled task in the task center. ' +
      'The task will repeat according to the schedule and execute the specified goal. ' +
      'Supports interval, daily, weekly, monthly, and one-time schedules.',
    promptSnippet:
      'Create scheduled tasks (reminders or agent actions) that repeat on a schedule.',
    promptGuidelines: [
      'Use create_scheduled_task when the user asks for recurring reminders or automated tasks.',
      'taskType "agent_prompt" (default) means the agent will execute the goal when triggered. "reminder" only sends a notification.',
      'For scheduleType "interval", provide intervalMinutes (e.g. 30 for every 30 minutes).',
      'For "daily_time", provide dailyTimes like ["09:00"].',
      'For "weekly_time", provide weeklyDays (1=Mon…7=Sun) and dailyTimes.',
      'For "monthly_time", provide monthlyDays (1-31) and dailyTimes.',
      'For "once_at", provide runAtMs as a UTC millisecond timestamp.',
      'goal supports natural language — describe what the task should do clearly.',
      'title is optional; it will be auto-generated from the goal if omitted.',
    ],
    parameters: createCreateScheduledTaskParameters(deps.Type),
    async execute(_toolCallId, input) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim()
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim()
      if (!proxyBase || !token) {
        return { content: [{ type: 'text', text: 'Error: proxy not configured' }] }
      }
      const body = {
        goal: input.goal,
        title: input.title || '',
        taskType: input.taskType || 'agent_prompt',
        scheduleType: input.scheduleType,
        intervalMinutes: input.intervalMinutes ?? null,
        dailyTimes: input.dailyTimes ?? [],
        weeklyDays: input.weeklyDays ?? [],
        monthlyDays: input.monthlyDays ?? [],
        runAtMs: input.runAtMs ?? null,
        timezone: input.timezone || 'Asia/Shanghai',
        resultInNewSession: input.resultInNewSession ?? false,
      }
      const resp = await deps.fetchImpl(`${proxyBase}/task/${token}/create`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      })
      const { data } = await parseProxyJsonResponse(resp)
      return { content: [{ type: 'text', text: JSON.stringify(data, null, 2) }], details: data }
    },
  }
}
