pub mod factory;
pub mod manager;
pub mod pi_bridge;
pub mod types;
pub mod wechat;

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
