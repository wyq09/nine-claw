export type SkillEvolutionConfig = {
  enabled: boolean
  autoCreateThreshold: number
  reflectionThreshold: number
  excludedSources: string[]
}

export type SkillExecutionContext = {
  skillName: string
  source: string
  explicitInvocation: boolean
  startIteration: number
  endIteration: number
}

export type SkillEvolutionRunContext = {
  taskIterations: number
  skillInvokedInHistory: boolean
  isSubAgent: boolean
  taskStatus: 'success' | 'error' | 'aborted'
  skillExecutionContext?: SkillExecutionContext | null
}

export type SkillEvolutionRoute =
  | { type: 'skip'; reason: string }
  | { type: 'auto_create'; prompt: string }
  | { type: 'reflect'; skillName: string; prompt: string }

export type ForkedAgentResult = {
  status: 'success' | 'skipped' | 'error'
  summary?: string
  totalCostUSD?: number
}

export type ForkSubAgent = (prompt: string, mode: 'skill_auto_create' | 'skill_reflect') => Promise<ForkedAgentResult>

export type SkillEvolutionHookResult = {
  route: SkillEvolutionRoute
  subAgentResult?: ForkedAgentResult
  addedCostUSD: number
}
