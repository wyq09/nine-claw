export const USAGE_DAILY_BAR_TRACK_PX = 120
export const USAGE_DAILY_COLUMN_WIDTH_PX = 44

export function computeDailyBarHeight(totalTokens: number, maxValue: number, trackPx = USAGE_DAILY_BAR_TRACK_PX): number {
  if (maxValue <= 0) {
    return 6
  }
  return Math.max(6, Math.round((totalTokens / maxValue) * trackPx))
}

export function formatDailyChartDayLabel(dayLabel: string): string {
  return dayLabel.length >= 10 ? dayLabel.slice(5) : dayLabel
}

export function usageDailyChartMinWidth(dayCount: number, columnWidth = USAGE_DAILY_COLUMN_WIDTH_PX): number {
  return Math.max(dayCount, 1) * columnWidth + Math.max(dayCount - 1, 0) * 8
}
