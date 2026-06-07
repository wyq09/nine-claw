import { fireEvent, render, screen, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { AgentEditorConfiguration } from './AgentEditorConfiguration'
import type { AgentInput, InstalledSkillItem } from '../../types'

const baseAgentDraft: AgentInput = {
  id: '',
  name: 'Alice',
  summary: '日常助手',
  description: '处理日常沟通',
  avatarUri: '',
  triggerCondition: '用户提到 Alice',
  manualTriggerOnly: false,
  systemPrompt: '你是 Alice。',
  skillIds: ['skill-a'],
  allowedToolIds: ['read_file'],
  defaultProviderId: 'openai',
  defaultModel: 'gpt-4o-mini',
  executionMode: 'single',
  botConfigs: {},
  heartbeatConfig: {
    timezone: 'Asia/Shanghai',
    tasks: [],
    schedules: [],
  },
}

const installedSkills: InstalledSkillItem[] = [
  {
    id: 'skill-a',
    name: 'Skill A',
    description: '帮助处理文件',
    path: '/tmp/skill-a',
    manifestPath: '/tmp/skill-a/SKILL.md',
    scope: 'workspace',
    installType: 'directory',
    updatedAt: 1,
  },
]

function renderEditor(overrides: Partial<AgentInput> = {}) {
  const props = {
    agentDraft: { ...baseAgentDraft, ...overrides },
    allSkills: installedSkills,
    mode: 'create' as const,
    modelOptions: [{ value: 'openai::gpt-4o-mini', label: 'OpenAI / gpt-4o-mini' }],
    onDraftChange: vi.fn(),
    onOpenPromptEditor: vi.fn(),
    onOpenSkillPicker: vi.fn(),
    onOpenWorkspace: vi.fn(),
    onToggleSkill: vi.fn(),
    selectedAgent: null,
  }
  render(<AgentEditorConfiguration {...props} />)
  return props
}

describe('AgentEditorConfiguration', () => {
  it('splits editor sections into menu items and shows only the active panel', () => {
    renderEditor()

    const menu = screen.getByRole('navigation', { name: '智能体配置菜单' })
    expect(within(menu).getByRole('button', { name: '基础信息' })).toBeInTheDocument()
    expect(within(menu).getByRole('button', { name: '模型' })).toBeInTheDocument()
    expect(within(menu).getByRole('button', { name: '工具权限' })).toBeInTheDocument()
    expect(screen.getByRole('textbox', { name: '展示名称' })).toBeInTheDocument()
    expect(screen.queryByRole('combobox', { name: '模型' })).not.toBeInTheDocument()

    fireEvent.click(within(menu).getByRole('button', { name: '模型' }))

    expect(screen.getByRole('combobox', { name: '模型' })).toBeInTheDocument()
    expect(screen.queryByRole('textbox', { name: '展示名称' })).not.toBeInTheDocument()
  })

  it('updates profile fields without leaving the active section', () => {
    const props = renderEditor()

    fireEvent.change(screen.getByRole('textbox', { name: '展示名称' }), {
      target: { value: '新的 Alice' },
    })

    expect(props.onDraftChange).toHaveBeenCalledWith({ name: '新的 Alice' })
  })

  it('enables agent loop from its menu section', () => {
    const props = renderEditor({ agentLoopConfig: undefined })

    fireEvent.click(screen.getByRole('button', { name: 'Agent Loop' }))
    fireEvent.click(screen.getByRole('checkbox', { name: /启用 Agent Loop/ }))

    expect(props.onDraftChange).toHaveBeenCalledWith({
      agentLoopConfig: expect.objectContaining({
        maxConcurrent: 5,
        batchFailStrategy: 'waitAll',
      }),
    })
  })

  it('searches within already added skills', () => {
    renderEditor()

    fireEvent.click(screen.getByRole('button', { name: '技能' }))
    expect(screen.getByText('Skill A')).toBeInTheDocument()

    fireEvent.change(screen.getByRole('textbox', { name: '搜索已添加技能' }), {
      target: { value: 'nomatch' },
    })

    expect(screen.queryByText('Skill A')).not.toBeInTheDocument()
    expect(screen.getByText('没有匹配的已添加技能')).toBeInTheDocument()

    fireEvent.change(screen.getByRole('textbox', { name: '搜索已添加技能' }), {
      target: { value: 'skill-a' },
    })

    expect(screen.getByText('Skill A')).toBeInTheDocument()
  })
})
