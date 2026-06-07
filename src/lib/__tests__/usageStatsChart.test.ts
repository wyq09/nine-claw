import { describe, expect, it } from 'vitest'
import {
  computeDailyBarHeight,
  formatDailyChartDayLabel,
  USAGE_DAILY_BAR_TRACK_PX,
  usageDailyChartMinWidth,
} from '../usageStatsChart'

describe('usageStatsChart', () => {
  it('scales bar height against the daily maximum', () => {
    expect(computeDailyBarHeight(500, 1000, USAGE_DAILY_BAR_TRACK_PX)).toBe(60)
    expect(computeDailyBarHeight(1000, 1000, USAGE_DAILY_BAR_TRACK_PX)).toBe(120)
    expect(computeDailyBarHeight(0, 0, USAGE_DAILY_BAR_TRACK_PX)).toBe(6)
  })

  it('formats day labels for chart axis', () => {
    expect(formatDailyChartDayLabel('2026-06-07')).toBe('06-07')
  })

  it('computes scrollable chart width from day count', () => {
    expect(usageDailyChartMinWidth(3)).toBe(148)
  })
})
