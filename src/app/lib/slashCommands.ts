export type SlashCommandId = 'new' | 'help'

export type SlashCommandDefinition = {
  id: SlashCommandId
  usage: `/${string}`
  description: string
}

export const SLASH_COMMANDS = {
  new: {
    id: 'new',
    usage: '/new',
    description: '开启一个新的会话',
  },
  help: {
    id: 'help',
    usage: '/help',
    description: '显示可用指令',
  },
} as const satisfies Record<SlashCommandId, SlashCommandDefinition>

const SLASH_COMMAND_LIST = [SLASH_COMMANDS.new, SLASH_COMMANDS.help] as const

export type SlashCommandParseResult =
  | { kind: 'none' }
  | { kind: 'command'; command: SlashCommandDefinition; args: string }
  | { kind: 'unknown'; name: string; args: string }

export function parseSlashCommand(input: string): SlashCommandParseResult {
  if (!input.startsWith('/')) {
    return { kind: 'none' }
  }

  const trimmed = input.trim()
  if (trimmed === '/') {
    return { kind: 'none' }
  }

  const match = /^\/([A-Za-z][\w-]*)(?:\s+(.*))?$/.exec(trimmed)
  if (!match) {
    return { kind: 'unknown', name: trimmed.slice(1), args: '' }
  }

  const [, rawName, rawArgs = ''] = match
  const name = rawName.toLowerCase()
  const command = SLASH_COMMAND_LIST.find((item) => item.id === name)
  if (!command) {
    return { kind: 'unknown', name, args: rawArgs.trim() }
  }

  return { kind: 'command', command, args: rawArgs.trim() }
}

export function buildSlashCommandHelp(): string {
  return SLASH_COMMAND_LIST.map((command) => `${command.usage} - ${command.description}`).join('\n')
}
