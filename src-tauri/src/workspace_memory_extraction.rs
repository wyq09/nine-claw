use crate::agent_capabilities::AgentCapabilityPolicy;
use crate::agents::{self, AgentRecord};
use crate::channels::pi_bridge::{PiBridge, PiProcessOutcome};
use crate::dev_trace::dev_trace;
use crate::llm_trace;
use crate::pi_runtime;
use crate::prompts;
use crate::provider_runtime::{normalized_provider_runtime_base_url, resolve_im_llm_runtime};
use crate::storage::chat_history::ChatTurn;
use crate::storage::workspaces::{self, WorkspaceMemoryRecord};
use crate::team_workspace;
use std::collections::HashSet;
use tauri::AppHandle;
use uuid::Uuid;

const MEMORY_EXTRACTION_LLM_CHUNK: usize = 512;
const MAX_RECENT_TURNS: usize = 6;
const MAX_RECENT_MEMORIES: i64 = 12;
const MAX_MEMORY_ITEMS: usize = 3;
const MAX_TITLE_CHARS: usize = 24;
const MAX_CONTENT_CHARS: usize = 220;
const MAX_TURN_TEXT_CHARS: usize = 1200;
const MAX_MEMORY_PREVIEW_CHARS: usize = 180;

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkspaceConversationTurn {
    turn_index: i32,
    speaker_label: String,
    user_prompt: String,
    assistant_reply: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExtractedWorkspaceMemory {
    title: String,
    content: String,
    tags: Vec<String>,
}

#[derive(Clone, Debug)]
struct WorkspaceMemoryExtractionRequest {
    workspace_id: String,
    session_id: String,
    latest_user_prompt: String,
    latest_assistant_reply: String,
    speaker_agent_id: Option<String>,
    speaker_name: Option<String>,
}

fn clamp_chars(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || max_chars == 0 {
        return String::new();
    }
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    format!(
        "{}…",
        trimmed
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>()
    )
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_memory_fingerprint(value: &str) -> String {
    normalize_whitespace(value).to_lowercase()
}

fn canonical_tag(tag: &str) -> Option<String> {
    match tag.trim().to_ascii_lowercase().as_str() {
        "decision" | "决策" => Some("decision".to_string()),
        "constraint" | "constraints" | "约束" | "限制" => Some("constraint".to_string()),
        "fact" | "facts" | "事实" => Some("fact".to_string()),
        "preference" | "preferences" | "偏好" => Some("preference".to_string()),
        "resource" | "resources" | "资料" | "资源" => Some("resource".to_string()),
        "plan" | "plans" | "计划" => Some("plan".to_string()),
        "risk" | "risks" | "风险" => Some("risk".to_string()),
        "workflow" | "workflows" | "流程" | "工作流" => Some("workflow".to_string()),
        "people" | "person" | "成员" | "人员" => Some("people".to_string()),
        _ => None,
    }
}

fn resolve_memory_extraction_model(record: &AgentRecord) -> (String, String) {
    if let Some(slot) = record
        .scenario_llm_config
        .as_ref()
        .and_then(|config| config.memory_extraction.as_ref())
    {
        let provider_id = slot.provider_id.trim();
        let model = slot.model.trim();
        if !provider_id.is_empty() && !model.is_empty() {
            return (provider_id.to_string(), model.to_string());
        }
    }
    (
        record.default_provider_id.trim().to_string(),
        record.default_model.trim().to_string(),
    )
}

fn render_recent_memories(memories: &[WorkspaceMemoryRecord]) -> String {
    if memories.is_empty() {
        return "_暂无_".to_string();
    }
    let mut lines = Vec::new();
    for memory in memories {
        let title = clamp_chars(&memory.title, MAX_TITLE_CHARS);
        let content = clamp_chars(&memory.content, MAX_MEMORY_PREVIEW_CHARS);
        if content.is_empty() {
            lines.push(format!("- {}", title));
        } else {
            lines.push(format!("- {}：{}", title, content));
        }
    }
    lines.join("\n")
}

fn render_conversation_excerpt(turns: &[WorkspaceConversationTurn]) -> String {
    if turns.is_empty() {
        return "_暂无_".to_string();
    }
    let mut lines = Vec::new();
    for turn in turns {
        lines.push(format!(
            "### 第 {} 轮 · {}",
            turn.turn_index, turn.speaker_label
        ));
        lines.push(format!(
            "用户：{}",
            clamp_chars(&turn.user_prompt, MAX_TURN_TEXT_CHARS)
        ));
        lines.push(format!(
            "助手：{}",
            clamp_chars(&turn.assistant_reply, MAX_TURN_TEXT_CHARS)
        ));
        lines.push(String::new());
    }
    lines.join("\n").trim().to_string()
}

fn build_conversation_turns(
    session_turns: &[ChatTurn],
    latest_user_prompt: &str,
    latest_assistant_reply: &str,
    speaker_label: &str,
) -> Vec<WorkspaceConversationTurn> {
    let mut turns: Vec<WorkspaceConversationTurn> = session_turns
        .iter()
        .rev()
        .filter(|turn| !turn.prompt.trim().is_empty() || !turn.answer.trim().is_empty())
        .take(MAX_RECENT_TURNS)
        .map(|turn| WorkspaceConversationTurn {
            turn_index: turn.turn_index,
            speaker_label: turn
                .speaker_agent_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "团队会话".to_string()),
            user_prompt: turn.prompt.trim().to_string(),
            assistant_reply: turn.answer.trim().to_string(),
        })
        .collect();
    turns.reverse();

    let latest_user_prompt = latest_user_prompt.trim();
    let latest_assistant_reply = latest_assistant_reply.trim();
    if latest_user_prompt.is_empty() || latest_assistant_reply.is_empty() {
        return turns;
    }

    let already_has_latest = turns.last().is_some_and(|turn| {
        normalize_memory_fingerprint(&turn.user_prompt)
            == normalize_memory_fingerprint(latest_user_prompt)
            && normalize_memory_fingerprint(&turn.assistant_reply)
                == normalize_memory_fingerprint(latest_assistant_reply)
    });
    if !already_has_latest {
        let next_index = turns.last().map(|turn| turn.turn_index + 1).unwrap_or(1);
        turns.push(WorkspaceConversationTurn {
            turn_index: next_index,
            speaker_label: speaker_label.trim().to_string(),
            user_prompt: latest_user_prompt.to_string(),
            assistant_reply: latest_assistant_reply.to_string(),
        });
    }

    if turns.len() > MAX_RECENT_TURNS {
        turns = turns.split_off(turns.len() - MAX_RECENT_TURNS);
    }
    turns
}

fn parse_memories_from_value(value: &serde_json::Value) -> Vec<ExtractedWorkspaceMemory> {
    let items = if let Some(memories) = value.get("memories").and_then(|item| item.as_array()) {
        memories.clone()
    } else if let Some(array) = value.as_array() {
        array.clone()
    } else if value.get("title").is_some() || value.get("content").is_some() {
        vec![value.clone()]
    } else {
        Vec::new()
    };

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        let Some(title) = item
            .get("title")
            .and_then(|field| field.as_str())
            .map(|field| clamp_chars(field, MAX_TITLE_CHARS))
            .filter(|field| !field.is_empty())
        else {
            continue;
        };
        let Some(content) = item
            .get("content")
            .and_then(|field| field.as_str())
            .map(|field| clamp_chars(field, MAX_CONTENT_CHARS))
            .filter(|field| !field.is_empty())
        else {
            continue;
        };
        let mut tags = item
            .get("tags")
            .and_then(|field| field.as_array())
            .map(|tags| {
                tags.iter()
                    .filter_map(|tag| tag.as_str())
                    .filter_map(canonical_tag)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        tags.dedup();
        if tags.is_empty() {
            tags.push("fact".to_string());
        }
        if tags.len() > 4 {
            tags.truncate(4);
        }

        let fingerprint = format!(
            "{}|{}",
            normalize_memory_fingerprint(&title),
            normalize_memory_fingerprint(&content)
        );
        if !seen.insert(fingerprint) {
            continue;
        }
        out.push(ExtractedWorkspaceMemory {
            title,
            content,
            tags,
        });
        if out.len() >= MAX_MEMORY_ITEMS {
            break;
        }
    }
    out
}

fn parse_workspace_memory_llm_output(raw: &str) -> Vec<ExtractedWorkspaceMemory> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut candidates = vec![trimmed.to_string()];
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if end >= start {
            candidates.push(trimmed[start..=end].to_string());
        }
    }
    if let (Some(start), Some(end)) = (trimmed.find('['), trimmed.rfind(']')) {
        if end >= start {
            candidates.push(trimmed[start..=end].to_string());
        }
    }

    for candidate in candidates {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&candidate) {
            let parsed = parse_memories_from_value(&value);
            if !parsed.is_empty() || candidate.contains("\"memories\"") {
                return parsed;
            }
        }
    }
    Vec::new()
}

fn memory_exists(candidate: &ExtractedWorkspaceMemory, existing: &[WorkspaceMemoryRecord]) -> bool {
    let candidate_title = normalize_memory_fingerprint(&candidate.title);
    let candidate_content = normalize_memory_fingerprint(&candidate.content);
    existing.iter().any(|memory| {
        let existing_title = normalize_memory_fingerprint(&memory.title);
        let existing_content = normalize_memory_fingerprint(&memory.content);
        (candidate_title == existing_title && candidate_content == existing_content)
            || (!candidate_title.is_empty()
                && candidate_title == existing_title
                && (candidate_content.contains(&existing_content)
                    || existing_content.contains(&candidate_content)))
            || (!candidate_content.is_empty() && candidate_content == existing_content)
    })
}

fn run_workspace_memory_extraction(
    app: &AppHandle,
    request: WorkspaceMemoryExtractionRequest,
) -> Result<usize, String> {
    if request.latest_user_prompt.trim().is_empty()
        || request.latest_assistant_reply.trim().is_empty()
    {
        return Ok(0);
    }

    let conn = crate::storage_conn(app)?;
    let Some(workspace) = workspaces::get_workspace(&conn, &request.workspace_id)? else {
        return Err("工作空间不存在".to_string());
    };
    let Some(supervisor_record) = agents::get_agent_record(app, &workspace.supervisor_agent_id)?
    else {
        return Err("主智能体不存在".to_string());
    };

    let (provider_id, model) = resolve_memory_extraction_model(&supervisor_record);
    let runtime = resolve_im_llm_runtime(app, &provider_id, &model)?;
    let base_normalized = normalized_provider_runtime_base_url(
        &runtime.base_url,
        &runtime.api_format,
        &runtime.provider_id,
    );
    let pi_rt = pi_runtime::require_pi_runtime_location(app)?;
    let recent_memories =
        workspaces::list_workspace_memories(&conn, &request.workspace_id, MAX_RECENT_MEMORIES)?;
    let session_turns = crate::storage::chat_history::list_chat_turns(&conn, &request.session_id)
        .unwrap_or_default();
    let speaker_label = request
        .speaker_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            request
                .speaker_agent_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("当前发言智能体");
    let conversation_turns = build_conversation_turns(
        &session_turns,
        &request.latest_user_prompt,
        &request.latest_assistant_reply,
        speaker_label,
    );
    let prompt = prompts::build_workspace_memory_extraction_prompt(
        workspace.name.trim(),
        supervisor_record.name.trim(),
        &render_recent_memories(&recent_memories),
        &render_conversation_excerpt(&conversation_turns),
    );

    let bridge = PiBridge::new(
        pi_rt,
        &runtime.provider_id,
        &runtime.api_format,
        &base_normalized,
        &runtime.api_key,
        &runtime.model,
        None,
    );
    let channel_id = format!("nc:workspace-memory:{}", request.workspace_id);
    let user_id = format!(
        "mem_extract:{}:{}",
        request.session_id,
        Uuid::new_v4().simple()
    );
    let mut trace_guard = if workspace.llm_trace_enabled != 0 {
        Some(llm_trace::TraceGuard::new(
            app,
            llm_trace::begin(
                app,
                Some(&request.workspace_id),
                "action_llm",
                "action:workspace_memory_extraction",
                "记忆提取",
                None,
                Some("LLM"),
                Some(&request.session_id),
                Some(&runtime.provider_id),
                Some(&runtime.model),
                vec![llm_trace::TraceSystemPromptSection {
                    label: "workspace_memory_extraction".to_string(),
                    content: "提取团队共享记忆候选条目，并返回结构化 JSON。".to_string(),
                }],
                &prompt,
            ),
        ))
    } else {
        None
    };
    let trace_id = trace_guard
        .as_ref()
        .and_then(|guard| guard.id())
        .map(ToOwned::to_owned);
    let trace_id_for_chunk = trace_id.clone();
    let trace_id_for_event = trace_id.clone();
    let outcome = bridge.process_message_interruptible_with_events(
        &channel_id,
        &user_id,
        &prompt,
        MEMORY_EXTRACTION_LLM_CHUNK,
        |chunk| {
            if let Some(tid) = trace_id_for_chunk.as_deref() {
                llm_trace::append_response(app, tid, chunk);
            }
        },
        |_| {},
        |event| {
            let Some(tid) = trace_id_for_event.as_deref() else {
                return;
            };
            if let Some(delta) = event.get("assistantMessageEvent").and_then(|evt| {
                if evt.get("type").and_then(|value| value.as_str()) == Some("thinking_delta") {
                    evt.get("delta").and_then(|value| value.as_str())
                } else {
                    None
                }
            }) {
                llm_trace::append_thinking(app, tid, delta);
            }
        },
    )?;

    let text = match outcome {
        PiProcessOutcome::Completed(result) => {
            if let Some(guard) = trace_guard.as_mut() {
                guard.finalize_done(
                    Some(result.full_text.clone()),
                    None,
                    Some(runtime.provider_id.clone()),
                    Some(runtime.model.clone()),
                    None,
                );
            }
            result.full_text
        }
        PiProcessOutcome::Aborted => {
            if let Some(guard) = trace_guard.as_mut() {
                guard.finalize_error("记忆提取被中断".to_string());
            }
            return Ok(0);
        }
    };
    let memories = parse_workspace_memory_llm_output(&text);
    if memories.is_empty() {
        return Ok(0);
    }

    let mut inserted = 0usize;
    for memory in memories {
        if memory_exists(&memory, &recent_memories) {
            continue;
        }
        team_workspace::write_team_memory_entry(
            app,
            &request.workspace_id,
            memory.title,
            memory.content,
            Some(workspace.supervisor_agent_id.clone()),
            memory.tags,
        )?;
        inserted += 1;
    }
    Ok(inserted)
}

pub(crate) fn spawn_workspace_memory_extraction(
    app: &AppHandle,
    workspace_id: &str,
    session_id: &str,
    latest_user_prompt: &str,
    latest_assistant_reply: &str,
    speaker_agent_id: Option<&str>,
    speaker_name: Option<&str>,
) {
    let workspace_id = workspace_id.trim();
    let session_id = session_id.trim();
    if workspace_id.is_empty()
        || session_id.is_empty()
        || latest_user_prompt.trim().is_empty()
        || latest_assistant_reply.trim().is_empty()
    {
        return;
    }

    let request = WorkspaceMemoryExtractionRequest {
        workspace_id: workspace_id.to_string(),
        session_id: session_id.to_string(),
        latest_user_prompt: latest_user_prompt.trim().to_string(),
        latest_assistant_reply: latest_assistant_reply.trim().to_string(),
        speaker_agent_id: speaker_agent_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        speaker_name: speaker_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    };
    let app = app.clone();
    dev_trace(
        "workspace.memory",
        format!(
            "schedule extraction: workspace={} session={}",
            request.workspace_id, request.session_id
        ),
    );
    let _ = tauri::async_runtime::spawn_blocking(move || {
        match run_workspace_memory_extraction(&app, request.clone()) {
            Ok(inserted) => dev_trace(
                "workspace.memory",
                format!(
                    "extraction finished: workspace={} session={} inserted={}",
                    request.workspace_id, request.session_id, inserted
                ),
            ),
            Err(error) => dev_trace(
                "workspace.memory",
                format!(
                    "extraction failed: workspace={} session={} error={}",
                    request.workspace_id, request.session_id, error
                ),
            ),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_row(title: &str, content: &str) -> WorkspaceMemoryRecord {
        WorkspaceMemoryRecord {
            id: "m1".to_string(),
            workspace_id: "ws-1".to_string(),
            title: title.to_string(),
            content: content.to_string(),
            author_agent_id: None,
            tags_json: "[]".to_string(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn resolve_memory_extraction_model_prefers_scenario_slot() {
        let record = AgentRecord {
            id: "sup".to_string(),
            name: "Supervisor".to_string(),
            summary: String::new(),
            description: String::new(),
            system_prompt: String::new(),
            capability_policy: AgentCapabilityPolicy::default(),
            skill_ids: Vec::new(),
            default_provider_id: "default-provider".to_string(),
            default_model: "default-model".to_string(),
            is_builtin: false,
            is_archived: false,
            execution_mode: "desktop".to_string(),
            collaboration_config: None,
            accent_color: None,
            bot_configs: Default::default(),
            heartbeat_config: Default::default(),
            scenario_llm_config: Some(crate::agents::AgentScenarioLlmConfig {
                title_generation: None,
                memory_extraction: Some(crate::agents::AgentScenarioLlmSlot {
                    provider_id: "memory-provider".to_string(),
                    model: "memory-model".to_string(),
                }),
                task_push_notification_copy: None,
            }),
            created_at: 0,
            updated_at: 0,
        };

        assert_eq!(
            resolve_memory_extraction_model(&record),
            ("memory-provider".to_string(), "memory-model".to_string())
        );
    }

    #[test]
    fn build_conversation_turns_appends_latest_round_when_missing() {
        let turns = build_conversation_turns(&[], "用户提问", "助手回复", "主智能体");
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].turn_index, 1);
        assert_eq!(turns[0].speaker_label, "主智能体");
    }

    #[test]
    fn parse_workspace_memory_llm_output_accepts_json_wrapper() {
        let raw = "下面是结果：{\"memories\":[{\"title\":\"部署约束\",\"content\":\"产物统一写入团队 artifacts 目录。\",\"tags\":[\"constraint\",\"resource\"]}]}";
        let parsed = parse_workspace_memory_llm_output(raw);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].title, "部署约束");
        assert_eq!(
            parsed[0].tags,
            vec!["constraint".to_string(), "resource".to_string()]
        );
    }

    #[test]
    fn parse_workspace_memory_llm_output_normalizes_tags_and_deduplicates() {
        let raw = r#"{"memories":[
          {"title":"团队偏好","content":"默认输出中文。","tags":["偏好","fact"]},
          {"title":"团队偏好","content":"默认输出中文。","tags":["preference"]}
        ]}"#;
        let parsed = parse_workspace_memory_llm_output(raw);
        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0].tags,
            vec!["preference".to_string(), "fact".to_string()]
        );
    }

    #[test]
    fn memory_exists_matches_same_title_and_content() {
        let candidate = ExtractedWorkspaceMemory {
            title: "团队约束".to_string(),
            content: "所有成果写入 artifacts 目录。".to_string(),
            tags: vec!["constraint".to_string()],
        };
        assert!(memory_exists(
            &candidate,
            &[memory_row("团队约束", "所有成果写入 artifacts 目录。")]
        ));
        assert!(!memory_exists(
            &candidate,
            &[memory_row("别的标题", "完全不同内容")]
        ));
    }
}
