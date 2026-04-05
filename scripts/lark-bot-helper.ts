import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import readline from 'node:readline'
import { randomUUID } from 'node:crypto'
import * as Lark from '@larksuiteoapi/node-sdk'

type ReceiveIdType = 'chat_id' | 'open_id' | 'user_id' | 'union_id' | 'email'

type HelperCommand =
  | {
      type: 'send_text'
      requestId: string
      receiveId: string
      receiveIdType?: ReceiveIdType
      content: string
    }
  | {
      type: 'send_media'
      requestId: string
      receiveId: string
      receiveIdType?: ReceiveIdType
      mediaType: 'image' | 'file' | 'video' | 'audio'
      filePath: string
      fileName?: string
    }
  | {
      type: 'stop'
      requestId?: string
    }

type ParsedArgs = {
  appId: string
  appSecret: string
  channelId: string
  agentLabel: string
}

const STARTUP_TIMEOUT_MS = 15_000

const args = parseArgs(process.argv.slice(2))

const logger = {
  debug: (...parts: unknown[]) => forwardSdkLog('debug', parts),
  info: (...parts: unknown[]) => forwardSdkLog('info', parts),
  warn: (...parts: unknown[]) => forwardSdkLog('warn', parts),
  error: (...parts: unknown[]) => forwardSdkLog('error', parts),
}

const client = new Lark.Client({
  appId: args.appId,
  appSecret: args.appSecret,
  appType: Lark.AppType.SelfBuild,
  domain: Lark.Domain.Feishu,
  logger,
})

const wsClient = new Lark.WSClient({
  appId: args.appId,
  appSecret: args.appSecret,
  logger,
  loggerLevel: Lark.LoggerLevel.info,
})

let shuttingDown = false

function parseArgs(argv: string[]): ParsedArgs {
  const values = new Map<string, string>()
  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index]
    if (!current?.startsWith('--')) {
      continue
    }
    const key = current.slice(2)
    const value = argv[index + 1] ?? ''
    values.set(key, value)
    index += 1
  }

  const appId = values.get('app-id')?.trim() ?? ''
  const appSecret = values.get('app-secret')?.trim() ?? ''
  const channelId = values.get('channel-id')?.trim() ?? 'lark'
  const agentLabel = values.get('agent-label')?.trim() ?? '未绑定智能体'

  if (!appId || !appSecret) {
    throw new Error('missing --app-id or --app-secret')
  }

  return { appId, appSecret, channelId, agentLabel }
}

function emit(payload: Record<string, unknown>): void {
  if (process.stdout.destroyed || !process.stdout.writable) {
    return
  }
  process.stdout.write(`${JSON.stringify(payload)}\n`)
}

function emitStatus(level: 'processing' | 'done' | 'warn' | 'error', message: string): void {
  emit({
    type: 'status',
    level,
    message,
    timestamp: Date.now(),
  })
}

function emitResponse(requestId: string | undefined, ok: boolean, error?: string): void {
  if (!requestId) {
    return
  }
  emit({
    type: 'response',
    requestId,
    ok,
    error,
    timestamp: Date.now(),
  })
}

function forwardSdkLog(level: 'debug' | 'info' | 'warn' | 'error', parts: unknown[]): void {
  const message = parts
    .flatMap((part) => {
      if (Array.isArray(part)) {
        return part.map((value) => stringifyLogPart(value))
      }
      return [stringifyLogPart(part)]
    })
    .filter(Boolean)
    .join(' ')
    .trim()

  if (!message) {
    return
  }

  if (message.includes('ws connect success')) {
    emitStatus('done', `飞书机器人已连接，当前绑定智能体: ${args.agentLabel}`)
    return
  }

  if (message.includes('ws client ready')) {
    emitStatus('done', `飞书长连接已就绪，当前绑定智能体: ${args.agentLabel}`)
    return
  }

  if (message.includes('client closed') || message.includes('closed manually')) {
    emitStatus('warn', '飞书机器人连接已关闭')
    return
  }

  if (level === 'error') {
    emitStatus('error', `飞书 SDK: ${message}`)
    return
  }

  if (level === 'warn') {
    emitStatus('warn', `飞书 SDK: ${message}`)
  }
}

function stringifyLogPart(value: unknown): string {
  if (typeof value === 'string') {
    return value
  }
  if (value instanceof Error) {
    return value.message
  }
  try {
    return JSON.stringify(value)
  } catch {
    return String(value)
  }
}

function ensureOk(response: { code?: number; msg?: string } | null | undefined, action: string): void {
  if (!response) {
    throw new Error(`${action}失败：飞书返回空响应`)
  }
  const code = response.code ?? 0
  if (code !== 0) {
    throw new Error(`${action}失败：${response.msg ?? `code=${code}`}`)
  }
}

function normalizeTextContent(rawContent: string, mentions?: Array<{ key?: string; name?: string }>): string {
  let text = rawContent
  try {
    const parsed = JSON.parse(rawContent) as { text?: string }
    if (typeof parsed?.text === 'string') {
      text = parsed.text
    }
  } catch {
    // ignore invalid JSON
  }

  for (const mention of mentions ?? []) {
    const key = mention.key?.trim()
    if (!key) {
      continue
    }
    text = text.replaceAll(key, '')
    text = text.replaceAll(`@${key}`, '')
  }

  return text.replace(/@?_user_\d+\s*/g, '').replace(/\u00A0/g, ' ').trim()
}

function inferFileType(filePath: string): 'opus' | 'mp4' | 'pdf' | 'doc' | 'xls' | 'ppt' | 'stream' {
  const extension = path.extname(filePath).toLowerCase()
  switch (extension) {
    case '.mp3':
    case '.wav':
    case '.ogg':
    case '.opus':
    case '.amr':
    case '.m4a':
    case '.aac':
      return 'opus'
    case '.mp4':
    case '.mov':
    case '.m4v':
    case '.webm':
      return 'mp4'
    case '.pdf':
      return 'pdf'
    case '.doc':
    case '.docx':
      return 'doc'
    case '.xls':
    case '.xlsx':
    case '.csv':
      return 'xls'
    case '.ppt':
    case '.pptx':
      return 'ppt'
    default:
      return 'stream'
  }
}

function parseContentObject(rawContent: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(rawContent)
    if (parsed && typeof parsed === 'object') {
      return parsed as Record<string, unknown>
    }
  } catch {
    // ignore invalid JSON
  }
  return {}
}

function firstString(...values: Array<unknown>): string {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) {
      return value.trim()
    }
  }
  return ''
}

function inferIncomingAttachmentType(messageType: string): 'image' | 'file' | 'audio' | 'video' | null {
  switch (messageType) {
    case 'image':
      return 'image'
    case 'audio':
      return 'audio'
    case 'media':
    case 'video':
      return 'video'
    case 'file':
      return 'file'
    default:
      return null
  }
}

function inferIncomingFileName(messageType: string, content: Record<string, unknown>): string {
  const directName = firstString(
    content.file_name,
    content.title,
    content.name,
    content.fileKey,
    content.file_key,
    content.image_key
  )
  if (directName) {
    return directName
  }
  switch (messageType) {
    case 'image':
      return 'image.png'
    case 'audio':
      return 'voice.mp3'
    case 'media':
    case 'video':
      return 'video.mp4'
    default:
      return 'attachment.bin'
  }
}

async function downloadMessageAttachment(messageId: string, messageType: string, content: Record<string, unknown>): Promise<{ mediaType: 'image' | 'file' | 'audio' | 'video'; filePath: string; fileName: string; transcript?: string }> {
  const mediaType = inferIncomingAttachmentType(messageType)
  if (!mediaType) {
    throw new Error(`暂不支持下载的飞书消息类型: ${messageType}`)
  }

  const fileKey = firstString(
    content.file_key,
    content.image_key,
    content.fileKey,
    content.imageKey
  )
  if (!fileKey) {
    throw new Error(`飞书消息 ${messageId} 缺少 file_key/image_key`)
  }

  const fileName = inferIncomingFileName(messageType, content)
  const dir = path.join(os.tmpdir(), 'nineclaw-lark-inbox')
  fs.mkdirSync(dir, { recursive: true })
  const tempPath = path.join(dir, `${Date.now()}-${randomUUID()}-${fileName}`)

  const resource = await client.im.v1.messageResource.get({
    params: {
      type: messageType,
    },
    path: {
      message_id: messageId,
      file_key: fileKey,
    },
  })
  await resource.writeFile(tempPath)

  return {
    mediaType,
    filePath: tempPath,
    fileName,
    transcript: firstString(content.text, content.transcript) || undefined,
  }
}

async function handleSendText(command: Extract<HelperCommand, { type: 'send_text' }>): Promise<void> {
  const response = await client.im.v1.message.create({
    params: {
      receive_id_type: command.receiveIdType ?? 'chat_id',
    },
    data: {
      receive_id: command.receiveId,
      msg_type: 'text',
      content: JSON.stringify({ text: command.content }),
      uuid: command.requestId || randomUUID(),
    },
  })
  ensureOk(response, '发送飞书文本消息')
}

async function handleSendMedia(command: Extract<HelperCommand, { type: 'send_media' }>): Promise<void> {
  if (!fs.existsSync(command.filePath)) {
    throw new Error(`媒体文件不存在: ${command.filePath}`)
  }

  if (command.mediaType === 'image') {
    const upload = await client.im.v1.image.create({
      data: {
        image_type: 'message',
        image: fs.createReadStream(command.filePath),
      },
    })
    if (!upload?.image_key) {
      throw new Error('上传飞书图片失败：未返回 image_key')
    }

    const response = await client.im.v1.message.create({
      params: {
        receive_id_type: command.receiveIdType ?? 'chat_id',
      },
      data: {
        receive_id: command.receiveId,
        msg_type: 'image',
        content: JSON.stringify({ image_key: upload.image_key }),
        uuid: command.requestId || randomUUID(),
      },
    })
    ensureOk(response, '发送飞书图片消息')
    return
  }

  const upload = await client.im.v1.file.create({
    data: {
      file_type: inferFileType(command.filePath),
      file_name: command.fileName?.trim() || path.basename(command.filePath),
      file: fs.createReadStream(command.filePath),
    },
  })
  if (!upload?.file_key) {
    throw new Error('上传飞书文件失败：未返回 file_key')
  }

  const response = await client.im.v1.message.create({
    params: {
      receive_id_type: command.receiveIdType ?? 'chat_id',
    },
    data: {
      receive_id: command.receiveId,
      msg_type: 'file',
      content: JSON.stringify({ file_key: upload.file_key }),
      uuid: command.requestId || randomUUID(),
    },
  })
  ensureOk(response, command.mediaType === 'video' ? '发送飞书视频文件' : '发送飞书文件消息')
}

async function handleCommand(command: HelperCommand): Promise<void> {
  switch (command.type) {
    case 'send_text':
      await handleSendText(command)
      return
    case 'send_media':
      await handleSendMedia(command)
      return
    case 'stop':
      await shutdown()
      return
  }
}

async function shutdown(): Promise<void> {
  if (shuttingDown) {
    return
  }
  emit({
    type: 'status',
    level: 'warn',
    message: '飞书机器人辅助进程已停止',
    timestamp: Date.now(),
  })
  shuttingDown = true
  try {
    wsClient.close({ force: true })
  } catch {
    // ignore shutdown failure
  }
  setTimeout(() => process.exit(0), 20)
}

async function main(): Promise<void> {
  emitStatus('processing', `正在启动飞书机器人，绑定智能体: ${args.agentLabel}`)

  const rl = readline.createInterface({
    input: process.stdin,
    crlfDelay: Infinity,
  })

  rl.on('line', async (line) => {
    const trimmed = line.trim()
    if (!trimmed) {
      return
    }

    let command: HelperCommand
    try {
      command = JSON.parse(trimmed) as HelperCommand
    } catch (error) {
      emitStatus('error', `飞书辅助进程收到非法命令: ${String(error)}`)
      return
    }

    try {
      await handleCommand(command)
      emitResponse(command.requestId, true)
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      emitStatus('error', message)
      emitResponse(command.requestId, false, message)
    }
  })

  rl.on('close', () => {
    void shutdown()
  })

  const dispatcher = new Lark.EventDispatcher({}).register({
    'im.message.receive_v1': async (data: {
      sender: { sender_id?: { open_id?: string; user_id?: string; union_id?: string }; sender_type?: string }
      message: {
        message_id: string
        chat_id: string
        chat_type: string
        message_type: string
        content: string
        mentions?: Array<{ key?: string; name?: string }>
      }
    }) => {
      const senderType = data.sender?.sender_type?.toLowerCase() ?? ''
      if (senderType && senderType !== 'user') {
        return
      }

      const senderId =
        data.sender?.sender_id?.open_id ||
        data.sender?.sender_id?.user_id ||
        data.sender?.sender_id?.union_id ||
        ''
      const receiveId = data.message.chat_id
      const chatType = data.message.chat_type || 'unknown'
      const sessionUserId = chatType === 'p2p' ? senderId || receiveId : `${receiveId}:${senderId || 'unknown'}`
      const text = normalizeTextContent(data.message.content, data.message.mentions)
      const content = parseContentObject(data.message.content)
      const attachments: Array<Record<string, unknown>> = []

      if (data.message.message_type !== 'text') {
        try {
          const attachment = await downloadMessageAttachment(
            data.message.message_id,
            data.message.message_type,
            content
          )
          attachments.push(attachment)
        } catch (error) {
          emitStatus('warn', `飞书附件下载失败: ${error instanceof Error ? error.message : String(error)}`)
        }
      }

      if (!text && attachments.length === 0) {
        return
      }

      emit({
        type: 'message',
        sessionUserId,
        senderId: senderId || receiveId,
        receiveId,
        receiveIdType: 'chat_id',
        chatType,
        messageId: data.message.message_id,
        messageType: data.message.message_type,
        text,
        attachments,
        timestamp: Date.now(),
      })
    },
  })

  // The Feishu SDK may hang indefinitely if the long connection never reaches ready.
  const startupTimeout = new Promise<never>((_, reject) => {
    const timer = setTimeout(() => {
      reject(
        new Error(
          `飞书长连接在 ${Math.floor(STARTUP_TIMEOUT_MS / 1000)} 秒内未就绪，请检查开放平台事件订阅和机器人权限`,
        ),
      )
    }, STARTUP_TIMEOUT_MS)
    timer.unref?.()
  })
  await Promise.race([wsClient.start({ eventDispatcher: dispatcher }), startupTimeout])
  emitStatus('done', `飞书机器人长连接已启动，当前绑定智能体: ${args.agentLabel}`)
}

process.on('SIGINT', () => {
  void shutdown()
})

process.on('SIGTERM', () => {
  void shutdown()
})

main().catch((error) => {
  const message = error instanceof Error ? error.message : String(error)
  emitStatus('error', `飞书机器人启动失败: ${message}`)
  process.exit(1)
})
