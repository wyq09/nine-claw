use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, FixedOffset, Local, LocalResult, NaiveDate,
    NaiveDateTime, NaiveTime, TimeZone, Utc,
};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCHEDULE_TYPE_INTERVAL: &str = "interval";
pub const SCHEDULE_TYPE_DAILY_TIME: &str = "daily_time";
pub const SCHEDULE_TYPE_WEEKLY_TIME: &str = "weekly_time";
pub const SCHEDULE_TYPE_MONTHLY_TIME: &str = "monthly_time";
pub const SCHEDULE_TYPE_ONCE_AT: &str = "once_at";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntervalSchedule {
    pub every_minutes: i64,
    pub start_at: Option<i64>,
    pub end_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyTimeSchedule {
    pub times: Vec<String>,
    pub days_of_week: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyTimeSchedule {
    pub times: Vec<String>,
    pub days_of_week: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyTimeSchedule {
    pub times: Vec<String>,
    pub days_of_month: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnceAtSchedule {
    pub run_at_ms: i64,
}

enum ResolvedTimezone {
    Named(Tz),
    Fixed(FixedOffset),
}

pub fn normalize_time_values(values: &[String]) -> Vec<String> {
    let mut items = values
        .iter()
        .filter_map(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return None;
            }
            NaiveTime::parse_from_str(trimmed, "%H:%M")
                .ok()
                .map(|time| time.format("%H:%M").to_string())
        })
        .collect::<Vec<_>>();
    items.sort();
    items.dedup();
    items
}

pub fn normalize_weekly_days(values: &[u32]) -> Vec<u32> {
    let mut items = values
        .iter()
        .copied()
        .filter(|value| (1..=7).contains(value))
        .collect::<Vec<_>>();
    items.sort();
    items.dedup();
    items
}

pub fn normalize_monthly_days(values: &[u32]) -> Vec<u32> {
    let mut items = values
        .iter()
        .copied()
        .filter(|value| (1..=31).contains(value))
        .collect::<Vec<_>>();
    items.sort();
    items.dedup();
    items
}

pub fn format_once_at_label(run_at_ms: i64) -> String {
    match DateTime::<Utc>::from_timestamp_millis(run_at_ms) {
        Some(dt) => dt
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        None => format!("{run_at_ms}"),
    }
}

pub fn format_weekly_days(days: &[u32]) -> String {
    let labels = normalize_weekly_days(days)
        .into_iter()
        .map(weekday_label)
        .collect::<Vec<_>>();
    if labels.is_empty() {
        "未设置".to_string()
    } else {
        labels.join("、")
    }
}

pub fn format_monthly_days(days: &[u32]) -> String {
    let labels = normalize_monthly_days(days)
        .into_iter()
        .map(|value| format!("{value} 号"))
        .collect::<Vec<_>>();
    if labels.is_empty() {
        "未设置".to_string()
    } else {
        labels.join("、")
    }
}

pub fn build_schedule_hint(
    schedule_type: &str,
    interval_minutes: Option<i64>,
    daily_times: &[String],
    weekly_days: &[u32],
    monthly_days: &[u32],
    run_at_ms: Option<i64>,
) -> String {
    match schedule_type {
        SCHEDULE_TYPE_INTERVAL => format!("每 {} 分钟重复", interval_minutes.unwrap_or(10)),
        SCHEDULE_TYPE_ONCE_AT => run_at_ms
            .map(|ms| format!("一次性，计划时间 {}", format_once_at_label(ms)))
            .unwrap_or_else(|| "一次性定时".to_string()),
        SCHEDULE_TYPE_WEEKLY_TIME => {
            let day_part = format_weekly_days(weekly_days);
            let time_part = normalize_time_values(daily_times);
            if time_part.is_empty() {
                format!("每周 {day_part}")
            } else {
                format!("每周 {day_part} {}", time_part.join("、"))
            }
        }
        SCHEDULE_TYPE_MONTHLY_TIME => {
            let day_part = format_monthly_days(monthly_days);
            let time_part = normalize_time_values(daily_times);
            if time_part.is_empty() {
                format!("每月 {day_part}")
            } else {
                format!("每月 {day_part} {}", time_part.join("、"))
            }
        }
        _ => {
            let time_part = normalize_time_values(daily_times);
            if time_part.is_empty() {
                "每日定时".to_string()
            } else {
                format!("每日 {}", time_part.join("、"))
            }
        }
    }
}

pub fn detect_schedule_type(prompt: &str, timezone: &str) -> String {
    let text = prompt.trim();
    let lowered = text.to_lowercase();

    if text.contains("每周")
        || text.contains("每星期")
        || text.contains("工作日")
        || text.contains("周末")
        || lowered.contains("weekly")
        || lowered.contains("every week")
    {
        if !extract_weekly_days(text).is_empty() {
            return SCHEDULE_TYPE_WEEKLY_TIME.to_string();
        }
    }

    if text.contains("每月")
        || text.contains("每个月")
        || lowered.contains("monthly")
        || lowered.contains("every month")
    {
        if !extract_monthly_days(text).is_empty() {
            return SCHEDULE_TYPE_MONTHLY_TIME.to_string();
        }
    }

    if text.contains("每隔")
        || lowered.contains("every ")
        || lowered.contains("hourly")
        || lowered.contains("minute")
    {
        if extract_interval_minutes(text).is_some() {
            return SCHEDULE_TYPE_INTERVAL.to_string();
        }
    }

    if text.contains("每天")
        || text.contains("每日")
        || lowered.contains("every day")
        || lowered.contains("daily")
    {
        if !extract_daily_times(text).is_empty() {
            return SCHEDULE_TYPE_DAILY_TIME.to_string();
        }
    }

    if text.contains("一次性")
        || text.contains("只执行一次")
        || text.contains("单次")
        || extract_once_at_ms(text, timezone).is_some()
    {
        return SCHEDULE_TYPE_ONCE_AT.to_string();
    }

    String::new()
}

pub fn extract_interval_minutes(prompt: &str) -> Option<i64> {
    let normalized = prompt.replace('个', "");
    if let Some(index) = normalized.find("分钟") {
        let number = extract_number_near(&normalized[..index])?;
        return Some(number);
    }
    if let Some(index) = normalized.find("小时") {
        let number = extract_number_near(&normalized[..index])?;
        return Some(number * 60);
    }
    let lowered = normalized.to_lowercase();
    if let Some(index) = lowered.find("hours") {
        let number = extract_number_near(&lowered[..index])?;
        return Some(number * 60);
    }
    if let Some(index) = lowered.find("hour") {
        let number = extract_number_near(&lowered[..index])?;
        return Some(number * 60);
    }
    if let Some(index) = lowered.find("minutes") {
        let number = extract_number_near(&lowered[..index])?;
        return Some(number);
    }
    if let Some(index) = lowered.find("minute") {
        let number = extract_number_near(&lowered[..index])?;
        return Some(number);
    }
    None
}

pub fn extract_daily_times(prompt: &str) -> Vec<String> {
    let normalized = prompt.replace('：', ":").replace('点', ":");
    let chars = normalized.chars().collect::<Vec<_>>();
    let mut times = Vec::new();
    let mut index = 0usize;
    while index < chars.len() {
        if !chars[index].is_ascii_digit() && !is_chinese_number_char(chars[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len()
            && (chars[index].is_ascii_digit() || is_chinese_number_char(chars[index]))
        {
            index += 1;
        }
        let hour_raw = chars[start..index].iter().collect::<String>();
        let Some(hour) = parse_digit_or_chinese(&hour_raw) else {
            continue;
        };
        if index >= chars.len() || chars[index] != ':' {
            continue;
        }
        index += 1;
        let minute = if index < chars.len() && chars[index] == '半' {
            index += 1;
            30
        } else {
            let minute_start = index;
            while index < chars.len()
                && (chars[index].is_ascii_digit() || is_chinese_number_char(chars[index]))
            {
                index += 1;
            }
            if minute_start == index {
                0
            } else {
                let minute_raw = chars[minute_start..index].iter().collect::<String>();
                let Some(parsed_minute) = parse_digit_or_chinese(&minute_raw) else {
                    continue;
                };
                parsed_minute
            }
        };
        if hour > 23 || minute > 59 {
            continue;
        }
        let mut adjusted_hour = hour;
        let prefix = prompt[..prompt.find(&hour_raw).unwrap_or(0)].to_string();
        if prefix.contains("下午") || prefix.contains("晚上") {
            if adjusted_hour < 12 {
                adjusted_hour += 12;
            }
        } else if prefix.contains("凌晨") && adjusted_hour == 12 {
            adjusted_hour = 0;
        }
        times.push(format!("{adjusted_hour:02}:{minute:02}"));
    }
    let mut seen = HashSet::new();
    times.retain(|item| seen.insert(item.clone()));
    times.sort();
    times
}

pub fn extract_weekly_days(prompt: &str) -> Vec<u32> {
    let mut days = Vec::new();
    if prompt.contains("工作日") {
        days.extend([1, 2, 3, 4, 5]);
    }
    if prompt.contains("周末") {
        days.extend([6, 7]);
    }
    for (needle, value) in [
        ("周一", 1_u32),
        ("星期一", 1_u32),
        ("周二", 2_u32),
        ("星期二", 2_u32),
        ("周三", 3_u32),
        ("星期三", 3_u32),
        ("周四", 4_u32),
        ("星期四", 4_u32),
        ("周五", 5_u32),
        ("星期五", 5_u32),
        ("周六", 6_u32),
        ("星期六", 6_u32),
        ("周日", 7_u32),
        ("周天", 7_u32),
        ("星期日", 7_u32),
        ("星期天", 7_u32),
    ] {
        if prompt.contains(needle) {
            days.push(value);
        }
    }
    normalize_weekly_days(&days)
}

pub fn extract_monthly_days(prompt: &str) -> Vec<u32> {
    if !prompt.contains("每月") && !prompt.contains("每个月") {
        return Vec::new();
    }
    let chars = prompt.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    let mut days = Vec::new();
    while index < chars.len() {
        if !chars[index].is_ascii_digit() && !is_chinese_number_char(chars[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len()
            && (chars[index].is_ascii_digit() || is_chinese_number_char(chars[index]))
        {
            index += 1;
        }
        let value_end = index;
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        if index >= chars.len() || (chars[index] != '号' && chars[index] != '日') {
            continue;
        }
        let raw = chars[start..value_end].iter().collect::<String>();
        let Some(value) = parse_digit_or_chinese(&raw) else {
            continue;
        };
        if (1..=31).contains(&value) {
            days.push(value);
        }
        index += 1;
    }
    normalize_monthly_days(&days)
}

pub fn extract_once_at_ms(prompt: &str, timezone: &str) -> Option<i64> {
    let time = extract_daily_times(prompt)
        .into_iter()
        .next()
        .and_then(|value| NaiveTime::parse_from_str(&value, "%H:%M").ok())?;
    let date = extract_relative_date(prompt, timezone)
        .or_else(|| extract_full_date(prompt))
        .or_else(|| extract_month_day(prompt, timezone))?;
    local_naive_to_utc_ms(timezone, NaiveDateTime::new(date, time))
}

fn extract_relative_date(prompt: &str, timezone: &str) -> Option<NaiveDate> {
    let today = timezone_now(timezone).date_naive();
    if prompt.contains("今天") {
        return Some(today);
    }
    if prompt.contains("明天") || prompt.contains("明早") || prompt.contains("明晚") {
        return Some(today + ChronoDuration::days(1));
    }
    if prompt.contains("后天") {
        return Some(today + ChronoDuration::days(2));
    }
    None
}

fn extract_full_date(prompt: &str) -> Option<NaiveDate> {
    let normalized = prompt
        .replace('/', "-")
        .replace('.', "-")
        .replace('年', "-")
        .replace('月', "-")
        .replace("日", "")
        .replace("号", "");
    let chars = normalized.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < chars.len() {
        if !chars[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len() && chars[index].is_ascii_digit() {
            index += 1;
        }
        if index >= chars.len() || chars[index] != '-' {
            continue;
        }
        let year_raw = chars[start..index].iter().collect::<String>();
        index += 1;
        let month_start = index;
        while index < chars.len() && chars[index].is_ascii_digit() {
            index += 1;
        }
        if month_start == index || index >= chars.len() || chars[index] != '-' {
            continue;
        }
        let month_raw = chars[month_start..index].iter().collect::<String>();
        index += 1;
        let day_start = index;
        while index < chars.len() && chars[index].is_ascii_digit() {
            index += 1;
        }
        if day_start == index {
            continue;
        }
        let day_raw = chars[day_start..index].iter().collect::<String>();
        let year: i32 = year_raw.parse().ok()?;
        let month: u32 = month_raw.parse().ok()?;
        let day: u32 = day_raw.parse().ok()?;
        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
            return Some(date);
        }
    }
    None
}

fn extract_month_day(prompt: &str, timezone: &str) -> Option<NaiveDate> {
    let chars = prompt.chars().collect::<Vec<_>>();
    let mut index = 0usize;
    while index < chars.len() {
        if !chars[index].is_ascii_digit() && !is_chinese_number_char(chars[index]) {
            index += 1;
            continue;
        }
        let month_start = index;
        while index < chars.len()
            && (chars[index].is_ascii_digit() || is_chinese_number_char(chars[index]))
        {
            index += 1;
        }
        if index >= chars.len() || chars[index] != '月' {
            continue;
        }
        let month_raw = chars[month_start..index].iter().collect::<String>();
        index += 1;
        let day_start = index;
        while index < chars.len()
            && (chars[index].is_ascii_digit() || is_chinese_number_char(chars[index]))
        {
            index += 1;
        }
        if day_start == index
            || index >= chars.len()
            || (chars[index] != '号' && chars[index] != '日')
        {
            continue;
        }
        let day_raw = chars[day_start..index].iter().collect::<String>();
        let month = parse_digit_or_chinese(&month_raw)?;
        let day = parse_digit_or_chinese(&day_raw)?;
        let current = timezone_now(timezone).date_naive();
        let mut year = current.year();
        let candidate = NaiveDate::from_ymd_opt(year, month, day)?;
        if candidate < current {
            year += 1;
        }
        return NaiveDate::from_ymd_opt(year, month, day);
    }
    None
}

fn local_naive_to_utc_ms(timezone: &str, naive: NaiveDateTime) -> Option<i64> {
    match resolve_timezone(timezone) {
        ResolvedTimezone::Named(tz) => match tz.from_local_datetime(&naive) {
            LocalResult::Single(value) => Some(value.with_timezone(&Utc).timestamp_millis()),
            LocalResult::Ambiguous(first, second) => {
                Some(first.min(second).with_timezone(&Utc).timestamp_millis())
            }
            LocalResult::None => None,
        },
        ResolvedTimezone::Fixed(offset) => offset
            .from_local_datetime(&naive)
            .single()
            .map(|value| value.with_timezone(&Utc).timestamp_millis()),
    }
}

fn timezone_now(timezone: &str) -> DateTime<Utc> {
    let now = Utc::now();
    match resolve_timezone(timezone) {
        ResolvedTimezone::Named(tz) => now.with_timezone(&tz).with_timezone(&Utc),
        ResolvedTimezone::Fixed(offset) => now.with_timezone(&offset).with_timezone(&Utc),
    }
}

fn resolve_timezone(raw: &str) -> ResolvedTimezone {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ResolvedTimezone::Named(chrono_tz::Asia::Shanghai);
    }
    if let Ok(tz) = trimmed.parse::<Tz>() {
        return ResolvedTimezone::Named(tz);
    }
    match trimmed {
        "PRC" => ResolvedTimezone::Named(chrono_tz::Asia::Shanghai),
        "UTC" | "Etc/UTC" | "GMT" => ResolvedTimezone::Named(chrono_tz::UTC),
        value => {
            let seconds = parse_explicit_offset(value).unwrap_or(8 * 3600);
            let offset = FixedOffset::east_opt(seconds)
                .unwrap_or_else(|| FixedOffset::east_opt(8 * 3600).expect("valid default offset"));
            ResolvedTimezone::Fixed(offset)
        }
    }
}

fn parse_explicit_offset(value: &str) -> Option<i32> {
    let normalized = value.strip_prefix("UTC").unwrap_or(value).trim();
    let sign = if normalized.starts_with('-') { -1 } else { 1 };
    let value = normalized.trim_start_matches(['+', '-']);
    let (hour_raw, minute_raw) = value.split_once(':')?;
    let hour: i32 = hour_raw.parse().ok()?;
    let minute: i32 = minute_raw.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(sign * (hour * 3600 + minute * 60))
}

fn weekday_label(day: u32) -> String {
    match day {
        1 => "周一".to_string(),
        2 => "周二".to_string(),
        3 => "周三".to_string(),
        4 => "周四".to_string(),
        5 => "周五".to_string(),
        6 => "周六".to_string(),
        7 => "周日".to_string(),
        value => format!("周{value}"),
    }
}

fn extract_number_near(input: &str) -> Option<i64> {
    let trimmed = input.trim();
    let digits = trimmed
        .chars()
        .rev()
        .take_while(|char| char.is_ascii_digit() || is_chinese_number_char(*char))
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    if let Ok(value) = digits.parse::<i64>() {
        return Some(value);
    }
    parse_chinese_number(&digits)
}

fn parse_digit_or_chinese(input: &str) -> Option<u32> {
    if let Ok(value) = input.parse::<u32>() {
        return Some(value);
    }
    parse_chinese_number(input).and_then(|value| u32::try_from(value).ok())
}

fn is_chinese_number_char(char: char) -> bool {
    matches!(
        char,
        '零' | '一' | '二' | '两' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
    )
}

fn parse_chinese_number(input: &str) -> Option<i64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "十" {
        return Some(10);
    }
    if let Some(rest) = trimmed.strip_prefix('十') {
        let tail = chinese_digit(rest.chars().next()?)?;
        return Some(10 + tail);
    }
    if let Some((head, tail)) = trimmed.split_once('十') {
        let head_value = if head.is_empty() {
            1
        } else {
            chinese_digit(head.chars().next()?)?
        };
        let tail_value = if tail.is_empty() {
            0
        } else {
            chinese_digit(tail.chars().next()?)?
        };
        return Some(head_value * 10 + tail_value);
    }
    chinese_digit(trimmed.chars().next()?)
}

fn chinese_digit(char: char) -> Option<i64> {
    match char {
        '零' => Some(0),
        '一' => Some(1),
        '二' | '两' => Some(2),
        '三' => Some(3),
        '四' => Some(4),
        '五' => Some(5),
        '六' => Some(6),
        '七' => Some(7),
        '八' => Some(8),
        '九' => Some(9),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_weekly_and_monthly_schedule_types() {
        assert_eq!(
            detect_schedule_type("每周一早上九点整理销售线索", "Asia/Shanghai"),
            SCHEDULE_TYPE_WEEKLY_TIME
        );
        assert_eq!(
            detect_schedule_type("每月5号 09:30 提醒我交房租", "Asia/Shanghai"),
            SCHEDULE_TYPE_MONTHLY_TIME
        );
    }

    #[test]
    fn parses_relative_once_at_datetime() {
        let result = extract_once_at_ms("明天下午3点提醒我给客户回电话", "Asia/Shanghai");
        assert!(result.is_some());
    }

    #[test]
    fn parses_weekly_days_and_monthly_days() {
        assert_eq!(extract_weekly_days("每周一和周三下午 6 点同步"), vec![1, 3]);
        assert_eq!(
            extract_monthly_days("每个月 5 号和 20 号提醒我"),
            vec![5, 20]
        );
    }
}
