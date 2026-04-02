use serde::{Deserialize, Serialize};

/// Status of a bot channel.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// Bot message forwarded to the frontend for display in Chat View.
#[derive(Clone, Debug, Serialize)]
pub struct BotMessage {
    pub channel_id: String,
    pub user_id: String,
    /// "inbound" | "outbound" | "outbound_chunk" | "outbound_done" | "error"
    pub direction: String,
    pub content: String,
    pub timestamp: i64,
}

/// Media types supported by channels.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Image,
    File,
    Video,
}

/// Payload for sending media through a channel.
#[derive(Clone, Debug)]
pub struct MediaPayload {
    pub media_type: MediaType,
    /// Original file name (e.g. "report.pdf").
    pub file_name: String,
    /// Raw bytes of the media content.
    pub data: Vec<u8>,
}
