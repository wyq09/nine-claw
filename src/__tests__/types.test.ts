import { describe, it, expect } from 'vitest'
import type { ProviderConfig } from '../types'

describe('ProviderConfig type — maxContextTokens field', () => {
  it('accepts optional maxContextTokens field', () => {
    const config: ProviderConfig = {
      enabled: true,
      added: true,
      apiFormat: 'openai',
      baseUrl: 'https://example.com',
      apiKey: 'key',
      model: 'gpt-4',
      note: '',
      displayName: '',
      status: '已配置',
      maxContextTokens: 128000,
    }
    expect(config.maxContextTokens).toBe(128000)
  })

  it('works without maxContextTokens (backward compatible)', () => {
    const config: ProviderConfig = {
      enabled: false,
      added: false,
      apiFormat: 'openai',
      baseUrl: '',
      apiKey: '',
      model: '',
      note: '',
      displayName: '',
      status: '未配置',
    }
    expect(config.maxContextTokens).toBeUndefined()
  })
})
