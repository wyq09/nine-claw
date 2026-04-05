use super::Channel;
use crate::agents::ConversationAgentConfig;

/// Configuration variants for creating channels via the factory.
///
/// Each variant carries all the parameters needed to construct that channel type.
/// To add a new channel:
///   1. Add a variant here (e.g. `DingTalk { ... }`).
///   2. Implement `Channel` for the new struct.
///   3. Add a match arm in `create_channel`.
pub enum ChannelConfig {
    WeChat {
        channel_id: String,
        agent_config: Option<ConversationAgentConfig>,
        token: String,
        base_url: String,
        route_tag: Option<String>,
        ai_provider_id: String,
        ai_api_format: String,
        ai_base_url: String,
        ai_api_key: String,
        ai_model: String,
    },
    Lark {
        channel_id: String,
        agent_config: Option<ConversationAgentConfig>,
        app_id: String,
        app_secret: String,
        ai_provider_id: String,
        ai_api_format: String,
        ai_base_url: String,
        ai_api_key: String,
        ai_model: String,
    },
    // ── Future channels ──
    // DingTalk { client_id: String, client_secret: String, ... },
}

impl ChannelConfig {
    /// Return the canonical channel id for this config variant.
    pub fn channel_id(&self) -> String {
        match self {
            ChannelConfig::WeChat { channel_id, .. } => channel_id.clone(),
            ChannelConfig::Lark { channel_id, .. } => channel_id.clone(),
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
            channel_id,
            agent_config,
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
            let mut ch = WeChatChannel::new(&channel_id, &token, &base_url, route_tag.as_deref());
            ch.set_ai_config(
                &ai_provider_id,
                &ai_api_format,
                &ai_base_url,
                &ai_api_key,
                &ai_model,
                agent_config,
            );
            Ok(Box::new(ch))
        }
        ChannelConfig::Lark {
            channel_id,
            agent_config,
            app_id,
            app_secret,
            ai_provider_id,
            ai_api_format,
            ai_base_url,
            ai_api_key,
            ai_model,
        } => {
            use super::lark::LarkChannel;
            let mut ch = LarkChannel::new(&channel_id, &app_id, &app_secret);
            ch.set_ai_config(
                &ai_provider_id,
                &ai_api_format,
                &ai_base_url,
                &ai_api_key,
                &ai_model,
                agent_config,
            );
            Ok(Box::new(ch))
        }
    }
}
