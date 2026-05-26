import { persistChatAttachments } from '../../lib/piClient'
import type { ChatAttachmentUpload, PersistedChatAttachment } from '../../types'

export function buildSessionWorkspaceAttachmentUploads(
  attachments: PersistedChatAttachment[],
): ChatAttachmentUpload[] {
  return attachments.map((attachment) => ({
    fileName: attachment.fileName || 'attachment',
    mimeType: attachment.mimeType || null,
    sourcePath: attachment.filePath,
  }))
}

export async function persistAttachmentsForSessionWorkspace(input: {
  agentId?: string | null
  sessionId: string
  workspaceId?: string | null
  attachments: PersistedChatAttachment[]
  enabled: boolean
}): Promise<PersistedChatAttachment[]> {
  if (!input.enabled || input.attachments.length === 0) {
    return input.attachments
  }

  const agentId = input.agentId?.trim()
  const sessionId = input.sessionId.trim()
  if (!agentId) {
    throw new Error('当前会话没有可写入附件的智能体，请先选择或创建智能体。')
  }
  if (!sessionId) {
    throw new Error('缺少会话 ID，无法写入会话工作区附件。')
  }

  return persistChatAttachments({
    agentId,
    sessionId,
    workspaceId: input.workspaceId?.trim() ? input.workspaceId : null,
    attachments: buildSessionWorkspaceAttachmentUploads(input.attachments),
  })
}
