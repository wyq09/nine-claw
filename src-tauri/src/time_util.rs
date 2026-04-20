use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn chrono_like_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}
