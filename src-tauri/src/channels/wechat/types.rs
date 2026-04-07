use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ── iLink Bot API Constants ──
pub const MSG_TYPE_USER: i32 = 1;
pub const MSG_TYPE_BOT: i32 = 2;
pub const MSG_ITEM_TYPE_TEXT: i32 = 1;
pub const MSG_ITEM_TYPE_IMAGE: i32 = 2;
pub const MSG_ITEM_TYPE_VOICE: i32 = 3;
pub const MSG_ITEM_TYPE_FILE: i32 = 4;
pub const MSG_ITEM_TYPE_VIDEO: i32 = 5;
pub const MSG_STATE_FINISH: i32 = 2;

/// iLink protocol version — must match a known version.
pub const CHANNEL_VERSION: &str = "2.0.0";

// ── API Response Types (mirrors the NineClaw WeChat plugin contract) ──

/// Response from getUpdates long-poll.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct GetUpdatesResp {
    pub ret: Option<i32>,
    pub errcode: Option<i32>,
    pub errmsg: Option<String>,
    pub msgs: Option<Vec<WeixinMessage>>,
    pub get_updates_buf: Option<String>,
    pub longpolling_timeout_ms: Option<i64>,
}

/// A single WeChat message (proto: WeixinMessage).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct WeixinMessage {
    pub seq: Option<i64>,
    pub message_id: Option<i64>,
    pub from_user_id: Option<String>,
    pub to_user_id: Option<String>,
    pub client_id: Option<String>,
    pub create_time_ms: Option<i64>,
    pub update_time_ms: Option<i64>,
    pub message_type: Option<i32>,
    pub message_state: Option<i32>,
    pub item_list: Option<Vec<MessageItem>>,
    pub context_token: Option<String>,
    pub session_id: Option<String>,
}

/// An item within a message.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct MessageItem {
    #[serde(rename = "type")]
    pub item_type: Option<i32>,
    pub text_item: Option<TextItem>,
    pub image_item: Option<ImageItem>,
    pub file_item: Option<FileItem>,
    pub video_item: Option<VideoItem>,
    pub voice_item: Option<VoiceItem>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct TextItem {
    pub text: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CdnMedia {
    pub encrypt_query_param: Option<String>,
    pub aes_key: Option<String>,
    pub encrypt_type: Option<i32>,
    pub full_url: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ImageItem {
    pub media: Option<CdnMedia>,
    pub thumb_media: Option<CdnMedia>,
    /// Raw AES-128 key as a 32-char hex string; preferred over media.aes_key for inbound images.
    pub aeskey: Option<String>,
    pub url: Option<String>,
    /// Base64-encoded image data.
    pub image_base64: Option<String>,
    /// Image URL (if provided by the platform).
    pub image_url: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct FileItem {
    pub media: Option<CdnMedia>,
    /// Base64-encoded file data.
    pub file_base64: Option<String>,
    /// Original file name.
    pub file_name: Option<String>,
    /// File URL (if provided by the platform).
    pub file_url: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct VideoItem {
    pub media: Option<CdnMedia>,
    pub thumb_media: Option<CdnMedia>,
    /// Base64-encoded video data.
    pub video_base64: Option<String>,
    /// Video URL (if provided by the platform).
    pub video_url: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct VoiceItem {
    pub media: Option<CdnMedia>,
    /// Base64-encoded voice/audio data.
    pub voice_base64: Option<String>,
    /// Voice URL (if provided by the platform).
    pub voice_url: Option<String>,
    /// Optional transcript text returned by the platform.
    pub text: Option<String>,
    /// Optional duration in milliseconds.
    pub duration_ms: Option<i64>,
    /// Optional original file name.
    pub file_name: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

/// Response from get_bot_qrcode.
/// Note: iLink API returns snake_case JSON — do NOT use camelCase rename.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct QRCodeResponse {
    pub qrcode: Option<String>,
    pub qrcode_img_content: Option<String>,
}

/// Response from get_qrcode_status.
/// Note: iLink API returns snake_case JSON — do NOT use camelCase rename.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct QRCodeStatusResponse {
    pub status: Option<String>,
    pub bot_token: Option<String>,
    pub ilink_bot_id: Option<String>,
    pub baseurl: Option<String>,
    pub ilink_user_id: Option<String>,
}

/// Login result returned to the Tauri command.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct WechatLoginResult {
    pub connected: bool,
    pub bot_token: Option<String>,
    pub account_id: Option<String>,
    pub base_url: Option<String>,
    pub user_id: Option<String>,
    pub message: String,
}
