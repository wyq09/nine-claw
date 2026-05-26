import { buildSkillReflectionPrompt } from './skill-evolution-prompts'
import type { SkillEvolutionConfig, SkillEvolutionRunContext, SkillEvolutionRoute } from './types'

export function shouldReflectOnSkill(
  context: SkillEvolutionRunContext,
  config: SkillEvolutionConfig,
): boolean {
  const skillContext = context.skillExecutionContext
  if (!config.enabled || !skillContext || context.isSubAgent) return false
  if (!skillContext.explicitInvocation) return false
  if (config.excludedSources.includes(skillContext.source)) return false
  if (context.taskStatus !== 'success') return false
  return skillContext.endIteration - skillContext.startIteration >= config.reflectionThreshold
}

export function routeSkillReflector(
  context: SkillEvolutionRunContext,
  config: SkillEvolutionConfig,
): SkillEvolutionRoute {
  const skillContext = context.skillExecutionContext
  if (!config.enabled) return { type: 'skip', reason: 'disabled' }
  if (!skillContext) return { type: 'skip', reason: 'no_skill_context' }
  if (context.isSubAgent) return { type: 'skip', reason: 'sub_agent' }
  if (context.taskStatus !== 'success') return { type: 'skip', reason: 'task_not_successful' }
  if (!skillContext.explicitInvocation) return { type: 'skip', reason: 'not_explicit_invocation' }
  if (config.excludedSources.includes(skillContext.source)) return { type: 'skip', reason: 'excluded_source' }
  if (skillContext.endIteration - skillContext.startIteration < config.reflectionThreshold) {
    return { type: 'skip', reason: 'below_reflection_threshold' }
  }
  return {
    type: 'reflect',
    skillName: skillContext.skillName,
    prompt: buildSkillReflectionPrompt(skillContext.skillName),
  }
}
