import { routeSkillAutoCreator } from './skill-auto-creator'
import { routeSkillReflector } from './skill-reflector'
import type {
  ForkSubAgent,
  SkillEvolutionConfig,
  SkillEvolutionHookResult,
  SkillEvolutionRoute,
  SkillEvolutionRunContext,
} from './types'

export const defaultSkillEvolutionConfig: SkillEvolutionConfig = {
  enabled: true,
  autoCreateThreshold: 12,
  reflectionThreshold: 5,
  excludedSources: ['default', 'brand'],
}

export function normalizeSkillEvolutionConfig(
  config: Partial<SkillEvolutionConfig> = {},
): SkillEvolutionConfig {
  const merged = { ...defaultSkillEvolutionConfig, ...config }
  return {
    enabled: merged.enabled !== false,
    autoCreateThreshold: Math.max(1, Math.floor(merged.autoCreateThreshold)),
    reflectionThreshold: Math.max(1, Math.floor(merged.reflectionThreshold)),
    excludedSources: [...new Set(merged.excludedSources.map((item) => item.trim()).filter(Boolean))],
  }
}

export function routeSkillEvolution(
  context: SkillEvolutionRunContext,
  configInput: Partial<SkillEvolutionConfig> = {},
): SkillEvolutionRoute {
  const config = normalizeSkillEvolutionConfig(configInput)
  if (!config.enabled) return { type: 'skip', reason: 'disabled' }
  if (context.isSubAgent) return { type: 'skip', reason: 'sub_agent' }

  if (context.skillExecutionContext) {
    return routeSkillReflector(context, config)
  }
  return routeSkillAutoCreator(context, config)
}

export async function runSkillEvolutionHooks(options: {
  context: SkillEvolutionRunContext
  config?: Partial<SkillEvolutionConfig>
  forkSubAgent: ForkSubAgent
}): Promise<SkillEvolutionHookResult> {
  const route = routeSkillEvolution(options.context, options.config)
  if (route.type === 'skip') {
    return { route, addedCostUSD: 0 }
  }

  const mode = route.type === 'auto_create' ? 'skill_auto_create' : 'skill_reflect'
  const rawResult = await options.forkSubAgent(route.prompt, mode)
  const subAgentResult = {
    status: rawResult.status,
    summary: rawResult.summary,
    totalCostUSD: rawResult.totalCostUSD,
  }
  return {
    route,
    subAgentResult,
    addedCostUSD: subAgentResult.totalCostUSD ?? 0,
  }
}
