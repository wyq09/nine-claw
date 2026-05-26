import { invoke } from '@tauri-apps/api/core'

export type SessionWorkspaceState = {
  sessionId: string
  topicWorkspaceDir: string
  currentWorkspaceDir: string
  currentIsTopic: boolean
  recents: Array<{ path: string; lastUsedAt: number }>
}

export type SessionWorkspaceEntry = {
  name: string
  relPath: string
  isDir: boolean
  size: number | null
  modifiedMs: number | null
}

export type SessionWorkspaceFileInfo = {
  name: string
  relPath: string
  absolutePath: string
  isDir: boolean
  size: number | null
  modifiedMs: number | null
}

export type SessionWorkspacePreviewKind =
  | 'markdown'
  | 'code'
  | 'html'
  | 'image'
  | 'text'
  | 'folder'
  | 'binary'

export type SessionWorkspaceReadResult = {
  info: SessionWorkspaceFileInfo
  content: string | null
  previewKind: SessionWorkspacePreviewKind
}

export function sessionWorkspaceGet(sessionId: string): Promise<SessionWorkspaceState> {
  return invoke<SessionWorkspaceState>('session_workspace_get', { sessionId })
}

export function sessionWorkspaceSwitch(
  sessionId: string,
  dir: string,
): Promise<SessionWorkspaceState> {
  return invoke<SessionWorkspaceState>('session_workspace_switch', { sessionId, dir })
}

export function sessionWorkspaceResetToTopic(sessionId: string): Promise<SessionWorkspaceState> {
  return invoke<SessionWorkspaceState>('session_workspace_reset_to_topic', { sessionId })
}

export function sessionWorkspaceListEntries(
  sessionId: string,
  subPath?: string | null,
): Promise<SessionWorkspaceEntry[]> {
  return invoke<SessionWorkspaceEntry[]>('session_workspace_list_entries', {
    sessionId,
    subPath: subPath ?? null,
  })
}

export function sessionWorkspaceReadFile(
  sessionId: string,
  relPath: string,
): Promise<SessionWorkspaceReadResult> {
  return invoke<SessionWorkspaceReadResult>('session_workspace_read_file', { sessionId, relPath })
}

export function sessionWorkspaceAbsolutePath(
  sessionId: string,
  relPath: string,
  allowDir = false,
): Promise<string> {
  return invoke<string>('session_workspace_absolute_path', { sessionId, relPath, allowDir })
}

export function sessionWorkspaceCreateFile(payload: {
  sessionId: string
  parentRel?: string | null
  name: string
  content?: string | null
}): Promise<SessionWorkspaceFileInfo> {
  return invoke<SessionWorkspaceFileInfo>('session_workspace_create_file', {
    sessionId: payload.sessionId,
    parentRel: payload.parentRel ?? null,
    name: payload.name,
    content: payload.content ?? null,
  })
}

export function sessionWorkspaceCreateDir(payload: {
  sessionId: string
  parentRel?: string | null
  name: string
}): Promise<SessionWorkspaceFileInfo> {
  return invoke<SessionWorkspaceFileInfo>('session_workspace_create_dir', {
    sessionId: payload.sessionId,
    parentRel: payload.parentRel ?? null,
    name: payload.name,
  })
}

export function sessionWorkspaceRename(payload: {
  sessionId: string
  relPath: string
  newName: string
}): Promise<SessionWorkspaceFileInfo> {
  return invoke<SessionWorkspaceFileInfo>('session_workspace_rename', payload)
}

export function sessionWorkspaceDelete(sessionId: string, relPath: string): Promise<void> {
  return invoke('session_workspace_delete', { sessionId, relPath })
}

export function sessionWorkspaceOpenPath(sessionId: string, relPath: string): Promise<void> {
  return invoke('session_workspace_open_path', { sessionId, relPath })
}

export function sessionWorkspaceRevealPath(sessionId: string, relPath: string): Promise<void> {
  return invoke('session_workspace_reveal_path', { sessionId, relPath })
}

export function sessionWorkspaceImportFiles(payload: {
  sessionId: string
  sourcePaths: string[]
  parentRel?: string | null
}): Promise<SessionWorkspaceFileInfo[]> {
  return invoke<SessionWorkspaceFileInfo[]>('session_workspace_import_files', {
    sessionId: payload.sessionId,
    sourcePaths: payload.sourcePaths,
    parentRel: payload.parentRel ?? null,
  })
}
