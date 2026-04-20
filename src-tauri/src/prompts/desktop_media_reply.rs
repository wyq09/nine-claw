use std::path::Path;

const DESKTOP_MEDIA_REPLY_BASE: &str = "当前回复目标是 NineClaw 桌面用户。如果你需要把本地生成的图片、文件或视频真正回复给用户，请单独输出一行 `::nc-media{type=\"image|file|video\" path=\"/absolute/path/to/file\"}`。该指令行不要附加解释文字；普通文本说明单独写在其他行。";

/// `team_artifacts_root`：团队会话下「项目成果」解析后的根目录；与 `agent_home/outbox` 二选一优先团队成果目录。
pub fn desktop_media_reply_prompt(agent_home: Option<&Path>, team_artifacts_root: Option<&Path>) -> String {
    let mut prompt = String::from(DESKTOP_MEDIA_REPLY_BASE);
    if let Some(root) = team_artifacts_root {
        prompt.push_str(" 生成给用户的正式产物时，不要只放在临时目录；**团队会话下请优先写入** `");
        prompt.push_str(&root.display().to_string());
        prompt.push_str("` 或其子目录（与侧栏「团队 · 成果」根目录一致），再在 `::nc-media` 里引用那个绝对路径。");
    } else if let Some(agent_home) = agent_home {
        let preferred_dir = agent_home.join("outbox");
        prompt.push_str(" 生成给用户的正式产物时，不要只放在临时目录；优先写到 `");
        prompt.push_str(&preferred_dir.display().to_string());
        prompt.push_str("` 或其子目录，再在 `::nc-media` 里引用那个绝对路径。");
    }
    prompt
}
