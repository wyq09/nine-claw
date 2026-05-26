use crate::agents::ConversationAgentConfig;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TurnContextBlock {
    pub label: String,
    pub content: String,
}

impl TurnContextBlock {
    pub fn new(label: impl Into<String>, content: impl Into<String>) -> Option<Self> {
        let label = label.into().trim().to_string();
        let content = content.into().trim().to_string();
        if label.is_empty() || content.is_empty() {
            None
        } else {
            Some(Self { label, content })
        }
    }
}

pub fn build_agent_dynamic_instruction_block(
    agent: &ConversationAgentConfig,
    current_prompt: &str,
) -> Option<TurnContextBlock> {
    let system_prompt = agent.system_prompt.trim();
    if system_prompt.is_empty() {
        return None;
    }
    let expanded_prompt = system_prompt.replace("${ARG}", current_prompt.trim());
    TurnContextBlock::new(
        "agent_dynamic_instruction",
        crate::prompts::render_agent_dynamic_instruction_context(&trim_user_dynamic_instruction(
            &expanded_prompt,
        )),
    )
}

pub fn build_workspace_memory_block(
    agent_id: &str,
    session_id: Option<&str>,
    current_prompt: &str,
) -> Result<Option<TurnContextBlock>, String> {
    let snapshot = crate::agent_workspace::build_session_workspace_system_prompt_for_query(
        agent_id,
        session_id,
        Some(current_prompt),
    )?;
    Ok(TurnContextBlock::new("workspace_memory", snapshot))
}

pub fn build_session_context_blocks(
    agent: &ConversationAgentConfig,
    session_id: Option<&str>,
    current_prompt: &str,
) -> Result<Vec<TurnContextBlock>, String> {
    let mut blocks = Vec::new();
    if let Some(block) = build_agent_dynamic_instruction_block(agent, current_prompt) {
        blocks.push(block);
    }
    if let Some(block) = build_workspace_memory_block(&agent.id, session_id, current_prompt)? {
        blocks.push(block);
    }
    Ok(blocks)
}

pub fn wrap_turn_context_message(
    original_message: &str,
    blocks: &[TurnContextBlock],
) -> String {
    let message = original_message.trim();
    let active_blocks = blocks
        .iter()
        .filter(|block| !block.label.trim().is_empty() && !block.content.trim().is_empty())
        .collect::<Vec<_>>();
    if active_blocks.is_empty() {
        return message.to_string();
    }

    let mut rendered = Vec::with_capacity(active_blocks.len() + 1);
    rendered.push("<nineclaw_turn_context>".to_string());
    for block in active_blocks {
        rendered.push(format!(
            "<context label=\"{}\">\n{}\n</context>",
            escape_xml_attr(&block.label),
            block.content.trim()
        ));
    }
    rendered.push("</nineclaw_turn_context>".to_string());

    if message.is_empty() {
        rendered.join("\n\n")
    } else {
        format!("{}\n\n{}", rendered.join("\n\n"), message)
    }
}

pub fn extend_with_block(blocks: &mut Vec<TurnContextBlock>, label: &str, content: impl Into<String>) {
    if let Some(block) = TurnContextBlock::new(label, content) {
        blocks.push(block);
    }
}

fn trim_user_dynamic_instruction(value: &str) -> String {
    const LIMIT: usize = 32_000;
    if value.chars().count() <= LIMIT {
        return value.trim().to_string();
    }

    let head = value.chars().take(LIMIT).collect::<String>();
    format!("{head}…\n(已截断至约 {LIMIT} 字，见仓库文档 docs/AGENT_SYSTEM_PROMPT.md)")
}

fn escape_xml_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_capabilities::AgentCapabilityPolicy;
    use crate::agent_workspace::{ensure_agent_workspace, AgentWorkspaceSeed};
    use crate::managed_runtime::{append_session_event, SessionEventKind};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::MutexGuard;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_WORKSPACE_ROOT_ENV: &str = "NINECLAW_WORKSPACE_ROOT";

    fn workspace_test_lock() -> MutexGuard<'static, ()> {
        crate::workspace_env_test_lock()
    }

    fn temp_workspace_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("nineclaw-turn-context-{nonce}"))
    }

    fn agent_with_prompt(system_prompt: &str) -> ConversationAgentConfig {
        ConversationAgentConfig {
            id: "agent".to_string(),
            name: "动态上下文测试".to_string(),
            summary: String::new(),
            description: String::new(),
            trigger_condition: String::new(),
            manual_trigger_only: false,
            system_prompt: system_prompt.to_string(),
            capability_policy: AgentCapabilityPolicy::default(),
            skill_ids: vec![],
            allowed_tool_ids: vec![],
            default_provider_id: "openai".to_string(),
            default_model: "gpt-4.1".to_string(),
            execution_mode: "single".to_string(),
            collaboration_config: None,
            accent_color: None,
            avatar_uri: None,
            scenario_llm_config: None,
            agent_loop_config: None,
        }
    }

    #[test]
    fn expands_arg_as_turn_context_not_system_material() {
        let block = build_agent_dynamic_instruction_block(
            &agent_with_prompt("请审查：${ARG}"),
            "合同条款",
        )
        .expect("dynamic block");

        assert_eq!(block.label, "agent_dynamic_instruction");
        assert!(block.content.contains("请审查：合同条款"));
        assert!(!block.content.contains("${ARG}"));
    }

    #[test]
    fn wraps_dynamic_blocks_before_user_message() {
        let message = wrap_turn_context_message(
            "用户原始问题",
            &[
                TurnContextBlock::new("workspace_memory", "Recent Session Events:\n- 事件")
                    .expect("block"),
                TurnContextBlock::new("team_preface", "团队上下文").expect("block"),
            ],
        );

        assert!(message.starts_with("<nineclaw_turn_context>"));
        assert!(message.contains("<context label=\"workspace_memory\">"));
        assert!(message.contains("Recent Session Events:"));
        assert!(message.ends_with("用户原始问题"));
    }

    #[test]
    fn builds_session_context_blocks_with_arg_before_workspace() {
        let _guard = workspace_test_lock();
        let root = temp_workspace_root();
        std::env::set_var(TEST_WORKSPACE_ROOT_ENV, &root);
        let home = ensure_agent_workspace(
            AgentWorkspaceSeed {
                id: "agent",
                name: "动态上下文测试",
                summary: "",
                description: "",
                accent_color: None,
                is_builtin: false,
            },
            true,
        )
        .expect("workspace");
        append_session_event(
            &home,
            "session-1",
            SessionEventKind::ToolCall,
            "调用 cargo test",
            None,
        )
        .expect("event");

        let dynamic_blocks = build_session_context_blocks(
            &agent_with_prompt("围绕 ${ARG} 执行"),
            Some("session-1"),
            "当前任务",
        )
        .expect("blocks");
        assert_eq!(dynamic_blocks.len(), 2);
        assert_eq!(dynamic_blocks[0].label, "agent_dynamic_instruction");
        assert!(dynamic_blocks[0].content.contains("围绕 当前任务 执行"));
        assert!(!dynamic_blocks[0].content.contains("${ARG}"));
        assert_eq!(dynamic_blocks[1].label, "workspace_memory");
        assert!(dynamic_blocks[1].content.contains("Recent Session Events:"));
        assert!(dynamic_blocks[1].content.contains("调用 cargo test"));

        let message = wrap_turn_context_message("用户消息", &dynamic_blocks);
        assert!(message.contains("<context label=\"agent_dynamic_instruction\">"));
        assert!(message.contains("<context label=\"workspace_memory\">"));
        assert!(message.ends_with("用户消息"));

        let _ = fs::remove_dir_all(root);
        std::env::remove_var(TEST_WORKSPACE_ROOT_ENV);
    }
}
