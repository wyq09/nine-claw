pub fn im_media_reply_prompt() -> &'static str {
    "当前回复目标是 IM 用户。NineClaw 已具备把本地图片、文件、视频发送给用户的能力，微信等通道会在你输出媒体指令后自动上传并下发。用户要图片或文件时，不要回答“当前通道不支持”“不能稳定发送”“只能读取展示”之类的限制性描述；如果文件已经存在或刚生成，请直接单独输出一行 `::nc-media{type=\"image|file|video\" path=\"/absolute/path/to/file\"}`。该指令行不要附加解释文字；普通文本说明单独写在其他行。若你在正文里单独列出本地绝对路径，NineClaw 也会把它视为待发送媒体，但优先使用 `::nc-media`。"
}

fn im_channel_reply_prompt(channel_id: &str) -> Option<&'static str> {
    let normalized = channel_id.trim().to_ascii_lowercase();
    if normalized.starts_with("wechat:") || normalized == "wechat" {
        Some(
            "当前通道是微信。默认只用纯文本自然段回复，不要使用 Markdown 标题、列表、表格、加粗、代码块或项目符号；除非用户明确要求，否则不要输出任何 Markdown 结构。",
        )
    } else {
        None
    }
}

pub fn im_turn_context_note(channel_id: &str) -> String {
    let mut sections = vec![im_media_reply_prompt().to_string()];
    if let Some(channel_prompt) = im_channel_reply_prompt(channel_id) {
        sections.push(channel_prompt.to_string());
    }

    format!(
        "【NineClaw 系统注入｜当前消息通道上下文】\n{}\n【/NineClaw 系统注入】",
        sections.join("\n")
    )
}
