import { describe, expect, it } from 'vitest'
import type { ProviderConfig, ProviderDefinition } from '../../types'
import { filterLlmProviderDefinitions, matchesLlmProviderSearch } from '../providerListSearch'

const openAi: ProviderDefinition = {
  id: 'openai',
  name: 'OpenAI',
  defaultBaseUrl: 'https://api.openai.com/v1',
  suggestedModel: 'gpt-4o-mini',
  description: 'OpenAI 官方接口',
  apiFormat: 'openai',
}

const deepSeek: ProviderDefinition = {
  id: 'deepseek',
  name: 'DeepSeek',
  defaultBaseUrl: 'https://api.deepseek.com',
  suggestedModel: 'deepseek-chat',
  description: '适合中文任务',
  apiFormat: 'openai',
}

const configs: Record<string, ProviderConfig> = {
  openai: {
    added: true,
    enabled: true,
    apiFormat: 'openai',
    displayName: '公司 OpenAI',
    baseUrl: 'https://api.openai.com/v1',
    apiKey: 'sk-test',
    model: 'gpt-4.1',
    note: '',
    status: '已配置',
  },
}

describe('providerListSearch', () => {
  it('matches provider display names and configured model names', () => {
    expect(matchesLlmProviderSearch(openAi, configs.openai, '公司')).toBe(true)
    expect(matchesLlmProviderSearch(openAi, configs.openai, 'gpt-4.1')).toBe(true)
    expect(matchesLlmProviderSearch(openAi, configs.openai, 'anthropic')).toBe(false)
  })

  it('matches preset provider names and suggested models when config is missing', () => {
    expect(matchesLlmProviderSearch(deepSeek, undefined, 'deepseek-chat')).toBe(true)
    expect(matchesLlmProviderSearch(deepSeek, undefined, 'DeepSeek')).toBe(true)
  })

  it('filters provider definitions by query', () => {
    expect(
      filterLlmProviderDefinitions([openAi, deepSeek], configs, 'gpt-4.1').map((item) => item.id),
    ).toEqual(['openai'])
    expect(filterLlmProviderDefinitions([openAi, deepSeek], configs, '').map((item) => item.id)).toEqual([
      'openai',
      'deepseek',
    ])
  })
})
