import { describe, expect, it } from 'vitest'

import { describeNetworkProxyMode, resolveCustomProxyUrl } from '../networkProxy'

describe('resolveCustomProxyUrl', () => {
  it('adds an http scheme for host:port input', () => {
    expect(resolveCustomProxyUrl('127.0.0.1:7890')).toBe('http://127.0.0.1:7890')
  })

  it('keeps explicit proxy schemes', () => {
    expect(resolveCustomProxyUrl('socks5://127.0.0.1:7890/')).toBe('socks5://127.0.0.1:7890')
  })

  it('returns null for invalid input', () => {
    expect(resolveCustomProxyUrl('http://')).toBeNull()
  })
})

describe('describeNetworkProxyMode', () => {
  it('prefers custom proxy messaging when both options are present', () => {
    expect(
      describeNetworkProxyMode({
        useSystemProxy: true,
        customProxyUrl: '127.0.0.1:7890',
      }),
    ).toContain('http://127.0.0.1:7890')
  })

  it('surfaces invalid custom proxy input', () => {
    expect(
      describeNetworkProxyMode({
        useSystemProxy: false,
        customProxyUrl: 'http://',
      }),
    ).toContain('格式无效')
  })
})
