use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bot {
    // Bot ID
    pub id: String,
    // Bot Secret
    pub secret: String,
    // Creation timestamp (unix epoch seconds)
    pub create_time: u64,
}

impl Bot {
    /// Create a new Bot with `create_time` set to the current timestamp.
    pub fn new(id: String, secret: String) -> Self {
        let create_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            id,
            secret,
            create_time,
        }
    }
}
