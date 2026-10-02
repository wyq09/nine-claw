//! 桌面 pi 进程池策略：容量上限 + LRU 驱逐 + 空闲回收。
//!
//! 本模块只做纯策略计算（不触碰真实进程），便于单元测试；池的存取与进程
//! 的 kill 由 `lib.rs` 的 `store_pooled_desktop_pi` / `take_pooled_desktop_pi`
//! 在锁外执行。

use std::time::{Duration, Instant};

pub(crate) const DEFAULT_POOL_MAX_ENTRIES: usize = 4;
pub(crate) const DEFAULT_POOL_IDLE_SECS: u64 = 600;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PiPoolPolicy {
    /// 池内最大驻留进程数；0 = 禁用池化（回合结束即回收进程）。
    pub max_entries: usize,
    /// 池内进程空闲超过该时长，在下次访问池时被回收。
    pub idle_limit: Duration,
}

impl PiPoolPolicy {
    pub fn from_env() -> Self {
        let max_entries = std::env::var("NINECLAW_PI_POOL_MAX")
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok())
            .unwrap_or(DEFAULT_POOL_MAX_ENTRIES);
        let idle_secs = std::env::var("NINECLAW_PI_POOL_IDLE_SECS")
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .filter(|secs| *secs > 0)
            .unwrap_or(DEFAULT_POOL_IDLE_SECS);
        Self {
            max_entries,
            idle_limit: Duration::from_secs(idle_secs),
        }
    }

    pub fn pooling_enabled(&self) -> bool {
        self.max_entries > 0
    }

    /// 超出容量时需要驱逐的 key，按 last_used 从旧到新驱逐（LRU），
    /// 驱逐后池大小恰为 max_entries。
    pub fn entries_to_evict(&self, last_used: &[(String, Instant)]) -> Vec<String> {
        if last_used.len() <= self.max_entries {
            return Vec::new();
        }
        let mut ordered: Vec<(String, Instant)> = last_used.to_vec();
        ordered.sort_by_key(|(_, at)| *at);
        let excess = last_used.len() - self.max_entries;
        ordered
            .into_iter()
            .take(excess)
            .map(|(key, _)| key)
            .collect()
    }

    /// 空闲时长达到 idle_limit 的 key（惰性回收：下次访问池时执行）。
    pub fn entries_idle_since(
        &self,
        last_used: &[(String, Instant)],
        now: Instant,
    ) -> Vec<String> {
        last_used
            .iter()
            .filter(|(_, at)| now.duration_since(*at) >= self.idle_limit)
            .map(|(key, _)| key.clone())
            .collect()
    }
}

/// stderr 缓冲保留的尾部字节数：超限后只保留最近的 stderr，
/// 防止池化进程存活数小时期间错误刷屏把缓冲撑到无界。
pub(crate) const STDERR_BUFFER_CAP_BYTES: usize = 256 * 1024;

/// 截断缓冲到尾部上限：超过 2× 上限时一次性裁掉，均摊 O(n)。
pub(crate) fn truncate_stderr_tail(buffer: &mut String, cap_bytes: usize) {
    if buffer.len() > cap_bytes.saturating_mul(2) {
        let cut = buffer.len() - cap_bytes;
        // 对齐到 UTF-8 字符边界，避免把多字节字符拦腰截断
        let cut = (cut..=buffer.len())
            .find(|idx| buffer.is_char_boundary(*idx))
            .unwrap_or(buffer.len());
        buffer.drain(..cut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn policy_reads_env_overrides_with_defaults() {
        let _guard = crate::workspace_env_test_lock();
        std::env::remove_var("NINECLAW_PI_POOL_MAX");
        std::env::remove_var("NINECLAW_PI_POOL_IDLE_SECS");
        assert_eq!(
            PiPoolPolicy::from_env(),
            PiPoolPolicy {
                max_entries: DEFAULT_POOL_MAX_ENTRIES,
                idle_limit: Duration::from_secs(DEFAULT_POOL_IDLE_SECS),
            }
        );

        std::env::set_var("NINECLAW_PI_POOL_MAX", "2");
        std::env::set_var("NINECLAW_PI_POOL_IDLE_SECS", "30");
        assert_eq!(
            PiPoolPolicy::from_env(),
            PiPoolPolicy {
                max_entries: 2,
                idle_limit: Duration::from_secs(30),
            }
        );

        std::env::set_var("NINECLAW_PI_POOL_MAX", "0");
        assert!(!PiPoolPolicy::from_env().pooling_enabled());

        // 非法值回退默认
        std::env::set_var("NINECLAW_PI_POOL_MAX", "not-a-number");
        std::env::set_var("NINECLAW_PI_POOL_IDLE_SECS", "-5");
        assert_eq!(
            PiPoolPolicy::from_env(),
            PiPoolPolicy {
                max_entries: DEFAULT_POOL_MAX_ENTRIES,
                idle_limit: Duration::from_secs(DEFAULT_POOL_IDLE_SECS),
            }
        );

        std::env::remove_var("NINECLAW_PI_POOL_MAX");
        std::env::remove_var("NINECLAW_PI_POOL_IDLE_SECS");
    }

    #[test]
    fn entries_to_evict_oldest_first_and_only_excess() {
        let now = Instant::now();
        let entries: Vec<(String, Instant)> = ["a", "b", "c", "d"]
            .into_iter()
            .enumerate()
            .map(|(i, key)| {
                (key.to_string(), now - Duration::from_secs(100 - i as u64 * 10))
            })
            .collect();

        let policy = PiPoolPolicy { max_entries: 2, idle_limit: Duration::from_secs(600) };
        assert_eq!(policy.entries_to_evict(&entries), vec!["a", "b"]);

        // 未超容量不驱逐
        assert!(policy.entries_to_evict(&entries[..2]).is_empty());

        // 禁用池化（max=0）：全部驱逐
        let disabled = PiPoolPolicy { max_entries: 0, idle_limit: Duration::from_secs(600) };
        assert_eq!(disabled.entries_to_evict(&entries).len(), 4);
    }

    #[test]
    fn entries_idle_since_detects_only_expired_entries() {
        let now = Instant::now();
        let entries = vec![
            ("fresh".to_string(), now - Duration::from_secs(10)),
            ("stale".to_string(), now - Duration::from_secs(700)),
        ];
        let policy = PiPoolPolicy { max_entries: 4, idle_limit: Duration::from_secs(600) };
        assert_eq!(policy.entries_idle_since(&entries, now), vec!["stale"]);
    }

    #[test]
    fn truncate_stderr_tail_keeps_recent_bytes() {
        let mut buffer = "x".repeat(1024);
        truncate_stderr_tail(&mut buffer, 128);
        assert!(buffer.len() <= 128);
        assert!(buffer.chars().all(|c| c == 'x'));

        // 小于上限不动
        let mut small = "y".repeat(16);
        truncate_stderr_tail(&mut small, 128);
        assert_eq!(small.len(), 16);

        // 空缓冲安全
        let mut empty = String::new();
        truncate_stderr_tail(&mut empty, 128);
        assert!(empty.is_empty());
    }

    #[test]
    fn truncate_stderr_tail_survives_repeated_growth() {
        let mut buffer = String::new();
        for round in 0..50 {
            buffer.push_str(&format!("round-{round}-{}\n", "z".repeat(4096)));
            truncate_stderr_tail(&mut buffer, 8 * 1024);
            assert!(buffer.len() <= 16 * 1024);
        }
        assert!(buffer.contains("round-49"));
        sleep(Duration::from_millis(1));
    }
}
