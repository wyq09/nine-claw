//! `image_analyze` 加入默认工具白名单后，存量「上一版默认列表」智能体的升级测试。

use super::*;

fn vision_test_connection() -> Connection {
    Connection::open_in_memory().expect("in-memory db")
}

#[test]
fn upgrades_previous_default_tool_list_to_include_image_analyze() {
    let connection = vision_test_connection();
    ensure_agents_schema(&connection).expect("ensure schema");
    let previous_tools = previous_default_allowed_tool_ids();
    assert!(!previous_tools.contains(&"image_analyze".to_string()));
    connection
        .execute(
            "INSERT INTO agents (id, name, summary, description, trigger_condition, manual_trigger_only, system_prompt, \
             default_provider_id, default_model, is_builtin, is_archived, execution_mode, collaboration_config_json, \
             heartbeat_config_json, capability_policy_json, allowed_tool_ids_json, accent_color, avatar_uri, \
             scenario_llm_config_json, agent_loop_config_json, created_at, updated_at) \
             VALUES ('previous-default', 'Previous Default', '', '', '', 0, '', 'openai', 'gpt-4.1', 0, 0, 'single', \
             NULL, NULL, NULL, ?1, NULL, NULL, NULL, NULL, 1, 1)",
            params![serialize_allowed_tool_ids(&previous_tools).expect("previous default tools")],
        )
        .expect("insert previous default");

    upgrade_legacy_default_allowed_tools(&connection).expect("upgrade defaults");

    let upgraded = get_active_agent_by_id(&connection, "previous-default")
        .expect("get previous default")
        .expect("previous default agent");
    assert_eq!(upgraded.allowed_tool_ids, default_allowed_tool_ids());
    assert!(upgraded.allowed_tool_ids.contains(&"image_analyze".to_string()));
}