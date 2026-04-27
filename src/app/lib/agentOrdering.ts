import type { AgentRecord } from '../../types'

/** 将当前默认智能体排在列表首位（用于列表展示；不改变其它项的相对顺序）。 */
export function orderAgentsWithDefaultFirst(agents: AgentRecord[], defaultAgentId: string): AgentRecord[] {
  const id = defaultAgentId?.trim()
  if (!id) {
    return agents
  }
  const idx = agents.findIndex((a) => a.id === id)
  if (idx <= 0) {
    return agents
  }
  const next = agents.slice()
  const [pick] = next.splice(idx, 1)
  return [pick, ...next]
}
