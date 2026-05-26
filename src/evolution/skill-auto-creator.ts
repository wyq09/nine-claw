import { buildSkillAutoCreationPrompt } from './skill-evolution-prompts'
import type { SkillEvolutionConfig, SkillEvolutionRunContext, SkillEvolutionRoute } from './types'

export function shouldAutoCreateSkill(
  context: SkillEvolutionRunContext,
  config: SkillEvolutionConfig,
): boolean {
  return (
    config.enabled &&
    context.taskIterations >= config.autoCreateThreshold &&
    !context.skillInvokedInHistory &&
    !context.skillExecutionContext &&
    !context.isSubAgent &&
    context.taskStatus === 'success'
  )
}

export function routeSkillAutoCreator(
  context: SkillEvolutionRunContext,
  config: SkillEvolutionConfig,
): SkillEvolutionRoute {
  if (!config.enabled) return { type: 'skip', reason: 'disabled' }
  if (context.isSubAgent) return { type: 'skip', reason: 'sub_agent' }
  if (context.taskStatus !== 'success') return { type: 'skip', reason: 'task_not_successful' }
  if (context.skillInvokedInHistory) return { type: 'skip', reason: 'skill_invoked' }
  if (context.skillExecutionContext) return { type: 'skip', reason: 'skill_context_present' }
  if (context.taskIterations < config.autoCreateThreshold) return { type: 'skip', reason: 'below_auto_create_threshold' }
  return { type: 'auto_create', prompt: buildSkillAutoCreationPrompt() }
}
