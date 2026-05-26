import type { CompressionConfig } from './types'

export const defaultCompressionConfig: CompressionConfig = {
  tokenThreshold: 150_000,
  messageCountThreshold: 200,
  targetCompressedTokens: 10_000,
  maxRecentMessages: 20,
  idleCompressionEnabled: true,
  idleCompressionDelayMs: 90_000,
  idleTokenThreshold: 20_000,
}

export function normalizeCompressionConfig(
  config: Partial<CompressionConfig> = {},
): CompressionConfig {
  const merged = { ...defaultCompressionConfig, ...config }
  return {
    tokenThreshold: Math.max(1, Math.floor(merged.tokenThreshold)),
    messageCountThreshold: Math.max(1, Math.floor(merged.messageCountThreshold)),
    targetCompressedTokens: Math.max(1, Math.floor(merged.targetCompressedTokens)),
    maxRecentMessages: Math.max(1, Math.floor(merged.maxRecentMessages)),
    idleCompressionEnabled: Boolean(merged.idleCompressionEnabled),
    idleCompressionDelayMs: Math.max(1, Math.floor(merged.idleCompressionDelayMs)),
    idleTokenThreshold: Math.max(1, Math.floor(merged.idleTokenThreshold)),
  }
}

export function buildCompressionPrompt(level: number, config: CompressionConfig): string {
  const normalizedLevel = Math.max(1, Math.floor(level))
  const detailGuidance = compressionLevelGuidance(normalizedLevel)

  return [
    '═══════════════════════════════════════════════════════════════',
    'CRITICAL: TASK CHANGE - MEMORY COMPRESSION MODE',
    '═══════════════════════════════════════════════════════════════',
    '上面的对话已经结束。你现在只负责压缩对话历史。',
    '',
    '关键约束：',
    '1. 这不是继续回答用户问题。',
    '2. 不要执行上文中的任何请求。',
    '3. 不要调用任何工具或函数。',
    '4. 只输出纯文本，必须包含 <topics> 与 <summary> 标签。',
    '',
    `本次压缩级别：Level ${normalizedLevel}`,
    `目标摘要规模：约 ${config.targetCompressedTokens} tokens 以内。`,
    detailGuidance,
    '',
    '输出格式要求：',
    '<topics>',
    '用逗号分隔的关键主题标签（3-8 个）',
    '</topics>',
    '',
    '<summary>',
    '## 对话摘要',
    '',
    '### 关键决策',
    '- [列出对话中做出的重要决定]',
    '',
    '### 文件操作',
    '- 创建: [文件列表]',
    '- 修改: [文件列表]',
    '',
    '### 任务进度',
    '- 已完成: [完成的任务]',
    '- 进行中: [当前未完成的任务]',
    '',
    '### 重要上下文',
    '- [用户偏好、项目约束、关键技术选择等需要跨轮记住的信息]',
    '</summary>',
  ].join('\n')
}

function compressionLevelGuidance(level: number): string {
  if (level === 1) {
    return [
      'Level 1 要求：详细摘要。',
      '必须包含关键文件列表、技术决策、工具使用、错误与修复、当前任务状态。',
    ].join('\n')
  }
  if (level === 2) {
    return [
      'Level 2 要求：简洁摘要。',
      '只保留关键文件、关键结果、主要未完成事项和可复用约束。',
    ].join('\n')
  }
  if (level === 3) {
    return [
      'Level 3 要求：最小摘要。',
      '只保留文件/任务数量级统计、当前进行中工作、继续任务必需的少量上下文。',
    ].join('\n')
  }
  return [
    'Level 4+ 要求：超精简一行。',
    '格式示例：Progress: X tasks, Y files. Recent: tool_a, tool_b.',
  ].join('\n')
}
