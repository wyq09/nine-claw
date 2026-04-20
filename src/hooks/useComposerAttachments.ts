import { useCallback, useEffect, useRef, useState } from 'react'
import type { ChangeEvent, ClipboardEvent as ReactClipboardEvent } from 'react'
import { persistChatAttachments } from '../lib/piClient'
import type { PersistedChatAttachment } from '../types'
import {
  buildAttachmentUploadFromFile,
  buildClipboardPathUpload,
  extractClipboardFilePathCandidates,
} from '../lib/composerAttachments'

type UseComposerAttachmentsOptions = {
  agentId: string
  sessionId?: string | null
  workspaceId?: string | null
  scopeKey: string
}

export function useComposerAttachments({
  agentId,
  sessionId,
  workspaceId,
  scopeKey,
}: UseComposerAttachmentsOptions) {
  const [attachments, setAttachments] = useState<PersistedChatAttachment[]>([])
  const [uploading, setUploading] = useState(false)
  const [error, setError] = useState('')
  const fileInputRef = useRef<HTMLInputElement | null>(null)

  useEffect(() => {
    setAttachments([])
    setError('')
    if (fileInputRef.current) {
      fileInputRef.current.value = ''
    }
  }, [scopeKey, agentId, workspaceId])

  const persistUploads = useCallback(
    async (uploads: Parameters<typeof persistChatAttachments>[0]['attachments']) => {
      if (uploads.length === 0) {
        return
      }
      const trimmedAgentId = agentId.trim()
      if (!trimmedAgentId) {
        setError('当前会话没有可写入附件的智能体，请先选择或创建智能体。')
        return
      }

      setUploading(true)
      setError('')
      try {
        const persisted = await persistChatAttachments({
          agentId: trimmedAgentId,
          sessionId: sessionId ?? null,
          workspaceId: workspaceId?.trim() ? workspaceId : null,
          attachments: uploads,
        })
        setAttachments((previous) => {
          const deduped = new Map(previous.map((item) => [item.filePath, item]))
          for (const item of persisted) {
            deduped.set(item.filePath, item)
          }
          return [...deduped.values()]
        })
      } catch (uploadError) {
        const message = uploadError instanceof Error ? uploadError.message : String(uploadError)
        setError(message)
      } finally {
        setUploading(false)
      }
    },
    [agentId, sessionId, workspaceId],
  )

  const handleFileInputChange = useCallback(
    async (event: ChangeEvent<HTMLInputElement>) => {
      const files = Array.from(event.target.files ?? [])
      event.target.value = ''
      if (files.length === 0) {
        return
      }

      const uploads = await Promise.all(files.map((file) => buildAttachmentUploadFromFile(file)))
      await persistUploads(uploads)
    },
    [persistUploads],
  )

  const handleComposerPaste = useCallback(
    async (event: ReactClipboardEvent<HTMLTextAreaElement>) => {
      if (uploading) {
        return
      }

      const pastedFiles = Array.from(event.clipboardData.files ?? [])
      const pastedPaths = extractClipboardFilePathCandidates(event.clipboardData)
      if (pastedFiles.length === 0 && pastedPaths.length === 0) {
        return
      }

      event.preventDefault()
      const uploads = [
        ...(await Promise.all(pastedFiles.map((file) => buildAttachmentUploadFromFile(file)))),
        ...pastedPaths.map((path) => buildClipboardPathUpload(path)),
      ]
      await persistUploads(uploads)
    },
    [persistUploads, uploading],
  )

  const openFilePicker = useCallback(() => {
    setError('')
    fileInputRef.current?.click()
  }, [])

  const removeAttachment = useCallback((attachmentId: string) => {
    setAttachments((previous) => previous.filter((item) => item.id !== attachmentId))
  }, [])

  const clearAttachments = useCallback(() => {
    setAttachments([])
  }, [])

  return {
    attachments,
    uploading,
    error,
    fileInputRef,
    openFilePicker,
    handleFileInputChange,
    handleComposerPaste,
    removeAttachment,
    clearAttachments,
    clearError: () => setError(''),
  }
}
