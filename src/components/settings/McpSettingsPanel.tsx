import { useEffect, useMemo, useState } from 'react'
import { AppIcon } from '../AppIcon'
import { loadMcpSettings, saveMcpSettings } from '../../lib/mcpClient'
import type { McpServerConfig, McpSettings } from '../../types/mcp'
import {
  createEmptyMcpServerConfig,
  formatCommandArgs,
  normalizeMcpSettings,
  parseJsonRecord,
  parseMcpServersImport,
  splitCommandArgs,
  stringifyJsonRecord,
  validateMcpServerConfig,
} from './mcpSettingsModel'

type McpServerDraft = McpServerConfig & {
  envText: string
  headersText: string
}

function toDraft(server: McpServerConfig): McpServerDraft {
  return {
    ...server,
    envText: stringifyJsonRecord(server.env),
    headersText: stringifyJsonRecord(server.headers),
  }
}

function toSettings(drafts: McpServerDraft[]): McpSettings {
  return normalizeMcpSettings({
    servers: drafts.map((draft) => ({
      id: draft.id,
      name: draft.name,
      transport: draft.transport,
      enabled: draft.enabled,
      command: draft.command,
      args: draft.args,
      env: parseJsonRecord(draft.envText, `Server ${draft.id || '未命名'} 的环境变量`),
      cwd: draft.cwd,
      url: draft.url,
      headers: parseJsonRecord(draft.headersText, `Server ${draft.id || '未命名'} 的请求头`),
    })),
  })
}

function createDraftId(existingDrafts: McpServerDraft[]): string {
  let index = existingDrafts.length + 1
  while (existingDrafts.some((draft) => draft.id === `mcp-server-${index}`)) {
    index += 1
  }
  return `mcp-server-${index}`
}

function mergeImportedServers(existingDrafts: McpServerDraft[], importedServers: McpServerConfig[]): McpServerDraft[] {
  const next = new Map(existingDrafts.map((draft) => [draft.id, draft]))
  for (const server of importedServers) {
    next.set(server.id, toDraft(server))
  }
  return Array.from(next.values())
}

export function McpSettingsPanel() {
  const [draftServers, setDraftServers] = useState<McpServerDraft[]>([])
  const [importJson, setImportJson] = useState('')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')

  useEffect(() => {
    let disposed = false
    setLoading(true)
    void loadMcpSettings()
      .then((settings) => {
        if (disposed) {
          return
        }
        setDraftServers(settings.servers.map(toDraft))
        setError('')
      })
      .catch((loadError: unknown) => {
        if (disposed) {
          return
        }
        setError(loadError instanceof Error ? loadError.message : String(loadError))
      })
      .finally(() => {
        if (!disposed) {
          setLoading(false)
        }
      })

    return () => {
      disposed = true
    }
  }, [])

  const validationMessages = useMemo(
    () =>
      draftServers.flatMap((draft) =>
        validateMcpServerConfig({
          ...draft,
          env: {},
          headers: {},
        }).map((message) => ({ id: draft.id || '未命名', message })),
      ),
    [draftServers],
  )

  const updateDraft = (index: number, updates: Partial<McpServerDraft>) => {
    setDraftServers((previous) => previous.map((draft, itemIndex) => (itemIndex === index ? { ...draft, ...updates } : draft)))
    setNotice('')
    setError('')
  }

  const addServer = () => {
    setDraftServers((previous) => [
      ...previous,
      toDraft({
        ...createEmptyMcpServerConfig(),
        id: createDraftId(previous),
      }),
    ])
    setNotice('')
    setError('')
  }

  const removeServer = (index: number) => {
    setDraftServers((previous) => previous.filter((_, itemIndex) => itemIndex !== index))
    setNotice('')
    setError('')
  }

  const handleImport = () => {
    try {
      const parsed = parseMcpServersImport(importJson)
      setDraftServers((previous) => mergeImportedServers(previous, parsed.servers))
      setImportJson('')
      setNotice(`已导入 ${parsed.servers.length} 个 MCP server。`)
      setError('')
    } catch (importError) {
      setError(importError instanceof Error ? importError.message : String(importError))
    }
  }

  const handleSave = async () => {
    setSaving(true)
    setNotice('')
    setError('')
    try {
      const payload = toSettings(draftServers)
      const saved = await saveMcpSettings(payload)
      setDraftServers(saved.servers.map(toDraft))
      setNotice(`MCP 设置已保存，共 ${saved.servers.length} 个 server。`)
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError))
    } finally {
      setSaving(false)
    }
  }

  if (loading) {
    return <p className="settings-note">正在加载 MCP 设置…</p>
  }

  return (
    <div className="settings-section-stack">
      <div className="settings-peer-gateway-block">
        <div className="settings-peer-gateway-title">
          <strong>MCP 接入</strong>
          <span>
            支持 <code>stdio</code>、<code>Streamable HTTP</code>、<code>SSE</code> 三种方式。保存后，agent
            可通过 <code>mcp_tool</code> 先列远端工具，再调用具体 MCP tool。
          </span>
        </div>
        <div className="settings-peer-actions">
          <button type="button" className="outline-button" onClick={addServer}>
            <AppIcon name="sparkles" size={18} />
            <span>新增 Server</span>
          </button>
          <button type="button" className="outline-button" disabled={saving} onClick={() => void handleSave()}>
            <AppIcon name="network" size={18} />
            <span>{saving ? '保存中…' : '保存 MCP 设置'}</span>
          </button>
        </div>
      </div>

      <div className="settings-peer-gateway-block">
        <div className="settings-peer-gateway-title">
          <strong>JSON 粘贴导入</strong>
          <span>
            支持直接粘贴 <code>{`{"mcpServers": {...}}`}</code> 格式；同 id 会覆盖当前草稿。
          </span>
        </div>
        <label className="input-field" style={{ minHeight: 200 }}>
          <textarea
            value={importJson}
            placeholder={`{\n  "mcpServers": {\n    "miview": {\n      "url": "http://127.0.0.1:25424/mcp",\n      "headers": {\n        "Authorization": "Bearer ..."\n      }\n    }\n  }\n}`}
            onChange={(event) => setImportJson(event.target.value)}
            style={{ minHeight: 180, resize: 'vertical' }}
          />
        </label>
        <div className="settings-peer-actions">
          <button type="button" className="outline-button" disabled={!importJson.trim()} onClick={handleImport}>
            <AppIcon name="book" size={18} />
            <span>导入 JSON</span>
          </button>
        </div>
      </div>

      {draftServers.length === 0 ? (
        <div className="agent-workspace-hint">
          <span>还没有配置任何 MCP server。你可以手动新增，也可以直接粘贴 JSON 导入。</span>
        </div>
      ) : null}

      {draftServers.map((draft, index) => {
        const transportLabel =
          draft.transport === 'stdio'
            ? 'stdio'
            : draft.transport === 'sse'
              ? 'SSE'
              : 'Streamable HTTP'

        return (
          <div key={`${draft.id || 'server'}-${index}`} className="settings-peer-gateway-block">
            <div className="settings-peer-gateway-title">
              <strong>{draft.id || `未命名 Server #${index + 1}`}</strong>
              <span>
                当前接入方式：<code>{transportLabel}</code>
              </span>
            </div>

            <div className="settings-row switch">
              <div>
                <strong>启用此 Server</strong>
                <p>关闭后配置仍会保留，但不会进入 `mcp_tool` 的可选服务列表。</p>
              </div>
              <input
                type="checkbox"
                checked={draft.enabled}
                onChange={(event) => updateDraft(index, { enabled: event.target.checked })}
              />
            </div>

            <div className="settings-row">
              <div>
                <strong>Server ID</strong>
                <p>唯一标识，agent 调用 `mcp_tool` 时通过 `server_id` 选择。</p>
              </div>
              <label className="input-field settings-peer-field">
                <input value={draft.id} placeholder="例如 miview" onChange={(event) => updateDraft(index, { id: event.target.value })} />
              </label>
            </div>

            <div className="settings-row">
              <div>
                <strong>显示名称</strong>
                <p>可选，仅用于设置页展示。</p>
              </div>
              <label className="input-field settings-peer-field">
                <input
                  value={draft.name}
                  placeholder="例如 MiView MCP"
                  onChange={(event) => updateDraft(index, { name: event.target.value })}
                />
              </label>
            </div>

            <div className="settings-row">
              <div>
                <strong>接入方式</strong>
                <p>支持本地进程 stdio、以及基于 HTTP/SSE 的远端 MCP 服务。</p>
              </div>
              <label className="select-field">
                <select
                  value={draft.transport}
                  onChange={(event) =>
                    updateDraft(index, {
                      transport: event.target.value as McpServerConfig['transport'],
                    })
                  }
                >
                  <option value="stdio">stdio</option>
                  <option value="streamable_http">Streamable HTTP</option>
                  <option value="sse">SSE</option>
                </select>
              </label>
            </div>

            {draft.transport === 'stdio' ? (
              <>
                <div className="settings-row">
                  <div>
                    <strong>启动命令</strong>
                    <p>例如 <code>npx</code>、<code>uvx</code> 或本地可执行文件路径。</p>
                  </div>
                  <label className="input-field settings-peer-field">
                    <input
                      value={draft.command}
                      placeholder="例如 npx"
                      onChange={(event) => updateDraft(index, { command: event.target.value })}
                    />
                  </label>
                </div>

                <div className="settings-row">
                  <div>
                    <strong>命令参数</strong>
                    <p>按 shell 风格输入，保存时会拆分为数组。</p>
                  </div>
                  <label className="input-field settings-peer-field">
                    <input
                      value={formatCommandArgs(draft.args)}
                      placeholder='例如 -y @modelcontextprotocol/server-filesystem "/tmp"'
                      onChange={(event) => updateDraft(index, { args: splitCommandArgs(event.target.value) })}
                    />
                  </label>
                </div>

                <div className="settings-row">
                  <div>
                    <strong>工作目录</strong>
                    <p>可选，留空则继承当前运行目录。</p>
                  </div>
                  <label className="input-field settings-peer-field">
                    <input
                      value={draft.cwd}
                      placeholder="例如 /Users/you/project"
                      onChange={(event) => updateDraft(index, { cwd: event.target.value })}
                    />
                  </label>
                </div>

                <div className="settings-row stacked">
                  <div>
                    <strong>环境变量 JSON</strong>
                    <p>要求是字符串值对象，例如 <code>{`{"TOKEN":"abc"}`}</code>。</p>
                  </div>
                  <label className="input-field" style={{ minHeight: 120 }}>
                    <textarea
                      value={draft.envText}
                      placeholder='{\n  "TOKEN": "value"\n}'
                      onChange={(event) => updateDraft(index, { envText: event.target.value })}
                      style={{ minHeight: 100, resize: 'vertical' }}
                    />
                  </label>
                </div>
              </>
            ) : (
              <>
                <div className="settings-row">
                  <div>
                    <strong>服务 URL</strong>
                    <p>
                      {draft.transport === 'sse'
                        ? '填写 SSE MCP 入口 URL。'
                        : '填写 Streamable HTTP MCP 入口 URL，例如 http://127.0.0.1:25424/mcp。'}
                    </p>
                  </div>
                  <label className="input-field settings-peer-field">
                    <input
                      value={draft.url}
                      placeholder="例如 http://127.0.0.1:25424/mcp"
                      onChange={(event) => updateDraft(index, { url: event.target.value })}
                    />
                  </label>
                </div>

                <div className="settings-row stacked">
                  <div>
                    <strong>请求头 JSON</strong>
                    <p>要求是字符串值对象，适合填 Authorization 等鉴权头。</p>
                  </div>
                  <label className="input-field" style={{ minHeight: 140 }}>
                    <textarea
                      value={draft.headersText}
                      placeholder='{\n  "Authorization": "Bearer ..."\n}'
                      onChange={(event) => updateDraft(index, { headersText: event.target.value })}
                      style={{ minHeight: 120, resize: 'vertical' }}
                    />
                  </label>
                </div>
              </>
            )}

            <div className="settings-peer-actions">
              <button type="button" className="outline-button" onClick={() => removeServer(index)}>
                <AppIcon name="close" size={18} />
                <span>删除此 Server</span>
              </button>
            </div>
          </div>
        )
      })}

      {validationMessages.length > 0 ? (
        <div className="skills-feedback error agent-feedback inline">
          <span>{validationMessages.map((item) => `[${item.id}] ${item.message}`).join(' ')}</span>
        </div>
      ) : null}
      {error ? (
        <div className="skills-feedback error agent-feedback inline">
          <span>{error}</span>
        </div>
      ) : null}
      {notice ? (
        <div className="skills-feedback success agent-feedback inline">
          <span>{notice}</span>
        </div>
      ) : null}
    </div>
  )
}
