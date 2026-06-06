import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'
import { pathToFileURL } from 'node:url'

const DEFAULT_LIST_LIMIT = 50
const MAX_RENDER_CHARS = 24000
const requireFromHere = createRequire(import.meta.url)

const MCP_TRANSPORT_ALIASES = {
  stdio: 'stdio',
  'streamable-http': 'streamable_http',
  streamable_http: 'streamable_http',
  streamablehttp: 'streamable_http',
  sse: 'sse',
}

function normalizeTransport(value) {
  if (typeof value !== 'string') {
    return null
  }
  return MCP_TRANSPORT_ALIASES[value.trim().toLowerCase()] ?? null
}

function trimString(value) {
  return typeof value === 'string' ? value.trim() : ''
}

function normalizeStringArray(value) {
  if (!Array.isArray(value)) {
    return []
  }
  return value.map((entry) => trimString(entry)).filter(Boolean)
}

function normalizeStringRecord(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return {}
  }
  const result = {}
  for (const [key, entry] of Object.entries(value)) {
    const normalizedKey = trimString(key)
    const normalizedValue = trimString(entry)
    if (normalizedKey && normalizedValue) {
      result[normalizedKey] = normalizedValue
    }
  }
  return result
}

function normalizeServerConfig(serverId, rawValue) {
  const record = rawValue && typeof rawValue === 'object' && !Array.isArray(rawValue) ? rawValue : {}
  const transport =
    normalizeTransport(record.transport) ??
    normalizeTransport(record.type) ??
    normalizeTransport(record.mode) ??
    (trimString(record.command) ? 'stdio' : 'streamable_http')

  return {
    id: trimString(serverId),
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

export function normalizeRuntimeMcpSettings(rawSettings) {
  if (!rawSettings || typeof rawSettings !== 'object' || Array.isArray(rawSettings)) {
    return { servers: [] }
  }

  const record = rawSettings
  const servers = []

  if (Array.isArray(record.servers)) {
    for (const rawServer of record.servers) {
      if (!rawServer || typeof rawServer !== 'object' || Array.isArray(rawServer)) {
        continue
      }
      const normalized = normalizeServerConfig(rawServer.id, rawServer)
      if (normalized.id) {
        servers.push(normalized)
      }
    }
    return { servers }
  }

  const rawServers = record.mcpServers
  if (!rawServers || typeof rawServers !== 'object' || Array.isArray(rawServers)) {
    return { servers: [] }
  }

  for (const [serverId, rawServer] of Object.entries(rawServers)) {
    const normalized = normalizeServerConfig(serverId, rawServer)
    if (normalized.id) {
      servers.push(normalized)
    }
  }

  return { servers }
}

export function loadRuntimeMcpSettings(configPath, fsSync = fs) {
  const normalizedPath = trimString(configPath)
  if (!normalizedPath) {
    return { servers: [] }
  }
  const raw = fsSync.readFileSync(normalizedPath, 'utf8')
  const parsed = JSON.parse(raw)
  return normalizeRuntimeMcpSettings(parsed)
}

export function resolveMcpServer(settings, serverId) {
  const enabledServers = (settings?.servers ?? []).filter((server) => server.enabled !== false)
  const requestedId = trimString(serverId)

  if (requestedId) {
    const matched = enabledServers.find((server) => server.id === requestedId)
    if (!matched) {
      throw new Error(`未找到启用中的 MCP server: ${requestedId}`)
    }
    return matched
  }

  if (enabledServers.length === 1) {
    return enabledServers[0]
  }

  if (enabledServers.length === 0) {
    throw new Error('当前没有启用的 MCP server。请先到设置 -> MCP 中配置并启用至少一个服务。')
  }

  throw new Error(`当前启用了 ${enabledServers.length} 个 MCP server，请显式传入 server_id。`)
}

function normalizeToolArguments(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return {}
  }
  return value
}

function clampListLimit(value) {
  if (!Number.isFinite(value)) {
    return DEFAULT_LIST_LIMIT
  }
  const normalized = Math.trunc(value)
  if (normalized <= 0) {
    return DEFAULT_LIST_LIMIT
  }
  return Math.min(normalized, 200)
}

export function normalizeMcpToolInput(input) {
  return {
    operation: trimString(input?.operation) || 'list_tools',
    serverId: trimString(input?.server_id),
    toolName: trimString(input?.tool_name),
    arguments: normalizeToolArguments(input?.arguments),
    limit: clampListLimit(input?.limit),
  }
}

function uniqueStrings(values) {
  return [...new Set(values.filter(Boolean))]
}

function candidateSdkResolvePaths(processApi = process) {
  const runtimeRoot = trimString(processApi?.env?.NINECLAW_PI_RUNTIME_ROOT)
  const cwd = typeof processApi?.cwd === 'function' ? trimString(processApi.cwd()) : trimString(process.cwd())
  return uniqueStrings([
    runtimeRoot,
    runtimeRoot ? path.join(runtimeRoot, 'node_modules') : '',
    cwd,
    cwd ? path.join(cwd, 'node_modules') : '',
  ])
}

function resolveSdkModule(specifier, processApi = process) {
  for (const basePath of candidateSdkResolvePaths(processApi)) {
    try {
      return requireFromHere.resolve(specifier, { paths: [basePath] })
    } catch {}
  }

  try {
    return requireFromHere.resolve(specifier)
  } catch (error) {
    throw new Error(
      `无法定位 ${specifier}。请确认运行时根目录已包含 MCP SDK，或在开发环境重新执行 npm install / prepare:pi-runtime。原始错误: ${error instanceof Error ? error.message : String(error)}`,
    )
  }
}

const sdkModuleCache = new Map()

async function importSdkModule(specifier, processApi = process) {
  const resolved = resolveSdkModule(specifier, processApi)
  if (!sdkModuleCache.has(resolved)) {
    sdkModuleCache.set(resolved, import(pathToFileURL(resolved).href))
  }
  return sdkModuleCache.get(resolved)
}

async function loadMcpSdk(processApi = process) {
  const [stdioModule, streamableModule, sseModule] = await Promise.all([
    importSdkModule('@modelcontextprotocol/sdk/client/stdio.js', processApi),
    importSdkModule('@modelcontextprotocol/sdk/client/streamableHttp.js', processApi),
    importSdkModule('@modelcontextprotocol/sdk/client/sse.js', processApi),
  ])

  return {
    StdioClientTransport: stdioModule.StdioClientTransport,
    getDefaultEnvironment: stdioModule.getDefaultEnvironment,
    StreamableHTTPClientTransport: streamableModule.StreamableHTTPClientTransport,
    SSEClientTransport: sseModule.SSEClientTransport,
  }
}

function headersToRecord(headers) {
  if (!headers) {
    return {}
  }
  if (headers instanceof Headers) {
    return Object.fromEntries(headers.entries())
  }
  if (Array.isArray(headers)) {
    return Object.fromEntries(headers.map(([key, value]) => [String(key), String(value)]))
  }
  if (typeof headers === 'object') {
    return Object.fromEntries(
      Object.entries(headers).map(([key, value]) => [String(key), value == null ? '' : String(value)]),
    )
  }
  return {}
}

function mergeHeaderRecords(baseHeaders, extraHeaders) {
  const merged = new Map()
  for (const [key, value] of Object.entries(baseHeaders)) {
    merged.set(key.toLowerCase(), value)
  }
  for (const [key, value] of Object.entries(extraHeaders)) {
    merged.set(key.toLowerCase(), value)
  }
  return Object.fromEntries(merged.entries())
}

function createFetchWithHeaders(fetchImpl, extraHeaders) {
  return async (url, init = {}) => {
    const mergedHeaders = mergeHeaderRecords(headersToRecord(init.headers), extraHeaders)
    return fetchImpl(url, {
      ...init,
      headers: mergedHeaders,
    })
  }
}

export async function createSdkTransport(server, deps = {}) {
  const sdk = await loadMcpSdk(deps.processApi ?? process)
  if (server.transport === 'stdio') {
    return new sdk.StdioClientTransport({
      command: server.command,
      args: server.args,
      env: {
        ...sdk.getDefaultEnvironment(),
        ...server.env,
      },
      cwd: server.cwd || undefined,
      stderr: 'pipe',
    })
  }

  const fetchImpl = deps.fetchImpl ?? globalThis.fetch
  const fetchWithHeaders = createFetchWithHeaders(fetchImpl, server.headers)

  if (server.transport === 'sse') {
    return new sdk.SSEClientTransport(new URL(server.url), {
      fetch: fetchWithHeaders,
      eventSourceInit: {
        fetch: fetchWithHeaders,
      },
    })
  }

  return new sdk.StreamableHTTPClientTransport(new URL(server.url), {
    fetch: fetchWithHeaders,
  })
}

function truncateText(text, maxChars = MAX_RENDER_CHARS) {
  const normalized = typeof text === 'string' ? text : String(text ?? '')
  if (normalized.length <= maxChars) {
    return normalized
  }
  return `${normalized.slice(0, maxChars)}\n\n[truncated ${normalized.length - maxChars} chars]`
}

function safePrettyJson(value) {
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

function describeInputSchema(tool) {
  const keys = Object.keys(tool?.inputSchema?.properties ?? {})
  if (keys.length === 0) {
    return '无参数'
  }
  return `参数: ${keys.join(', ')}`
}

export function formatMcpToolList(server, tools, limit = DEFAULT_LIST_LIMIT) {
  const shown = tools.slice(0, limit)
  const lines = [
    `MCP server: ${server.id}`,
    `Available tools: ${shown.length}/${tools.length}`,
  ]
  for (const tool of shown) {
    const desc = trimString(tool.description)
    lines.push(`- ${tool.name}${desc ? `: ${desc}` : ''}`)
    lines.push(`  ${describeInputSchema(tool)}`)
  }
  if (shown.length < tools.length) {
    lines.push(`仅展示前 ${shown.length} 个工具；可通过 limit 查看更多。`)
  }
  return lines.join('\n')
}

function renderContentBlock(block) {
  if (!block || typeof block !== 'object') {
    return safePrettyJson(block)
  }
  if (block.type === 'text') {
    return block.text ?? ''
  }
  if (block.type === 'image') {
    return `[image:${block.mimeType ?? 'unknown'} bytes=${(block.data ?? '').length}]`
  }
  if (block.type === 'audio') {
    return `[audio:${block.mimeType ?? 'unknown'} bytes=${(block.data ?? '').length}]`
  }
  if (block.type === 'resource') {
    return safePrettyJson(block.resource ?? block)
  }
  if (block.type === 'resource_link') {
    return safePrettyJson(block)
  }
  return safePrettyJson(block)
}

export function formatMcpCallResult(server, toolName, result) {
  const lines = [
    `MCP server: ${server.id}`,
    `Tool: ${toolName}`,
    `isError: ${result?.isError ? 'yes' : 'no'}`,
  ]

  if (result?.structuredContent !== undefined) {
    lines.push('structuredContent:')
    lines.push(safePrettyJson(result.structuredContent))
  }

  if (Array.isArray(result?.content) && result.content.length > 0) {
    lines.push('content:')
    for (const block of result.content) {
      lines.push(renderContentBlock(block))
    }
  } else if (result?.toolResult !== undefined) {
    lines.push('toolResult:')
    lines.push(safePrettyJson(result.toolResult))
  } else {
    lines.push(safePrettyJson(result))
  }

  return truncateText(lines.join('\n'))
}

function createTransportRpcClient(transport) {
  let nextId = 1
  const pending = new Map()

  const rejectAll = (error) => {
    for (const entry of pending.values()) {
      entry.reject(error)
    }
    pending.clear()
  }

  const handleMessage = (message) => {
    if (!message || typeof message !== 'object') {
      return
    }
    if (Array.isArray(message)) {
      for (const item of message) {
        handleMessage(item)
      }
      return
    }

    if ('id' in message && pending.has(message.id)) {
      const pendingEntry = pending.get(message.id)
      pending.delete(message.id)
      if (message.error) {
        pendingEntry.reject(new Error(message.error.message || safePrettyJson(message.error)))
      } else {
        pendingEntry.resolve(message.result)
      }
    }
  }

  async function request(method, params) {
    const id = nextId
    nextId += 1
    const responsePromise = new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject })
    })
    await transport.send({
      jsonrpc: '2.0',
      id,
      method,
      params,
    })
    return responsePromise
  }

  return {
    async connect() {
      transport.onmessage = handleMessage
      transport.onerror = (error) => rejectAll(error instanceof Error ? error : new Error(String(error)))
      transport.onclose = () => rejectAll(new Error('MCP transport 已关闭'))
      await transport.start()
      const initializeResult = await request('initialize', {
        protocolVersion: '2025-06-18',
        capabilities: {},
        clientInfo: {
          name: 'nineclaw-mcp-tool',
          version: '1.0.0',
        },
      })
      if (typeof transport.setProtocolVersion === 'function' && initializeResult?.protocolVersion) {
        transport.setProtocolVersion(initializeResult.protocolVersion)
      }
      await transport.send({
        jsonrpc: '2.0',
        method: 'notifications/initialized',
      })
      return initializeResult
    },
    async listTools() {
      return request('tools/list', {})
    },
    async callTool(params) {
      return request('tools/call', params)
    },
    async close() {
      if (typeof transport.close === 'function') {
        await transport.close()
      }
    },
  }
}

async function cleanupClient(client, transport) {
  try {
    if (typeof transport?.terminateSession === 'function') {
      await transport.terminateSession()
    }
  } catch {}

  try {
    if (typeof client?.close === 'function') {
      await client.close()
      return
    }
  } catch {}

  if (typeof transport?.close === 'function') {
    try {
      await transport.close()
    } catch {}
  }
}

async function executeMcpOperation(input, deps) {
  const configPath = trimString(deps.processApi?.env?.NINECLAW_MCP_CONFIG_FILE)
  if (!configPath) {
    throw new Error('当前会话没有注入 MCP 配置文件，请先在设置 -> MCP 中保存配置后重试。')
  }

  const settings = loadRuntimeMcpSettings(configPath, deps.fsSync ?? fs)
  const server = resolveMcpServer(settings, input.serverId)
  const transport = await (deps.transportFactory ?? createSdkTransport)(server, deps)
  const client = deps.clientFactory?.(server, deps) ?? createTransportRpcClient(transport)

  await client.connect()
  try {
    if (input.operation === 'list_tools') {
      const result = await client.listTools()
      return {
        content: [{ type: 'text', text: formatMcpToolList(server, result.tools ?? [], input.limit) }],
        details: {
          serverId: server.id,
          tools: result.tools ?? [],
        },
      }
    }

    if (!input.toolName) {
      throw new Error('operation=call_tool 时必须提供 tool_name。')
    }

    const result = await client.callTool({
      name: input.toolName,
      arguments: input.arguments,
    })
    return {
      content: [{ type: 'text', text: formatMcpCallResult(server, input.toolName, result) }],
      details: {
        serverId: server.id,
        toolName: input.toolName,
        result,
      },
    }
  } finally {
    await cleanupClient(client, transport)
  }
}

function createToolArgumentsSchema(Type) {
  if (typeof Type.Any === 'function') {
    return Type.Record(Type.String(), Type.Any(), {
      description: 'JSON object passed through to the remote MCP tool as arguments.',
    })
  }
  return Type.String({
    description: 'JSON object passed through to the remote MCP tool as arguments.',
  })
}

export function createMcpToolParameters(Type) {
  return Type.Object({
    operation: Type.Union([Type.Literal('list_tools'), Type.Literal('call_tool')], {
      description: 'List tools exposed by a configured MCP server, or call one remote MCP tool.',
    }),
    server_id: Type.Optional(
      Type.String({
        description: 'Configured MCP server id. Omit only when exactly one enabled server exists.',
      }),
    ),
    tool_name: Type.Optional(
      Type.String({
        description: 'Remote MCP tool name. Required when operation=call_tool.',
      }),
    ),
    arguments: Type.Optional(createToolArgumentsSchema(Type)),
    limit: Type.Optional(
      Type.Integer({
        minimum: 1,
        maximum: 200,
        description: 'Maximum number of tools to render when operation=list_tools.',
      }),
    ),
  })
}

export function createMcpTool(deps) {
  return {
    name: 'mcp_tool',
    label: 'MCP Tool',
    description:
      'Bridge to configured MCP servers. Use it to inspect remote MCP tools or call a specific remote MCP tool.',
    promptSnippet:
      'List tools from a configured MCP server, then call the remote tool you actually need.',
    promptGuidelines: [
      'Use operation=list_tools first when you are not sure which remote MCP tool is available.',
      'If multiple MCP servers are enabled, pass server_id explicitly.',
      'Pass a JSON object in arguments when operation=call_tool.',
    ],
    parameters: createMcpToolParameters(deps.Type),
    async execute(_toolCallId, rawInput) {
      const input = normalizeMcpToolInput(rawInput)
      return executeMcpOperation(input, {
        fsSync: deps.fsSync ?? fs,
        fetchImpl: deps.fetchImpl ?? globalThis.fetch,
        processApi: deps.processApi ?? process,
        clientFactory: deps.clientFactory,
        transportFactory: deps.transportFactory,
      })
    },
  }
}
