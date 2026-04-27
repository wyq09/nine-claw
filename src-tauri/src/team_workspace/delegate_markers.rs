use tauri::AppHandle;

use super::run_delegate_with_provider;
use crate::agents;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DelegateMarker {
    target_agent_id: String,
    task: String,
}

/// 计划协议：单项任务。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DelegatePlanItem {
    pub assignee: String,
    pub task: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct DelegatePlanPayload {
    items: Vec<DelegatePlanItem>,
}

/// 将 `NINECLAW_DELEGATE_PLAN_JSON:{...}` 行转换为 HTML 注释形式的内联占位。
pub fn expand_delegate_plan_markers_in_text(source_text: &str) -> Result<String, String> {
    let prefix = "NINECLAW_DELEGATE_PLAN_JSON:";
    let mut out_lines: Vec<String> = Vec::new();
    for line in source_text.lines() {
        let trimmed = line.trim();
        if let Some(json_part) = trimmed.strip_prefix(prefix) {
            let parsed: DelegatePlanPayload = serde_json::from_str(json_part.trim())
                .map_err(|e| format!("计划 JSON 无效: {e}"))?;
            let plan_id = uuid::Uuid::new_v4().to_string();
            let payload = serde_json::json!({
                "planId": plan_id,
                "items": parsed.items,
            });
            let payload_s =
                serde_json::to_string(&payload).map_err(|e| format!("序列化计划失败: {e}"))?;
            out_lines.push(format!("<!--NC_DELEGATE_PLAN:{}-->", payload_s));
            continue;
        }
        out_lines.push(line.to_string());
    }
    Ok(out_lines.join("\n"))
}

/// 解析并执行 `NINECLAW_DELEGATE_JSON:{...}` 行，返回去掉标记后的正文 + 委派输出拼接。
pub fn expand_delegate_markers_in_text(
    app: &AppHandle,
    workspace_id: &str,
    source_text: &str,
    provider: Option<&crate::ProviderRuntimeConfig>,
) -> Result<String, String> {
    let prefix = "NINECLAW_DELEGATE_JSON:";
    let mut parts: Vec<String> = Vec::new();
    for line in source_text.lines() {
        let trimmed = line.trim();
        if let Some(json_part) = trimmed.strip_prefix(prefix) {
            let parsed: DelegateMarker = serde_json::from_str(json_part.trim())
                .map_err(|e| format!("委派 JSON 无效: {e}"))?;
            let target_label = match agents::get_agent_record(app, &parsed.target_agent_id) {
                Ok(Some(r)) => format!("**{}** (`{}`)", r.name, r.id),
                _ => format!("`{}`", parsed.target_agent_id),
            };
            let block = match provider {
                None => "\n\n[委派未执行：缺少 Provider 配置]\n".to_string(),
                Some(p) => match run_delegate_with_provider(
                    app,
                    workspace_id,
                    &parsed.target_agent_id,
                    &parsed.task,
                    p,
                ) {
                    Ok(body) => {
                        format!("\n\n---\n### 子智能体委派 · {}\n\n{}\n", target_label, body)
                    }
                    Err(e) => format!("\n\n[委派失败: {}]\n", e),
                },
            };
            parts.push(block);
            continue;
        }
        parts.push(line.to_string());
    }
    Ok(parts.join("\n"))
}
