import { describe, expect, it, vi } from 'vitest'
import { routeSkillEvolution, runSkillEvolutionHooks } from '../skill-evolution'
import type { SkillEvolutionRunContext } from '../types'

const successContext: SkillEvolutionRunContext = {
  taskIterations: 12,
  skillInvokedInHistory: false,
  isSubAgent: false,
  taskStatus: 'success',
}

describe('skill evolution routing', () => {
  it('B1 routes to reflection when skill execution context exists', () => {
    const route = routeSkillEvolution({
      ...successContext,
      skillExecutionContext: {
        skillName: 'custom-skill',
        source: 'user',
        explicitInvocation: true,
        startIteration: 2,
        endIteration: 7,
      },
    })

    expect(route.type).toBe('reflect')
    expect(route.type === 'reflect' ? route.prompt : '').toContain('SKILL REFLECTION MODE')
  })

  it('B2 triggers auto creation for long successful main-agent tasks without skills', () => {
    const route = routeSkillEvolution(successContext)

    expect(route.type).toBe('auto_create')
    expect(route.type === 'auto_create' ? route.prompt : '').toContain('SKILL AUTO-CREATION MODE')
  })

  it('B3 skips auto creation for short, failed, sub-agent, or skill-invoking tasks', () => {
    expect(routeSkillEvolution({ ...successContext, taskIterations: 11 }).type).toBe('skip')
    expect(routeSkillEvolution({ ...successContext, taskStatus: 'error' }).type).toBe('skip')
    expect(routeSkillEvolution({ ...successContext, isSubAgent: true }).type).toBe('skip')
    expect(routeSkillEvolution({ ...successContext, skillInvokedInHistory: true }).type).toBe('skip')
  })

  it('B4 triggers reflection for explicit non-system skill executions over threshold', () => {
    const route = routeSkillEvolution({
      ...successContext,
      taskIterations: 2,
      skillExecutionContext: {
        skillName: 'reader',
        source: 'user',
        explicitInvocation: true,
        startIteration: 1,
        endIteration: 6,
      },
    })

    expect(route.type).toBe('reflect')
  })

  it('B5 skips reflection for system skills, implicit skills, and short skill runs', () => {
    const baseSkill = {
      skillName: 'reader',
      source: 'user',
      explicitInvocation: true,
      startIteration: 1,
      endIteration: 6,
    }
    expect(routeSkillEvolution({ ...successContext, skillExecutionContext: { ...baseSkill, source: 'default' } }).type).toBe('skip')
    expect(routeSkillEvolution({ ...successContext, skillExecutionContext: { ...baseSkill, explicitInvocation: false } }).type).toBe('skip')
    expect(routeSkillEvolution({ ...successContext, skillExecutionContext: { ...baseSkill, endIteration: 5 } }).type).toBe('skip')
  })

  it('B7 aggregates sub-agent cost without exposing intermediate steps', async () => {
    const forkSubAgent = vi.fn().mockResolvedValue({
      status: 'success',
      summary: 'created skill',
      totalCostUSD: 0.42,
      hiddenMessages: ['not part of public contract'],
    })

    const result = await runSkillEvolutionHooks({
      context: successContext,
      forkSubAgent,
    })

    expect(forkSubAgent).toHaveBeenCalledWith(expect.stringContaining('SKILL AUTO-CREATION MODE'), 'skill_auto_create')
    expect(result.addedCostUSD).toBe(0.42)
    expect(result.subAgentResult).not.toHaveProperty('hiddenMessages')
  })
})
