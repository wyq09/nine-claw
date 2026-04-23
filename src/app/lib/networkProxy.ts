import type { NetworkProxySettings } from '../../types'

export function resolveCustomProxyUrl(raw: string): string | null {
  const trimmed = raw.trim()
  if (!trimmed) {
    return null
  }

  const candidate = trimmed.includes('://') ? trimmed : `http://${trimmed}`
  try {
    const url = new URL(candidate)
    if (!url.hostname) {
      return null
    }
    return url.toString().replace(/\/$/, '')
  } catch {
    return null
  }
}

export function describeNetworkProxyMode(settings: NetworkProxySettings): string {
  const customProxyUrl = resolveCustomProxyUrl(settings.customProxyUrl)
  if (customProxyUrl) {
    return `当前优先使用自定义代理 ${customProxyUrl}`
  }
  if (settings.useSystemProxy) {
    return '当前跟随系统代理；若填写下方地址，会优先走自定义代理'
  }
  if (settings.customProxyUrl.trim()) {
    return '自定义代理地址格式无效，请检查后再测试'
  }
  return '当前未启用代理；填写地址后会优先走自定义代理'
}
