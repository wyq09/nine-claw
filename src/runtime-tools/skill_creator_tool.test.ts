import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { Type } from 'typebox'
import { describe, expect, it } from 'vitest'
import { createSkillCreatorTool } from './skill_creator_tool.mjs'

function deps(root: string) {
  return {
    Type,
    fsSync: { existsSync: () => true },
    fsPromises: { mkdir, writeFile, readFile },
    pathApi: { join },
    processApi: { env: { NINECLAW_SKILL_CREATOR_ROOT: root } },
  }
}

describe('skill-creator runtime tool', () => {
  it('creates a SKILL.md under the configured skill root', async () => {
    const root = await mkdtemp(join(tmpdir(), 'nineclaw-skill-create-'))
    try {
      const tool = createSkillCreatorTool(deps(root))
      const result = await tool.execute('call-1', {
        mode: 'quick',
        suggestedName: 'URL Summarizer',
        description: 'Summarize URL content.',
        workflow: '1. Fetch URL\n2. Extract main text\n3. Summarize',
        inputs: 'URL',
        outputs: 'Markdown summary',
      })

      const path = result.details.path
      expect(path).toBe(join(root, 'url-summarizer', 'SKILL.md'))
      const content = await readFile(path, 'utf8')
      expect(content).toContain('description: "Summarize URL content."')
      expect(content).toContain('## Workflow')
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })
})
