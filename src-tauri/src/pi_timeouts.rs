//! Pi 子进程流式读取超时，可通过环境变量覆盖（每次启动 pi 会话时读取一次）。
//!
//! - `NINECLAW_PI_TOTAL_RUNTIME_SECS`：单次 pi 运行总时长上限，默认 180，范围 30–2592000（30 天）。
//! - `NINECLAW_PI_FIRST_OUTPUT_SECS`：首包输出等待，默认 60，范围 10–86400。
//! - `NINECLAW_PI_IDLE_OUTPUT_SECS`：有输出后静默等待，默认 60，范围 10–86400。

use std::time::Duration;

const DEFAULT_TOTAL_SECS: u64 = 1800;
const DEFAULT_IO_SECS: u64 = 60;
const MIN_TOTAL_SECS: u64 = 30;
const MAX_TOTAL_SECS: u64 = 30 * 24 * 3600;
const MIN_IO_SECS: u64 = 10;
const MAX_IO_SECS: u64 = 24 * 3600;

fn parse_env_secs(name: &'static str, default: u64, min: u64, max: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|&n| (min..=max).contains(&n))
        .unwrap_or(default)
}

pub fn pi_total_runtime_timeout() -> Duration {
    Duration::from_secs(parse_env_secs(
        "NINECLAW_PI_TOTAL_RUNTIME_SECS",
        DEFAULT_TOTAL_SECS,
        MIN_TOTAL_SECS,
        MAX_TOTAL_SECS,
    ))
}

pub fn pi_first_output_timeout() -> Duration {
    Duration::from_secs(parse_env_secs(
        "NINECLAW_PI_FIRST_OUTPUT_SECS",
        DEFAULT_IO_SECS,
        MIN_IO_SECS,
        MAX_IO_SECS,
    ))
}

pub fn pi_idle_output_timeout() -> Duration {
    Duration::from_secs(parse_env_secs(
        "NINECLAW_PI_IDLE_OUTPUT_SECS",
        DEFAULT_IO_SECS,
        MIN_IO_SECS,
        MAX_IO_SECS,
    ))
}
