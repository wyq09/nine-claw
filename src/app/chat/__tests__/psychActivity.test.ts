import { describe, expect, it } from 'vitest'
import { partitionPsychActivityContent } from '../psychActivity'

describe('partitionPsychActivityContent', () => {
  it('splits nc_psych and strips tags', () => {
    expect(partitionPsychActivityContent('你好<nc_psych>有点紧张。</nc_psych>出去了。')).toEqual([
      { kind: 'text', body: '你好' },
      { kind: 'psych', body: '有点紧张。' },
      { kind: 'text', body: '出去了。' },
    ])
  })

  it('allows attributes on opening nc_psych tag', () => {
    expect(partitionPsychActivityContent('A<nc_psych class="x">B</nc_psych>C')).toEqual([
      { kind: 'text', body: 'A' },
      { kind: 'psych', body: 'B' },
      { kind: 'text', body: 'C' },
    ])
  })

  it('parses 【心理活动】…【/心理活动】delimiter', () => {
    expect(partitionPsychActivityContent('对白【心理活动】犹豫一下【/心理活动】继续')).toEqual([
      { kind: 'text', body: '对白' },
      { kind: 'psych', body: '犹豫一下' },
      { kind: 'text', body: '继续' },
    ])
  })

  it('treats unclosed nc_psych as streaming psych remainder', () => {
    expect(partitionPsychActivityContent('x<nc_psych>未完')).toEqual([
      { kind: 'text', body: 'x' },
      { kind: 'psych', body: '未完' },
    ])
  })

  it('handles unclosed bracket form as trailing psych body', () => {
    expect(partitionPsychActivityContent('前言【心理活动】还在想')).toEqual([
      { kind: 'text', body: '前言' },
      { kind: 'psych', body: '还在想' },
    ])
  })
})
