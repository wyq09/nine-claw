import type { McpServerConfig, McpSettings, McpTransportType, ParsedMcpImport } from '../../types/mcp'

const MCP_TRANSPORT_ALIASES: Record<string, McpTransportType> = {
  stdio: 'stdio',
  'streamable-http': 'streamable_http',
  streamable_http: 'streamable_http',
  streamablehttp: 'streamable_http',
  sse: 'sse',
}

export function createEmptyMcpServerConfig(): McpServerConfig {
  return {
    id: '',
    name: '',
    transport: 'streamable_http',
    enabled: true,
    command: '',
    args: [],
    env: {},
    cwd: '',
    url: '',
    headers: {},
  }
}

export function normalizeMcpTransport(value: unknown): McpTransportType | null {
  if (typeof value !== 'string') {
    return null
  }
  return MCP_TRANSPORT_ALIASES[value.trim().toLowerCase()] ?? null
}

export function splitCommandArgs(input: string): string[] {
  const args: string[] = []
  let current = ''
  let quote: '"' | "'" | null = null
  let escaping = false

  for (const char of input) {
    if (escaping) {
      current += char
      escaping = false
      continue
    }
    if (char === '\\') {
      escaping = true
      continue
    }
    if (quote) {
      if (char === quote) {
        quote = null
      } else {
        current += char
      }
      continue
    }
    if (char === '"' || char === "'") {
      quote = char
      continue
    }
    if (/\s/.test(char)) {
      if (current) {
        args.push(current)
        current = ''
      }
      continue
    }
    current += char
  }

  if (escaping) {
    current += '\\'
  }
  if (current) {
    args.push(current)
  }
  return args
}

export function formatCommandArgs(args: string[]): string {
  return args
    .map((value) => {
      if (!value.includes(' ') && !value.includes('"')) {
        return value
      }
      return `"${value.replaceAll('"', '\\"')}"`
    })
    .join(' ')
}

export function parseJsonRecord(input: string, fieldLabel: string): Record<string, string> {
  const trimmed = input.trim()
  if (!trimmed) {
    return {}
  }

  let parsed: unknown
  try {
    parsed = JSON.parse(trimmed)
  } catch (error) {
    throw new Error(`${fieldLabel} 不是合法 JSON：${error instanceof Error ? error.message : String(error)}`)
  }

  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    throw new Error(`${fieldLabel} 必须是 JSON 对象。`)
  }

  const result: Record<string, string> = {}
  for (const [key, value] of Object.entries(parsed)) {
    if (typeof value !== 'string') {
      throw new Error(`${fieldLabel} 的值必须全部是字符串，字段 ${key} 不符合要求。`)
    }
    result[key] = value
  }
  return result
}

export function stringifyJsonRecord(value: Record<string, string>): string {
  if (Object.keys(value).length === 0) {
    return ''
  }
  return JSON.stringify(value, null, 2)
}

function coerceString(value: unknown): string {
  return typeof value === 'string' ? value.trim() : ''
}

function coerceStringArray(value: unknown): string[] {
  if (Array.isArray(value)) {
    return value.flatMap((entry) => (typeof entry === 'string' && entry.trim() ? [entry.trim()] : []))
  }
  if (typeof value === 'string' && value.trim()) {
    return splitCommandArgs(value)
  }
  return []
}

function coerceStringRecord(value: unknown): Record<string, string> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    return {}
  }

  const result: Record<string, string> = {}
  for (const [key, entry] of Object.entries(value)) {
    if (typeof entry === 'string' && key.trim()) {
      result[key] = entry
    }
  }
  return result
}

function inferTransport(raw: Record<string, unknown>): McpTransportType {
  const explicit =
    normalizeMcpTransport(raw.transport) ??
    normalizeMcpTransport(raw.type) ??
    normalizeMcpTransport(raw.mode)
  if (explicit) {
    return explicit
  }
  if (typeof raw.command === 'string' && raw.command.trim()) {
    return 'stdio'
  }
  return 'streamable_http'
}

export function normalizeMcpServerConfig(input: Partial<McpServerConfig>): McpServerConfig {
  const base = createEmptyMcpServerConfig()
  return {
    ...base,
    ...input,
    id: (input.id ?? base.id).trim(),
    name: (input.name ?? base.name).trim(),
    transport: input.transport ?? base.transport,
    enabled: input.enabled ?? true,
    command: (input.command ?? base.command).trim(),
    args: Array.isArray(input.args) ? input.args.filter((value) => value.trim()) : [],
    env: input.env ?? {},
    cwd: (input.cwd ?? base.cwd).trim(),
    url: (input.url ?? base.url).trim(),
    headers: input.headers ?? {},
  }
}

export function validateMcpServerConfig(server: McpServerConfig): string[] {
  const errors: string[] = []
  if (!server.id.trim()) {
    errors.push('ID 不能为空。')
  }

  if (server.transport === 'stdio') {
    if (!server.command.trim()) {
      errors.push(`Server ${server.id || '未命名'} 的启动命令不能为空。`)
    }
  } else if (!server.url.trim()) {
    errors.push(`Server ${server.id || '未命名'} 的 URL 不能为空。`)
  }

  return errors
}

export function normalizeMcpSettings(settings: McpSettings): McpSettings {
  const seen = new Set<string>()
  const servers = settings.servers
    .map((server) => normalizeMcpServerConfig(server))
    .filter((server) => {
      if (!server.id) {
        return false
      }
      if (seen.has(server.id)) {
        return false
      }
      seen.add(server.id)
      return true
    })
  return { servers }
}

export function parseMcpServersImport(input: string): ParsedMcpImport {
  let parsed: unknown
  try {
    parsed = JSON.parse(input)
  } catch (error) {
    throw new Error(`导入 JSON 解析失败：${error instanceof Error ? error.message : String(error)}`)
  }

  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    throw new Error('导入内容必须是一个 JSON 对象。')
  }

  const rawServers = (parsed as Record<string, unknown>).mcpServers
  if (typeof rawServers !== 'object' || rawServers === null || Array.isArray(rawServers)) {
    throw new Error('导入内容缺少 `mcpServers` 对象。')
  }

  const servers: McpServerConfig[] = []
  for (const [serverId, rawValue] of Object.entries(rawServers)) {
    if (typeof rawValue !== 'object' || rawValue === null || Array.isArray(rawValue)) {
      throw new Error(`Server ${serverId} 的配置必须是对象。`)
    }
    const record = rawValue as Record<string, unknown>
    const transport = inferTransport(record)
    const server = normalizeMcpServerConfig({
      id: serverId,
      name: coerceString(record.name),
      transport,
      enabled: record.enabled !== false,
      command: coerceString(record.command),
      args: coerceStringArray(record.args),
      env: coerceStringRecord(record.env),
      cwd: coerceString(record.cwd),
      url: coerceString(record.url),
      headers: coerceStringRecord(record.headers),
    })
    const errors = validateMcpServerConfig(server)
    if (errors.length > 0) {
      throw new Error(errors.join(' '))
    }
    servers.push(server)
  }

  return normalizeMcpSettings({ servers })
}
