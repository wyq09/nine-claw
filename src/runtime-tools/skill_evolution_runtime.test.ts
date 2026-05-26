import { mkdir, mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { createSkillEvolutionRuntime } from './skill_evolution_runtime.mjs'

async function writePromptFiles(root: string) {
  await mkdir(root, { recursive: true })
  const auto = join(root, 'auto.md')
  const reflect = join(root, 'reflect.md')
  await import('node:fs/promises').then((fs) =>
    Promise.all([
      fs.writeFile(auto, 'SKILL AUTO-CREATION MODE {{turn_count}} {{task_prompt}}', 'utf8'),
      fs.writeFile(reflect, 'SKILL REFLECTION MODE {{skill_name}} {{turn_count}}', 'utf8'),
    ]),
  )
  return { auto, reflect }
}

describe('skill evolution runtime', () => {
  it('runs an isolated subagent and writes cost/log report after threshold', async () => {
    const root = await mkdtemp(join(tmpdir(), 'nineclaw-skill-evolution-'))
    const prompts = await writePromptFiles(root)
    const exec = vi.fn().mockResolvedValue({
      code: 0,
      stdout: JSON.stringify({
        type: 'message_end',
        message: {
          role: 'assistant',
          content: [{ type: 'text', text: 'created skill' }],
          usage: { input: 10, output: 5, cacheRead: 3, cost: { total: 0.12 } },
        },
      }),
      stderr: '',
      killed: false,
    })
    const log = vi.fn()
    const env: Record<string, string> = {
        NINECLAW_AGENT_HOME: root,
        NINECLAW_SKILL_EVOLUTION_AUTO_CREATE_THRESHOLD: '1',
    }
    const processApi = {
      env,
      argv: ['/node', '/pi'],
      execPath: '/node',
      cwd: () => root,
    }
    const runtime = createSkillEvolutionRuntime({
      fsSync: await import('node:fs'),
      fsPromises: await import('node:fs/promises'),
      pathApi: await import('node:path'),
      processApi,
      consoleApi: { log },
      pi: { exec },
      extensionPath: '/tmp/ext.mjs',
      autoCreationPromptPath: prompts.auto,
      reflectionPromptPath: prompts.reflect,
    })

    runtime.reset('do a repeatable task')
    runtime.noteTurnEnd()
    await runtime.maybeRunAfterAgent({ messages: [] }, { cwd: root })

    expect(exec).toHaveBeenCalled()
    expect(log).toHaveBeenCalledWith(expect.stringContaining('nineclaw_skill_evolution_usage'))
    expect(processApi.env.NINECLAW_SKILL_EVOLUTION_SUBAGENT).toBeUndefined()
    const report = await readFile(join(root, '.debug', 'skill-evolution.jsonl'), 'utf8')
    expect(report).toContain('"route":"auto_create"')
    expect(report).toContain('"cost":0.12')

    await rm(root, { recursive: true, force: true })
  })

  it('skips auto creation when a non-system skill was active and skips reflection for default skills', async () => {
    const root = await mkdtemp(join(tmpdir(), 'nineclaw-skill-evolution-source-'))
    const prompts = await writePromptFiles(root)
    const exec = vi.fn().mockResolvedValue({ code: 0, stdout: '', stderr: '' })
    const baseDeps = {
      fsSync: await import('node:fs'),
      fsPromises: await import('node:fs/promises'),
      pathApi: await import('node:path'),
      pi: { exec },
      extensionPath: '/tmp/ext.mjs',
      autoCreationPromptPath: prompts.auto,
      reflectionPromptPath: prompts.reflect,
    }

    const activeUserSkillRuntime = createSkillEvolutionRuntime({
      ...baseDeps,
      processApi: {
        env: {
          NINECLAW_AGENT_HOME: root,
          NINECLAW_SKILL_EVOLUTION_AUTO_CREATE_THRESHOLD: '1',
          NINECLAW_ACTIVE_SKILL_SOURCES_JSON: JSON.stringify({
            custom: { id: 'custom', source: 'user' },
          }),
        },
        argv: ['/node', '/pi'],
        execPath: '/node',
        cwd: () => root,
      },
      consoleApi: { log: vi.fn() },
    })
    activeUserSkillRuntime.reset('repeatable task')
    activeUserSkillRuntime.noteTurnEnd()
    await activeUserSkillRuntime.maybeRunAfterAgent({ messages: [] }, { cwd: root })

    const defaultSkillRuntime = createSkillEvolutionRuntime({
      ...baseDeps,
      processApi: {
        env: {
          NINECLAW_AGENT_HOME: root,
          NINECLAW_SKILL_EVOLUTION_REFLECTION_THRESHOLD: '1',
          NINECLAW_ACTIVE_SKILL_SOURCES_JSON: JSON.stringify({
            system: { id: 'system', source: 'default' },
          }),
        },
        argv: ['/node', '/pi'],
        execPath: '/node',
        cwd: () => root,
      },
      consoleApi: { log: vi.fn() },
    })
    defaultSkillRuntime.reset('/skill:system do it')
    defaultSkillRuntime.noteTurnEnd()
    await defaultSkillRuntime.maybeRunAfterAgent({ messages: [] }, { cwd: root })

    expect(exec).not.toHaveBeenCalled()
    await rm(root, { recursive: true, force: true })
  })
})
