function readFileSafe(fsSync, filePath) {
  try {
    return fsSync.readFileSync(filePath, 'utf8')
  } catch {
    return ''
  }
}

function envBool(processApi, name, fallback) {
  const raw = processApi?.env?.[name]
  if (raw == null || String(raw).trim() === '') return fallback
  return !/^(0|false|no|off)$/i.test(String(raw).trim())
}

function envNumber(processApi, name, fallback) {
  const value = Number(processApi?.env?.[name])
  return Number.isFinite(value) && value > 0 ? Math.floor(value) : fallback
}

function envList(processApi, name, fallback) {
  const raw = processApi?.env?.[name]
  if (raw == null || String(raw).trim() === '') return fallback
  return String(raw)
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean)
}

function parseSkillSourceMap(processApi) {
  const raw = processApi?.env?.NINECLAW_ACTIVE_SKILL_SOURCES_JSON
  if (!raw) return {}
  try {
    const parsed = JSON.parse(raw)
    if (!parsed || typeof parsed !== 'object') return {}
    const out = {}
    for (const [key, value] of Object.entries(parsed)) {
      if (!key || !value || typeof value !== 'object') continue
      out[key.toLowerCase()] = {
        id: String(value.id || key),
        source: String(value.source || 'user'),
        sourceType: String(value.sourceType || ''),
      }
    }
    return out
  } catch {
    return {}
  }
}

function textFromMessage(message) {
  const parts = []
  for (const block of Array.isArray(message?.content) ? message.content : []) {
    if (block?.type === 'text' && block.text) parts.push(block.text)
  }
  return parts.join('\n').trim()
}

function assistantExcerpt(messages) {
  return (Array.isArray(messages) ? messages : [])
    .map(textFromMessage)
    .filter(Boolean)
    .slice(-4)
    .join('\n\n')
    .slice(-6000)
}

function parseExplicitSkill(prompt) {
  const text = String(prompt || '').trim()
  const slash = /^\/skill:([a-zA-Z0-9._-]+)/.exec(text)
  if (slash) return slash[1]
  const named = /(?:使用|执行|调用)\s*(?:skill|技能)\s*[:：]?\s*([a-zA-Z0-9._-]+)/i.exec(text)
  return named?.[1] || null
}

function isExcludedSkillSource(source, excludedSources) {
  return excludedSources.includes(String(source || '').trim().toLowerCase())
}

function skillSourceForName(skillName, skillSourceMap) {
  const key = String(skillName || '').trim().toLowerCase()
  if (!key) return 'user'
  return skillSourceMap[key]?.source || 'user'
}

function hasErrorMessage(messages) {
  return (Array.isArray(messages) ? messages : []).some((message) => {
    if (message?.stopReason === 'error' || message?.stopReason === 'aborted') return true
    if (message?.errorMessage) return true
    return false
  })
}

function renderTemplate(template, vars) {
  let out = template
  for (const [key, value] of Object.entries(vars)) {
    out = out.replaceAll(`{{${key}}}`, String(value ?? ''))
  }
  return out
}

function getPiInvocation(processApi, args) {
  const currentScript = processApi?.argv?.[1]
  if (currentScript && !currentScript.startsWith('/$bunfs/root/')) {
    return { command: processApi.execPath, args: [currentScript, ...args] }
  }
  return { command: 'pi', args }
}

function parseJsonModeOutput(stdout) {
  const usage = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, cost: 0, turns: 0 }
  let finalText = ''
  for (const rawLine of String(stdout || '').split(/\r?\n/)) {
    if (!rawLine.trim()) continue
    let event
    try {
      event = JSON.parse(rawLine)
    } catch {
      continue
    }
    if (event.type !== 'message_end' || !event.message) continue
    const text = textFromMessage(event.message)
    if (event.message.role === 'assistant') {
      usage.turns += 1
      finalText = text || finalText
      const u = event.message.usage
      if (u) {
        usage.input += u.input || u.inputTokens || 0
        usage.output += u.output || u.outputTokens || 0
        usage.cacheRead += u.cacheRead || u.cache_read_input_tokens || 0
        usage.cacheWrite += u.cacheWrite || u.cache_creation_input_tokens || 0
        usage.cost += u.cost?.total || u.cost || 0
      }
    }
  }
  return { finalText, usage }
}

async function runSubagent(deps, prompt, ctx, timeoutMs) {
  const extensionPath = deps.extensionPath
  const args = ['--mode', 'json', '-p', '--no-session']
  if (extensionPath) args.push('--extension', extensionPath)
  args.push('--tools', 'skill-creator,read,write,edit,ls,grep,find,bash')
  args.push(prompt)

  const invocation = getPiInvocation(deps.processApi, args)
  const previousSubagentFlag = deps.processApi?.env?.NINECLAW_SKILL_EVOLUTION_SUBAGENT
  if (deps.processApi?.env) deps.processApi.env.NINECLAW_SKILL_EVOLUTION_SUBAGENT = '1'
  let result
  try {
    result = await deps.pi.exec(invocation.command, invocation.args, {
      cwd: ctx?.cwd || deps.processApi?.cwd?.(),
      timeout: timeoutMs,
    })
  } finally {
    if (deps.processApi?.env) {
      if (previousSubagentFlag == null) delete deps.processApi.env.NINECLAW_SKILL_EVOLUTION_SUBAGENT
      else deps.processApi.env.NINECLAW_SKILL_EVOLUTION_SUBAGENT = previousSubagentFlag
    }
  }
  const parsed = parseJsonModeOutput(result.stdout)
  return {
    ok: result.code === 0,
    code: result.code,
    stderr: result.stderr,
    output: parsed.finalText,
    usage: parsed.usage,
  }
}

async function appendReport(deps, record) {
  const home = deps.processApi?.env?.NINECLAW_AGENT_HOME
  if (!home) return
  const dir = deps.pathApi.join(home, '.debug')
  await deps.fsPromises.mkdir(dir, { recursive: true })
  const file = deps.pathApi.join(dir, 'skill-evolution.jsonl')
  await deps.fsPromises.appendFile(file, `${JSON.stringify(record)}\n`, 'utf8')
}

function emitUsageLine(deps, route, usage) {
  const payload = {
    type: 'nineclaw_skill_evolution_usage',
    route,
    usage: {
      input: usage?.input || 0,
      output: usage?.output || 0,
      cacheRead: usage?.cacheRead || 0,
      cacheWrite: usage?.cacheWrite || 0,
      totalTokens:
        (usage?.input || 0) + (usage?.output || 0) + (usage?.cacheRead || 0) + (usage?.cacheWrite || 0),
    },
    cost: usage?.cost || 0,
  }
  deps.consoleApi?.log?.(JSON.stringify(payload))
}

export function createSkillEvolutionRuntime(deps) {
  const config = {
    enabled: envBool(deps.processApi, 'NINECLAW_SKILL_EVOLUTION_ENABLED', true),
    autoCreateThreshold: envNumber(deps.processApi, 'NINECLAW_SKILL_EVOLUTION_AUTO_CREATE_THRESHOLD', 12),
    reflectionThreshold: envNumber(deps.processApi, 'NINECLAW_SKILL_EVOLUTION_REFLECTION_THRESHOLD', 5),
    excludedSources: envList(deps.processApi, 'NINECLAW_SKILL_EVOLUTION_EXCLUDED_SOURCES', ['default', 'brand'])
      .map((item) => item.toLowerCase()),
    timeoutMs: envNumber(deps.processApi, 'NINECLAW_SKILL_EVOLUTION_TIMEOUT_MS', 60000),
  }
  const autoTemplate = readFileSafe(deps.fsSync, deps.autoCreationPromptPath)
  const reflectionTemplate = readFileSafe(deps.fsSync, deps.reflectionPromptPath)
  let state = {
    prompt: '',
    turnCount: 0,
    toolNames: new Set(),
    explicitSkillName: null,
    activeSkillIds: [],
    activeSkillSources: {},
    running: false,
  }

  function reset(prompt) {
    const activeSkillSources = parseSkillSourceMap(deps.processApi)
    state = {
      prompt: String(prompt || ''),
      turnCount: 0,
      toolNames: new Set(),
      explicitSkillName: parseExplicitSkill(prompt),
      activeSkillIds: Object.keys(activeSkillSources),
      activeSkillSources,
      running: false,
    }
  }

  function noteToolCall(event) {
    const name = String(event?.toolName || '')
    if (name) state.toolNames.add(name)
  }

  function noteTurnEnd() {
    state.turnCount += 1
  }

  async function maybeRunAfterAgent(event, ctx) {
    if (!config.enabled || state.running) return
    if (deps.processApi?.env?.NINECLAW_SKILL_EVOLUTION_SUBAGENT === '1') return
    const taskStatus = hasErrorMessage(event?.messages) ? 'error' : 'success'
    if (taskStatus !== 'success') return

    const toolNames = Array.from(state.toolNames)
    const excerpt = assistantExcerpt(event?.messages)
    let route = null
    let prompt = ''

    if (state.explicitSkillName) {
      const source = skillSourceForName(state.explicitSkillName, state.activeSkillSources)
      if (isExcludedSkillSource(source, config.excludedSources)) return
      if (state.turnCount < config.reflectionThreshold) return
      prompt = renderTemplate(reflectionTemplate, {
        skill_name: state.explicitSkillName,
        turn_count: state.turnCount,
        tool_names: toolNames.join(', ') || 'none',
        conversation_excerpt: excerpt,
      })
      route = 'reflect'
    } else {
      const activeUserSkill = state.activeSkillIds.some((id) => {
        const source = state.activeSkillSources[id]?.source || 'user'
        return !isExcludedSkillSource(source, config.excludedSources)
      })
      const usedSkill = activeUserSkill || toolNames.includes('invoke_skill') || toolNames.includes('skill-creator')
      if (usedSkill || state.turnCount < config.autoCreateThreshold) return
      prompt = renderTemplate(autoTemplate, {
        task_prompt: state.prompt,
        turn_count: state.turnCount,
        tool_names: toolNames.join(', ') || 'none',
        conversation_excerpt: excerpt,
      })
      route = 'auto_create'
    }

    state.running = true
    const startedAt = Date.now()
    try {
      const result = await runSubagent(deps, prompt, ctx, config.timeoutMs)
      emitUsageLine(deps, route, result.usage)
      await appendReport(deps, {
        kind: 'skill_evolution',
        route,
        sessionId: deps.processApi?.env?.NINECLAW_SESSION_ID || null,
        skillName: state.explicitSkillName,
        turnCount: state.turnCount,
        toolNames,
        ok: result.ok,
        output: result.output,
        stderr: result.stderr,
        usage: result.usage,
        durationMs: Date.now() - startedAt,
        createdAt: new Date().toISOString(),
      })
    } catch (error) {
      await appendReport(deps, {
        kind: 'skill_evolution',
        route,
        sessionId: deps.processApi?.env?.NINECLAW_SESSION_ID || null,
        skillName: state.explicitSkillName,
        turnCount: state.turnCount,
        ok: false,
        error: error?.message || String(error),
        durationMs: Date.now() - startedAt,
        createdAt: new Date().toISOString(),
      })
    } finally {
      state.running = false
    }
  }

  return {
    reset,
    noteToolCall,
    noteTurnEnd,
    maybeRunAfterAgent,
  }
}
