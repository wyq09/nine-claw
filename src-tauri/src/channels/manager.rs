use std::collections::HashMap;

use tauri::AppHandle;

use super::factory::{self, ChannelConfig};
use super::types::{ChannelStatus, MediaPayload};
use super::Channel;

/// Manages all bot channels and routes messages between channels and the AI backend.
pub struct ChannelManager {
    channels: HashMap<String, Box<dyn Channel>>,
}

impl ChannelManager {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    /// Register a channel using the factory.
    ///
    /// The `ChannelConfig` variant determines which concrete channel type gets
    /// created.  Callers never need to know about concrete types.
    pub fn register_channel(&mut self, config: ChannelConfig) -> Result<(), String> {
        let id = config.channel_id().to_string();
        let channel = factory::create_channel(config)?;
        self.channels.insert(id, channel);
        Ok(())
    }

    /// Start a specific channel by ID.
    pub fn start_channel(&mut self, channel_id: &str, app: AppHandle) -> Result<(), String> {
        let channel = self
            .channels
            .get_mut(channel_id)
            .ok_or_else(|| format!("未知通道: {channel_id}"))?;

        channel.start(app)
    }

    /// Stop a specific channel.
    pub fn stop_channel(&mut self, channel_id: &str) -> Result<(), String> {
        let channel = self
            .channels
            .get_mut(channel_id)
            .ok_or_else(|| format!("未知通道: {channel_id}"))?;
        channel.stop()
    }

    /// Get status of a specific channel.
    pub fn get_status(&self, channel_id: &str) -> Result<ChannelStatus, String> {
        let channel = self
            .channels
            .get(channel_id)
            .ok_or_else(|| format!("未知通道: {channel_id}"))?;
        Ok(channel.status())
    }

    /// Send a text message through a specific channel.
    pub fn send_message(
        &self,
        channel_id: &str,
        user_id: &str,
        content: &str,
    ) -> Result<(), String> {
        let channel = self
            .channels
            .get(channel_id)
            .ok_or_else(|| format!("未知通道: {channel_id}"))?;
        channel.send_message(user_id, content)
    }

    /// Send a media message through a specific channel.
    pub fn send_media(
        &self,
        channel_id: &str,
        user_id: &str,
        media: &MediaPayload,
    ) -> Result<(), String> {
        let channel = self
            .channels
            .get(channel_id)
            .ok_or_else(|| format!("未知通道: {channel_id}"))?;
        channel.send_media(user_id, media)
    }
}
