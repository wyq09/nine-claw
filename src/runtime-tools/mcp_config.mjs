import fs from 'node:fs'
import { parseProxyJsonResponse } from './memory_tool_transport.mjs'

const MCP_TRANSPORT_ALIASES = {
  stdio: 'stdio',
  'streamable-http': 'streamable_http',
  streamable_http: 'streamable_http',
  streamablehttp: 'streamable_http',
  http: 'streamable_http',
  sse: 'sse',
}

const VALID_OPERATIONS = new Set(['add', 'list', 'remove', 'enable', 'disable'])

function trimString(value) {
  return typeof value === 'string' ? value.trim() : ''
}

export function normalizeTransport(value) {
  if (typeof value !== 'string') {
    return null
  }
  return MCP_TRANSPORT_ALIASES[value.trim().toLowerCase()] ?? null
}

function normalizeStringArray(value) {
  if (Array.isArray(value)) {
    return value.map((entry) => trimString(entry)).filter(Boolean)
  }
  if (typeof value === 'string' && value.trim()) {
    return value
      .trim()
      .split(/\s+/)
      .filter(Boolean)
  }
  return []
}

function normalizeStringRecord(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return {}
  }
  const result = {}
  for (const [key, entry] of Object.entries(value)) {
    const normalizedKey = trimString(key)
    if (normalizedKey && entry != null) {
      result[normalizedKey] = typeof entry === 'string' ? entry.trim() : String(entry)
    }
  }
  return result
}

function slugify(value) {
  return trimString(value)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
}

export function deriveServerId(raw) {
  const explicit = trimString(raw.id) || trimString(raw.name)
  if (explicit) {
    return slugify(explicit) || explicit
  }
  const url = trimString(raw.url)
  if (url) {
    try {
      return slugify(new URL(url).hostname) || 'mcp-server'
    } catch {}
  }
  const command = trimString(raw.command)
  if (command) {
    const base = command.split(/[\\/]/).pop() ?? command
    return slugify(base) || 'mcp-server'
  }
  return 'mcp-server'
}

export function normalizeServerConfig(raw) {
  const record = raw && typeof raw === 'object' && !Array.isArray(raw) ? raw : {}
  const transport =
    normalizeTransport(record.transport) ??
    normalizeTransport(record.type) ??
    normalizeTransport(record.mode) ??
    (trimString(record.command) ? 'stdio' : 'streamable_http')

  return {
    id: deriveServerId(record),
    name: trimString(record.name),
    transport,
    enabled: record.enabled !== false,
    command: trimString(record.command),
    args: normalizeStringArray(record.args),
    env: normalizeStringRecord(record.env),
    cwd: trimString(record.cwd),
    url: trimString(record.url),
    headers: normalizeStringRecord(record.headers),
  }
}

export function validateServerConfig(server) {
  if (!server.id) {
    throw new Error('MCP server 缺少可用的 id（可显式提供 id，或提供 name / url / command 以便自动生成）。')
  }
  if (server.transport === 'stdio') {
    if (!server.command) {
      throw new Error(`MCP server "${server.id}" 使用 stdio 传输时必须提供 command。`)
    }
    return
  }
  if (!server.url) {
    throw new Error(`MCP server "${server.id}" 使用 ${server.transport} 传输时必须提供 url。`)
  }
  let parsed
  try {
    parsed = new URL(server.url)
  } catch {
    throw new Error(`MCP server "${server.id}" 的 url 非法：${server.url}`)
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(`MCP server "${server.id}" 的 url 只支持 http / https。`)
  }
}

function parseMaybeJson(value) {
  if (typeof value !== 'string') {
    return value
  }
  const trimmed = value.trim()
  if (!trimmed) {
    return null
  }
  try {
    return JSON.parse(trimmed)
  } catch (error) {
    throw new Error(`config 不是合法 JSON：${error instanceof Error ? error.message : String(error)}`)
  }
}

function expandConfigValue(config) {
  const value = parseMaybeJson(config)
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return []
  }
  // Standard `{ mcpServers: { id: {...} } }` shape.
  if (value.mcpServers && typeof value.mcpServers === 'object' && !Array.isArray(value.mcpServers)) {
    return Object.entries(value.mcpServers).map(([id, server]) => ({
      id,
      ...(server && typeof server === 'object' ? server : {}),
    }))
  }
  // A single server object.
  return [value]
}

export function collectServersForAdd(input) {
  const collected = []

  if (Array.isArray(input?.servers)) {
    collected.push(...input.servers)
  }

  if (input?.config != null) {
    collected.push(...expandConfigValue(input.config))
  }

  const hasStructuredFields =
    trimString(input?.id) ||
    trimString(input?.url) ||
    trimString(input?.command) ||
    normalizeTransport(input?.transport)
  if (hasStructuredFields) {
    collected.push({
      id: input.id,
      name: input.name,
      transport: input.transport,
      enabled: input.enabled,
      command: input.command,
      args: input.args,
      env: input.env,
      cwd: input.cwd,
      url: input.url,
      headers: input.headers,
    })
  }

  if (collected.length === 0) {
    throw new Error(
      '没有可添加的 MCP server。请提供 servers 数组、config（含 mcpServers 或单个 server 对象），或直接给出 transport/url/command 等字段。',
    )
  }

  const servers = collected.map((raw) => normalizeServerConfig(raw))
  for (const server of servers) {
    validateServerConfig(server)
  }
  return servers
}

export function normalizeMcpConfigInput(input) {
  const operation = trimString(input?.operation).toLowerCase()
  return {
    operation: VALID_OPERATIONS.has(operation) ? operation : 'list',
    id: trimString(input?.id),
    raw: input ?? {},
  }
}

function describeServer(server) {
  const state = server.enabled === false ? '已停用' : '启用中'
  const target =
    server.transport === 'stdio'
      ? server.command || '(缺少 command)'
      : server.url || '(缺少 url)'
  return `- ${server.id} [${server.transport}] ${state} → ${target}`
}

function formatServerList(servers) {
  if (!Array.isArray(servers) || servers.length === 0) {
    return '当前没有已保存的 MCP server。'
  }
  return [`已保存 ${servers.length} 个 MCP server：`, ...servers.map(describeServer)].join('\n')
}

function writeSnapshot(snapshot, deps) {
  const configPath = trimString(deps.processApi?.env?.NINECLAW_MCP_CONFIG_FILE)
  if (!configPath || !snapshot || typeof snapshot !== 'object') {
    return false
  }
  try {
    deps.fsSync.writeFileSync(configPath, JSON.stringify(snapshot, null, 2))
    return true
  } catch {
    return false
  }
}

async function callProxy(path, body, deps) {
  const proxyBase = trimString(deps.processApi?.env?.NINECLAW_PROXY_BASE_URL)
  const token = trimString(deps.processApi?.env?.NINECLAW_PROXY_SESSION_TOKEN)
  if (!proxyBase || !token) {
    throw new Error('MCP 配置代理未就绪（缺少 proxy base url / token），无法持久化配置。')
  }
  const response = await deps.fetchImpl(`${proxyBase}/mcp/${token}/${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body ?? {}),
  })
  const { ok, data } = await parseProxyJsonResponse(response)
  if (!ok || data?.ok === false) {
    const message = data?.error || data?.message || `MCP 配置操作失败 (HTTP ${response.status})`
    throw new Error(message)
  }
  return data
}

async function executeMcpConfig(input, deps) {
  if (input.operation === 'add') {
    const servers = collectServersForAdd(input.raw)
    const data = await callProxy('add', { servers }, deps)
    const applied = writeSnapshot(data?.snapshot, deps)
    const ids = servers.map((server) => server.id).join('、')
    const note = applied ? '已立即生效，无需重启即可使用。' : '配置已保存，重新打开会话后生效。'
    return {
      content: [
        {
          type: 'text',
          text: [`已保存 MCP server：${ids}。${note}`, '', formatServerList(data?.data?.servers)].join('\n'),
        },
      ],
      details: data,
    }
  }

  if (input.operation === 'list') {
    const data = await callProxy('list', {}, deps)
    writeSnapshot(data?.snapshot, deps)
    return {
      content: [{ type: 'text', text: formatServerList(data?.data?.servers) }],
      details: data,
    }
  }

  if (!input.id) {
    throw new Error(`operation=${input.operation} 时必须提供 id。`)
  }

  if (input.operation === 'remove') {
    const data = await callProxy('remove', { id: input.id }, deps)
    const applied = writeSnapshot(data?.snapshot, deps)
    const note = applied ? '已立即生效。' : '重新打开会话后生效。'
    return {
      content: [
        { type: 'text', text: [`已删除 MCP server：${input.id}。${note}`, '', formatServerList(data?.data?.servers)].join('\n') },
      ],
      details: data,
    }
  }

  const enabled = input.operation === 'enable'
  const data = await callProxy('set-enabled', { id: input.id, enabled }, deps)
  const applied = writeSnapshot(data?.snapshot, deps)
  const verb = enabled ? '启用' : '停用'
  const note = applied ? '已立即生效。' : '重新打开会话后生效。'
  return {
    content: [
      { type: 'text', text: [`已${verb} MCP server：${input.id}。${note}`, '', formatServerList(data?.data?.servers)].join('\n') },
    ],
    details: data,
  }
}

export function createMcpConfigParameters(Type) {
  const stringRecord =
    typeof Type.Any === 'function'
      ? Type.Record(Type.String(), Type.Any())
      : Type.String()
  return Type.Object({
    operation: Type.Union(
      [
        Type.Literal('add'),
        Type.Literal('list'),
        Type.Literal('remove'),
        Type.Literal('enable'),
        Type.Literal('disable'),
      ],
      {
        description:
          'Manage this system\'s own MCP server configuration: add (save/update), list, remove, enable, disable.',
      },
    ),
    id: Type.Optional(
      Type.String({
        description: 'Server id. Required for remove/enable/disable. Optional for add (auto-derived from name/url/command).',
      }),
    ),
    name: Type.Optional(Type.String({ description: 'Human-friendly display name (add).' })),
    transport: Type.Optional(
      Type.Union([Type.Literal('stdio'), Type.Literal('sse'), Type.Literal('streamable_http')], {
        description: 'Transport type for add. Inferred from command/url when omitted.',
      }),
    ),
    command: Type.Optional(Type.String({ description: 'Executable to launch (stdio transport).' })),
    args: Type.Optional(Type.Array(Type.String(), { description: 'Command arguments (stdio transport).' })),
    env: Type.Optional(stringRecord),
    cwd: Type.Optional(Type.String({ description: 'Working directory for the stdio command.' })),
    url: Type.Optional(Type.String({ description: 'Endpoint URL (sse / streamable_http transport).' })),
    headers: Type.Optional(stringRecord),
    enabled: Type.Optional(Type.Boolean({ description: 'Whether the server is enabled. Defaults to true on add.' })),
    config: Type.Optional(
      typeof Type.Any === 'function'
        ? Type.Any({
            description:
              'Paste raw MCP config JSON here: either a single server object or a { "mcpServers": { id: {...} } } map. Used by operation=add.',
          })
        : Type.String({ description: 'Raw MCP config JSON string (single server or { mcpServers } map).' }),
    ),
    servers: Type.Optional(
      Type.Array(
        typeof Type.Any === 'function' ? Type.Record(Type.String(), Type.Any()) : Type.String(),
        { description: 'Array of server config objects to add at once.' },
      ),
    ),
  })
}

export function createMcpConfigTool(deps) {
  return {
    name: 'mcp_config',
    label: 'MCP Config',
    description:
      'Manage MCP server configuration stored inside THIS system. When the user gives you MCP server connection info (transport/url/headers/command JSON), call operation=add to save it here directly — do not ask where to store it and do not suggest Claude Desktop or Cursor. Newly added servers become usable immediately (no app restart). Supports stdio, sse, and streamable_http transports.',
    promptSnippet:
      'Save, list, remove, enable, or disable MCP servers in this system. Added servers work right away via the mcp_tool.',
    promptGuidelines: [
      'When the user provides MCP server config JSON, immediately use operation=add to store it here — you are the system that hosts it.',
      'Accept either structured fields (transport/url/headers or command/args) or paste the raw JSON into the config field.',
      'stdio transport needs command (+ optional args/env/cwd); sse and streamable_http need url (+ optional headers).',
      'After adding, the server is usable without restarting; use mcp_tool to list/call its remote tools.',
      'Use operation=list to review saved servers; remove/enable/disable take an id.',
    ],
    parameters: createMcpConfigParameters(deps.Type),
    async execute(_toolCallId, rawInput) {
      const input = normalizeMcpConfigInput(rawInput)
      return executeMcpConfig(input, {
        fsSync: deps.fsSync ?? fs,
        fetchImpl: deps.fetchImpl ?? globalThis.fetch,
        processApi: deps.processApi ?? process,
      })
    },
  }
}
