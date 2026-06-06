export type McpTransportType = 'stdio' | 'streamable_http' | 'sse'

export type McpServerConfig = {
  id: string
  name: string
  transport: McpTransportType
  enabled: boolean
  command: string
  args: string[]
  env: Record<string, string>
  cwd: string
  url: string
  headers: Record<string, string>
}

export type McpSettings = {
  servers: McpServerConfig[]
}

export type ParsedMcpImport = {
  servers: McpServerConfig[]
}
