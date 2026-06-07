import { render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { UserMemorySettingsPanel } from '../UserMemorySettingsPanel'
import { workspaceKvMemoryUiList, workspaceKvMemoryUiReorganize } from '../../../lib/workspaceKvMemoryClient'

vi.mock('../../../lib/workspaceKvMemoryClient', () => ({
  workspaceKvMemoryUiForget: vi.fn(),
  workspaceKvMemoryUiList: vi.fn(),
  workspaceKvMemoryUiReorganize: vi.fn(),
  workspaceKvMemoryUiStore: vi.fn(),
}))

describe('UserMemorySettingsPanel', () => {
  beforeEach(() => {
    vi.mocked(workspaceKvMemoryUiReorganize).mockResolvedValue(undefined)
    vi.mocked(workspaceKvMemoryUiList).mockResolvedValue([])
  })

  it('uses the shared settings select field styling for the memory scope dropdown', async () => {
    const { container } = render(
      <UserMemorySettingsPanel
        agents={[{ id: 'agent-a', name: '助手 A' } as never]}
        defaultAgentId="agent-a"
      />,
    )

    await waitFor(() => {
      expect(screen.getByRole('combobox', { name: '选择全局或某一智能体的记忆命名空间' })).toBeInTheDocument()
    })

    expect(container.querySelector('.user-memory-agent-select.select-field')).toBeTruthy()
  })
})
