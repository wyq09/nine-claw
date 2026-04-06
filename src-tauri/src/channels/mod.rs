pub mod factory;
pub mod im_reply_format;
pub mod lark;
pub mod manager;
pub mod pi_bridge;
pub mod types;
pub mod wechat;

/// 合并连续入站消息的等待窗口（毫秒）。可用环境变量 `NINECLAW_IM_MESSAGE_MERGE_MS` 覆盖（0–10000），默认 300。
pub(crate) fn im_message_merge_window_ms() -> u64 {
    const DEFAULT_MS: u64 = 300;
    std::env::var("NINECLAW_IM_MESSAGE_MERGE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value <= 10_000)
        .unwrap_or(DEFAULT_MS)
}

use tauri::AppHandle;
use types::{ChannelStatus, MediaPayload};

/// Core trait that every channel must implement (NineClaw channel interface pattern).
pub trait Channel: Send + Sync {
    /// Start listening for incoming messages.
    /// `app` is used for emitting events to the frontend.
    fn start(&mut self, app: AppHandle) -> Result<(), String>;

    /// Stop the channel and release resources.
    fn stop(&mut self) -> Result<(), String>;

    /// Send a text message to a specific user on this channel.
    fn send_message(&self, user_id: &str, content: &str) -> Result<(), String>;

    /// Send a media message (image / file / video) to a specific user.
    fn send_media(&self, user_id: &str, media: &MediaPayload) -> Result<(), String>;

    /// Get current status.
    fn status(&self) -> ChannelStatus;
}
