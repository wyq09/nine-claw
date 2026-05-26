use std::path::Path;

const DESKTOP_MEDIA_REPLY_BASE: &str = "当前回复目标是 NineClaw 桌面用户。如果你需要把本地生成的图片、文件或视频真正回复给用户，请单独输出一行 `::nc-media{type=\"image|file|video\" path=\"/absolute/path/to/file\"}`。该指令行不要附加解释文字；普通文本说明单独写在其他行。";

/// `workspace_output_root`：当前会话的产物目录；与 `agent_home/outbox` 二选一优先会话目录。
pub fn desktop_media_reply_prompt(
    agent_home: Option<&Path>,
    workspace_output_root: Option<&Path>,
) -> String {
    let mut prompt = String::from(DESKTOP_MEDIA_REPLY_BASE);
    if let Some(root) = workspace_output_root {
        prompt.push_str(" 生成给用户的正式产物时，不要只放在临时目录；**请优先写入当前会话工作区** `");
        prompt.push_str(&root.display().to_string());
        prompt.push_str("` 或其子目录（与右侧「会话工作区」一致），再在 `::nc-media` 里引用那个绝对路径。");
    } else if let Some(agent_home) = agent_home {
        let preferred_dir = agent_home.join("outbox");
        prompt.push_str(" 生成给用户的正式产物时，不要只放在临时目录；优先写到 `");
        prompt.push_str(&preferred_dir.display().to_string());
        prompt.push_str("` 或其子目录，再在 `::nc-media` 里引用那个绝对路径。");
    }
    prompt
}
