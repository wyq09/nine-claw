use super::Channel;

/// Configuration variants for creating channels via the factory.
///
/// Each variant carries all the parameters needed to construct that channel type.
/// To add a new channel:
///   1. Add a variant here (e.g. `DingTalk { ... }`).
///   2. Implement `Channel` for the new struct.
///   3. Add a match arm in `create_channel`.
pub enum ChannelConfig {
    WeChat {
        token: String,
        base_url: String,
        route_tag: Option<String>,
        ai_provider_id: String,
        ai_api_format: String,
        ai_base_url: String,
        ai_api_key: String,
        ai_model: String,
    },
    // ── Future channels ──
    // DingTalk { client_id: String, client_secret: String, ... },
    // Lark { app_id: String, app_secret: String, ... },
}

impl ChannelConfig {
    /// Return the canonical channel id for this config variant.
    pub fn channel_id(&self) -> &'static str {
        match self {
            ChannelConfig::WeChat { .. } => "wechat",
        }
    }
}

/// Create a boxed `Channel` from a `ChannelConfig`.
///
/// This is the single entry point for channel construction — all channel
/// creation goes through here so that callers never need to know about
/// concrete channel types.
pub fn create_channel(config: ChannelConfig) -> Result<Box<dyn Channel>, String> {
    match config {
        ChannelConfig::WeChat {
            token,
            base_url,
            route_tag,
            ai_provider_id,
            ai_api_format,
            ai_base_url,
            ai_api_key,
            ai_model,
        } => {
            use super::wechat::WeChatChannel;
            let mut ch = WeChatChannel::new(&token, &base_url, route_tag.as_deref());
            ch.set_ai_config(
                &ai_provider_id,
                &ai_api_format,
                &ai_base_url,
                &ai_api_key,
                &ai_model,
            );
            Ok(Box::new(ch))
        }
    }
}
