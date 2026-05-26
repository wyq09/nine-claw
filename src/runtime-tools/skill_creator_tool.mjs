function sanitizeSkillName(value) {
  const raw = String(value || '').trim().toLowerCase()
  const normalized = raw
    .replace(/[^a-z0-9\u4e00-\u9fff._-]+/g, '-')
    .replace(/-+/g, '-')
    .replace(/^[-._]+|[-._]+$/g, '')
  return normalized || `auto-skill-${Date.now()}`
}

function yamlString(value) {
  return JSON.stringify(String(value || '').trim())
}

function firstNonEmpty(...values) {
  for (const value of values) {
    const text = String(value || '').trim()
    if (text) return text
  }
  return ''
}

function defaultSkillRoot(deps, ctx) {
  const explicit = deps.processApi?.env?.NINECLAW_SKILL_CREATOR_ROOT?.trim()
  if (explicit) return explicit
  const cwd = ctx?.cwd || deps.processApi?.cwd?.() || process.cwd()
  return deps.pathApi.join(cwd, '.agents', 'skills')
}

function buildSkillMarkdown(input, skillName) {
  const title = firstNonEmpty(input.title, input.name, skillName)
  const description = firstNonEmpty(
    input.description,
    input.task,
    'Auto-created NineClaw skill.',
  )
  const workflow = firstNonEmpty(input.workflow, input.task, input.instructions)
  const inputShape = firstNonEmpty(input.input, input.inputs, 'User-provided task context.')
  const outputShape = firstNonEmpty(input.output, input.outputs, 'A concise result or modified workspace artifacts.')
  const constraints = firstNonEmpty(input.constraints, 'Keep the workflow general, conservative, and reusable.')

  return [
    '---',
    `name: ${yamlString(title)}`,
    `description: ${yamlString(description)}`,
    '---',
    '',
    `# ${title}`,
    '',
    description,
    '',
    '## When To Use',
    '',
    workflow || '- Use this skill when the same workflow appears again.',
    '',
    '## Inputs',
    '',
    inputShape,
    '',
    '## Workflow',
    '',
    workflow || '- Understand the user request.',
    '- Gather only the context needed for the task.',
    '- Execute the repeatable steps carefully.',
    '- Verify the result before responding.',
    '',
    '## Output',
    '',
    outputShape,
    '',
    '## Constraints',
    '',
    constraints,
    '',
  ].join('\n')
}

async function writeSkillFile(deps, dir, content) {
  await deps.fsPromises.mkdir(dir, { recursive: true })
  const filePath = deps.pathApi.join(dir, 'SKILL.md')
  await deps.fsPromises.writeFile(filePath, content, 'utf8')
  return filePath
}

async function appendSkillReflection(deps, filePath, input) {
  const existing = await deps.fsPromises.readFile(filePath, 'utf8').catch(() => '')
  const update = [
    '',
    '## Auto-Reflection Update',
    '',
    `Recorded at: ${new Date().toISOString()}`,
    '',
    firstNonEmpty(input.improvement, input.task, input.description),
    '',
  ].join('\n')
  await deps.fsPromises.writeFile(filePath, `${existing.trimEnd()}${update}`, 'utf8')
}

export function createSkillCreatorParameters(Type) {
  return Type.Object({
    mode: Type.Optional(
      Type.Union([Type.Literal('quick'), Type.Literal('create'), Type.Literal('update')], {
        description: 'Use quick/create for a new skill, update for improving an existing skill.',
      }),
    ),
    skillName: Type.Optional(Type.String({ description: 'Stable skill id/name, e.g. url-summarizer.' })),
    suggestedName: Type.Optional(Type.String({ description: 'Alternative suggested skill id/name.' })),
    title: Type.Optional(Type.String({ description: 'Human-friendly title.' })),
    description: Type.Optional(Type.String({ description: 'Short description for the SKILL.md frontmatter.' })),
    task: Type.Optional(Type.String({ description: 'What workflow to capture or improve.' })),
    workflow: Type.Optional(Type.String({ description: 'Repeatable step-by-step workflow.' })),
    inputs: Type.Optional(Type.String({ description: 'Expected inputs.' })),
    outputs: Type.Optional(Type.String({ description: 'Expected outputs.' })),
    constraints: Type.Optional(Type.String({ description: 'Important guardrails and limitations.' })),
    improvement: Type.Optional(Type.String({ description: 'Concrete change to append when updating an existing skill.' })),
  })
}

export function createSkillCreatorTool(deps) {
  return {
    name: 'skill-creator',
    label: 'Skill Creator',
    description:
      'Create or conservatively update a NineClaw skill file. Use mode "quick" to create a reusable skill from a clear workflow.',
    promptSnippet: 'Create or update a SKILL.md file from a reusable workflow.',
    promptGuidelines: [
      'Only create a skill when the workflow is reusable, well-defined, valuable, and generalizable.',
      'Prefer mode "quick" with a concise workflow, inputs, outputs, and constraints.',
      'Use mode "update" only for concrete, actionable improvements to an existing non-system skill.',
    ],
    parameters: createSkillCreatorParameters(deps.Type),
    async execute(_toolCallId, input, _signal, _onUpdate, ctx) {
      const mode = String(input?.mode || 'quick').trim()
      const skillName = sanitizeSkillName(input?.skillName || input?.suggestedName || input?.title || input?.description)
      const root = defaultSkillRoot(deps, ctx)
      const dir = deps.pathApi.join(root, skillName)
      const filePath = deps.pathApi.join(dir, 'SKILL.md')

      if (mode === 'update') {
        const exists = deps.fsSync.existsSync(filePath)
        if (!exists) {
          return {
            content: [{ type: 'text', text: `Skill ${skillName} does not exist at ${filePath}; update skipped.` }],
            details: { ok: false, reason: 'missing_skill', path: filePath, skillName },
          }
        }
        await appendSkillReflection(deps, filePath, input || {})
        return {
          content: [{ type: 'text', text: `Updated skill ${skillName}: ${filePath}` }],
          details: { ok: true, mode: 'update', path: filePath, skillName },
        }
      }

      const content = buildSkillMarkdown(input || {}, skillName)
      const writtenPath = await writeSkillFile(deps, dir, content)
      return {
        content: [{ type: 'text', text: `Created skill ${skillName}: ${writtenPath}` }],
        details: { ok: true, mode: 'create', path: writtenPath, skillName },
      }
    },
  }
}
