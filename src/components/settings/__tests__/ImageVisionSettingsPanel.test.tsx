import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ImageVisionSettingsPanel } from '../ImageVisionSettingsPanel'

vi.mock('../../../lib/imageVisionClient', () => ({
  loadImageVisionPreferences: vi.fn(),
  saveImageVisionPreferences: vi.fn(async () => undefined),
}))

import {
  loadImageVisionPreferences,
  saveImageVisionPreferences,
} from '../../../lib/imageVisionClient'

const storedConfig = JSON.stringify({
  apiFormat: 'openai',
  baseUrl: 'https://vision.example.com/v1',
  apiKey: 'secret-key',
  model: 'qwen-vl-max',
  maxOutputTokens: 1024,
  defaultPrompt: '描述图表数据',
})

describe('ImageVisionSettingsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(loadImageVisionPreferences).mockResolvedValue(null)
  })

  it('loads stored vision config on mount', async () => {
    vi.mocked(loadImageVisionPreferences).mockResolvedValue(storedConfig)

    render(<ImageVisionSettingsPanel />)

    expect(await screen.findByDisplayValue('qwen-vl-max')).toBeInTheDocument()
    expect(screen.getByDisplayValue('https://vision.example.com/v1')).toBeInTheDocument()
    expect(screen.getAllByText('已配置').length).toBeGreaterThan(0)
  })

  it('shows default placeholders when nothing is configured', async () => {
    render(<ImageVisionSettingsPanel />)

    await waitFor(() => {
      expect(screen.getAllByText('未配置').length).toBeGreaterThan(0)
    })
    expect(loadImageVisionPreferences).toHaveBeenCalledTimes(1)
    expect(screen.getByText('保存识图配置')).toBeEnabled()
  })

  it('saves the draft config through the backend command', async () => {
    vi.mocked(loadImageVisionPreferences).mockResolvedValue(storedConfig)

    render(<ImageVisionSettingsPanel />)
    fireEvent.change(await screen.findByDisplayValue('qwen-vl-max'), {
      target: { value: 'gpt-4o' },
    })
    fireEvent.click(screen.getByText('保存识图配置'))

    await waitFor(() => {
      expect(saveImageVisionPreferences).toHaveBeenCalledTimes(1)
    })
    const payload = vi.mocked(saveImageVisionPreferences).mock.calls[0][0]
    expect(JSON.parse(payload)).toMatchObject({
      model: 'gpt-4o',
      baseUrl: 'https://vision.example.com/v1',
      apiKey: 'secret-key',
    })
    expect(await screen.findByText('识图模型配置已保存，新会话起生效。')).toBeInTheDocument()
  })

  it('refuses to save an incomplete config', async () => {
    render(<ImageVisionSettingsPanel />)

    await waitFor(() => {
      expect(screen.getByText('未配置')).toBeInTheDocument()
    })
    fireEvent.change(screen.getByLabelText('视觉模型名称', { exact: false }), {
      target: { value: 'qwen-vl-max' },
    })
    fireEvent.click(screen.getByText('保存识图配置'))

    expect(
      await screen.findByText('请完整填写 Base URL、API Key 和模型名称后再保存。'),
    ).toBeInTheDocument()
    expect(saveImageVisionPreferences).not.toHaveBeenCalled()
  })

  it('surfaces backend save failures', async () => {
    vi.mocked(loadImageVisionPreferences).mockResolvedValue(storedConfig)
    vi.mocked(saveImageVisionPreferences).mockRejectedValue(new Error('db locked'))

    render(<ImageVisionSettingsPanel />)
    fireEvent.click(await screen.findByText('保存识图配置'))

    expect(await screen.findByText('db locked')).toBeInTheDocument()
  })
})