export type WorkspaceRecord = {
  id: string
  name: string
  description: string
  supervisorAgentId: string
  /** 空字符串：默认 `teams/<id>/artifacts`；非空：自定义绝对路径 */
  artifactsRoot: string
  /**
   * 团队会话注入的「主智能体角色」Markdown。空 = 使用应用内置默认（随可委派成员数变化）；
   * 非空则整段写入团队前言（建议以 `## 主智能体角色（MUST）` 开头）。
   */
  supervisorOrchestrationPrompt?: string
  /** 1 = 启用 LLM 调用链调试模式（本工作空间内会把主 Agent↔Pi / 主 Agent↔子 Agent 的完整调用写入 `.debug/*.jsonl`） */
  llmTraceEnabled?: number
  createdAt: number
  updatedAt: number
  archived: number
}

export type ArtifactsTreeEntry = {
  name: string
  relPath: string
  isDir: boolean
  size: number | null
  modifiedMs: number | null
}

export type WorkspaceMemberView = {
  agentId: string
  name: string
  summary: string
  description: string
  role: string
  skillIds: string[]
  avatarUri?: string
}

export type WorkspaceResourceRecord = {
  id: string
  workspaceId: string
  fileName: string
  relPath: string
  mime: string
  size: number
  uploaderAgentId: string | null
  createdAt: number
}

export type WorkspaceMemoryRecord = {
  id: string
  workspaceId: string
  title: string
  content: string
  authorAgentId: string | null
  tagsJson: string
  scope: string
  scopeAgentId: string | null
  createdAt: number
  updatedAt: number
}
