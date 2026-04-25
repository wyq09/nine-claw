import { describe, expect, it } from 'vitest'
import {
  buildSlashCommandHelp,
  parseSlashCommand,
  SLASH_COMMANDS,
} from '../slashCommands'

describe('parseSlashCommand', () => {
  it('ignores normal chat text', () => {
    expect(parseSlashCommand('hello')).toEqual({ kind: 'none' })
    expect(parseSlashCommand(' /new')).toEqual({ kind: 'none' })
  })

  it('recognizes /new with surrounding whitespace', () => {
    expect(parseSlashCommand('/new')).toEqual({
      kind: 'command',
      command: SLASH_COMMANDS.new,
      args: '',
    })
    expect(parseSlashCommand('/new   ')).toEqual({
      kind: 'command',
      command: SLASH_COMMANDS.new,
      args: '',
    })
  })

  it('recognizes /help and preserves arguments', () => {
    expect(parseSlashCommand('/help new')).toEqual({
      kind: 'command',
      command: SLASH_COMMANDS.help,
      args: 'new',
    })
  })

  it('treats unknown slash commands as actionable command errors', () => {
    expect(parseSlashCommand('/missing')).toEqual({
      kind: 'unknown',
      name: 'missing',
      args: '',
    })
  })

  it('does not treat a bare slash as a command', () => {
    expect(parseSlashCommand('/')).toEqual({ kind: 'none' })
  })
})

describe('buildSlashCommandHelp', () => {
  it('lists supported commands and descriptions', () => {
    const help = buildSlashCommandHelp()

    expect(help).toContain('/new')
    expect(help).toContain('开启一个新的会话')
    expect(help).toContain('/help')
    expect(help).toContain('显示可用指令')
  })
})
