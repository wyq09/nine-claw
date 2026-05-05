import { invoke } from '@tauri-apps/api/core'

export type WorkspaceKvMemoryUiEntry = {
  key: string
  value: unknown
  updatedAt: number
  originKind?: 'manual' | 'auto' | 'migration' | string
  bucket?: string
  tags?: string[]
}

export async function workspaceKvMemoryUiList(options: {
  agentId?: string | null
  workspaceId?: string | null
  limit?: number
}): Promise<{ ok: boolean; workspaceId: string; entries: WorkspaceKvMemoryUiEntry[]; total?: number }> {
  return invoke('workspace_kv_memory_ui_list', {
    agentId: options.agentId ?? null,
    workspaceId: options.workspaceId ?? null,
    limit: options.limit ?? 500,
  })
}

export async function workspaceKvMemoryUiStore(options: {
  agentId?: string | null
  workspaceId?: string | null
  key: string
  value: unknown
}): Promise<{ ok: boolean; workspaceId: string; key: string; updatedAt: number }> {
  return invoke('workspace_kv_memory_ui_store', {
    agentId: options.agentId ?? null,
    workspaceId: options.workspaceId ?? null,
    key: options.key,
    value: options.value,
  })
}

export async function workspaceKvMemoryUiForget(options: {
  agentId?: string | null
  workspaceId?: string | null
  key: string
}): Promise<void> {
  await invoke('workspace_kv_memory_ui_forget', {
    agentId: options.agentId ?? null,
    workspaceId: options.workspaceId ?? null,
    key: options.key,
  })
}

export async function workspaceKvMemoryUiReorganize(options: {
  agentId?: string | null
  workspaceId?: string | null
}): Promise<{ ok: boolean; inserted?: number; skipped?: number; message?: string }> {
  return invoke('workspace_kv_memory_ui_reorganize', {
    agentId: options.agentId ?? null,
    workspaceId: options.workspaceId ?? null,
  })
}
