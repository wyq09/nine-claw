import { invoke } from '@tauri-apps/api/core'

export async function generateSessionConversationTitle(
  agentId: string,
  sessionId: string,
  userMessage: string,
): Promise<string> {
  return invoke<string>('generate_session_conversation_title', {
    agentId,
    sessionId,
    userMessage,
  })
}
