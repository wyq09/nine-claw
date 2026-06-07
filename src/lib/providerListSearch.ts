import type { ProviderConfig, ProviderDefinition } from '../types'

export function normalizeProviderSearchQuery(query: string): string {
  return query.trim().toLowerCase()
}

export function resolveProviderDisplayName(
  definition: ProviderDefinition,
  config: ProviderConfig | undefined,
): string {
  const label = config?.displayName?.trim()
  return label || definition.name
}

export function matchesLlmProviderSearch(
  definition: ProviderDefinition,
  config: ProviderConfig | undefined,
  query: string,
): boolean {
  const normalized = normalizeProviderSearchQuery(query)
  if (!normalized) {
    return true
  }

  const haystack = [
    definition.id,
    definition.name,
    definition.description,
    definition.suggestedModel,
    resolveProviderDisplayName(definition, config),
    config?.model ?? '',
    config?.status ?? '',
  ]

  return haystack.some((value) => value.toLowerCase().includes(normalized))
}

export function filterLlmProviderDefinitions(
  providers: ProviderDefinition[],
  providerConfigs: Record<string, ProviderConfig>,
  query: string,
): ProviderDefinition[] {
  return providers.filter((definition) =>
    matchesLlmProviderSearch(definition, providerConfigs[definition.id], query),
  )
}
