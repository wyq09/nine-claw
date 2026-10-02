//! session_fork 单元测试:PI JSONL 切点、事件流截断与 fork 编排。

use super::*;
use crate::storage::chat_history::{
    append_chat_turn_allocating_index as append_turn_row, create_chat_session,
    list_chat_turns, AppendChatTurnInput, CreateChatSessionInput,
};
use crate::storage::db::open_in_memory;

fn temp_root(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "nineclaw-session-fork-{tag}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn header_line_json(id: &str) -> String {
    format!(
        r#"{{"type":"session","version":3,"id":"{id}","timestamp":"2026-08-18T00:00:00.000Z","cwd":"/tmp/demo"}}"#
    )
}

fn user_line(id: &str, parent: Option<&str>, text: &str) -> String {
    let parent_json = match parent {
        Some(value) => format!(r#""{value}""#),
        None => "null".to_string(),
    };
    format!(
        r#"{{"type":"message","id":"{id}","parentId":{parent_json},"timestamp":"2026-08-18T00:00:01.000Z","message":{{"role":"user","content":{}}}}}"#,
        serde_json::to_string(text).unwrap()
    )
}

fn user_line_parts(id: &str, text: &str) -> String {
    format!(
        r#"{{"type":"message","id":"{id}","parentId":null,"timestamp":"2026-08-18T00:00:01.000Z","message":{{"role":"user","content":[{{"type":"text","text":{}}}]}}}}"#,
        serde_json::to_string(text).unwrap()
    )
}

fn assistant_line(id: &str, parent: &str, text: &str) -> String {
    format!(
        r#"{{"type":"message","id":"{id}","parentId":"{parent}","timestamp":"2026-08-18T00:00:02.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":{}}}]}}}}"#,
        serde_json::to_string(text).unwrap()
    )
}

#[test]
fn user_message_text_supports_string_and_parts() {
    let string_entry: Value = serde_json::from_str(&user_line("a", None, "你好")).unwrap();
    assert_eq!(
        user_message_text(&string_entry).as_deref(),
        Some("你好")
    );
    let parts_entry: Value =
        serde_json::from_str(&user_line_parts("b", "分块内容")).unwrap();
    assert_eq!(
        user_message_text(&parts_entry).as_deref(),
        Some("分块内容")
    );
    let assistant: Value =
        serde_json::from_str(&assistant_line("c", "b", "回复")).unwrap();
    assert_eq!(user_message_text(&assistant), None);
}

#[test]
fn plan_prefix_cuts_before_next_user_entry() {
    let raw = [
        header_line_json("src"),
        user_line("u0", None, "第一问"),
        assistant_line("a0", "u0", "第一答"),
        user_line("u1", Some("a0"), "第二问"),
        assistant_line("a1", "u1", "第二答"),
        user_line("u2", Some("a1"), "第三问"),
        assistant_line("a2", "u2", "第三答"),
    ]
    .join("\n");
    let allowed = vec!["第一问".to_string(), "第二问".to_string()];
    let prefix = plan_pi_session_prefix(&raw, &allowed);
    assert_eq!(prefix.matched_user_entries, 2);
    assert_eq!(prefix.kept_lines.len(), 4);
    assert!(prefix.kept_lines.iter().all(|line| !line.contains("第三")));
    assert!(prefix.kept_lines[0].contains("第一问"));
    assert!(prefix.kept_lines[3].contains("第二答"));
}

#[test]
fn plan_prefix_matches_wrapped_prompt_suffix() {
    let wrapped = format!(
        "<nineclaw_turn_context>\n<context label=\"a\">上下文</context>\n</nineclaw_turn_context>\n\n{}",
        "真实提问"
    );
    let raw = [
        header_line_json("src"),
        user_line("u0", None, &wrapped),
        assistant_line("a0", "u0", "答"),
    ]
    .join("\n");
    let prefix = plan_pi_session_prefix(&raw, &["真实提问".to_string()]);
    assert_eq!(prefix.matched_user_entries, 1);
    assert_eq!(prefix.kept_lines.len(), 2);
}

#[test]
fn plan_prefix_skips_multimodal_turn_absent_from_jsonl() {
    // turn1 是多模态轮：主 JSONL 里没有它的 user 消息。
    let raw = [
        header_line_json("src"),
        user_line("u0", None, "看下这张图"),
        assistant_line("a0", "u0", "图里是封面"),
        user_line("u2", Some("a0"), "继续分析文本"),
        assistant_line("a2", "u2", "文本结论"),
    ]
    .join("\n");
    let allowed = vec![
        "看下这张图".to_string(),
        "图片追问（不在主 JSONL）".to_string(),
        "继续分析文本".to_string(),
    ];
    let prefix = plan_pi_session_prefix(&raw, &allowed);
    assert_eq!(prefix.matched_user_entries, 2);
    assert_eq!(prefix.kept_lines.len(), 4);
    assert!(prefix.kept_lines[3].contains("文本结论"));
}

#[test]
fn plan_prefix_falls_back_to_ordinal_cut_when_no_match() {
    let raw = [
        header_line_json("src"),
        user_line("u0", None, "alpha"),
        assistant_line("a0", "u0", "回复0"),
        user_line("u1", Some("a0"), "beta"),
        assistant_line("a1", "u1", "回复1"),
    ]
    .join("\n");
    let allowed = vec!["完全不匹配".to_string(), "也不匹配".to_string()];
    let prefix = plan_pi_session_prefix(&raw, &allowed);
    assert_eq!(prefix.matched_user_entries, 0);
    // 兜底：按序数保留前 2 个 user 条目及第 2 轮的后续条目（与匹配
    // 路径语义一致——含 fork 点轮次的完整块）。
    assert_eq!(prefix.kept_lines.len(), 4);
    assert!(prefix.kept_lines[3].contains("回复1"));
}

#[test]
fn plan_prefix_empty_source_keeps_nothing() {
    let prefix = plan_pi_session_prefix("", &["任意".to_string()]);
    assert!(prefix.kept_lines.is_empty());
    let prefix = plan_pi_session_prefix("垃圾行\n", &[]);
    assert!(prefix.kept_lines.is_empty());
}

#[test]
fn render_fork_header_reuses_source_fields_and_links_parent() {
    let source = read_session_header(&header_line_json("src")).unwrap();
    let parent = Path::new("/data/pi-sessions/source.jsonl");
    let line = render_fork_header(
        Some(&source),
        "new-session",
        "2026-08-18T01:02:03.000Z",
        Path::new("/fallback"),
        parent,
    )
    .unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["type"], "session");
    assert_eq!(value["version"], 3);
    assert_eq!(value["id"], "new-session");
    assert_eq!(value["cwd"], "/tmp/demo");
    assert_eq!(value["parentSession"].as_str(), Some(parent.to_string_lossy().as_ref()));
    assert_eq!(value["timestamp"], "2026-08-18T01:02:03.000Z");
}

#[test]
fn render_fork_header_synthesizes_when_missing() {
    let line = render_fork_header(
        None,
        "fresh",
        "2026-08-18T01:02:03.000Z",
        Path::new("/agent/home"),
        Path::new("/source.jsonl"),
    )
    .unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["version"], 3);
    assert_eq!(value["cwd"], "/agent/home");
    assert_eq!(value["id"], "fresh");
}

#[test]
fn rewrite_events_truncates_at_nth_prompt_and_rewrites_session() {
    let events = [
        r#"{"id":"e1","session_id":"src","kind":"mode_set","summary":"execution_mode: single","detail":{"mode":"single"},"created_at":1}"#,
        r#"{"id":"e2","session_id":"src","kind":"prompt","summary":"第一问","created_at":2}"#,
        r#"{"id":"e3","session_id":"src","kind":"mode_set","summary":"execution_mode: worker","detail":{"mode":"worker"},"created_at":3}"#,
        r#"{"id":"e4","session_id":"src","kind":"prompt","summary":"第二问","created_at":4}"#,
        r#"{"id":"e5","session_id":"src","kind":"assistant_output","summary":"回复","created_at":5}"#,
    ]
    .join("\n");
    let kept = rewrite_events_for_fork(&events, "branch", 1);
    assert_eq!(kept.len(), 2);
    assert!(kept[0].contains("\"mode_set\""));
    assert!(kept[0].contains("\"mode\":\"single\""));
    assert!(kept.iter().all(|line| line.contains("\"session_id\":\"branch\"")));
    assert!(kept.iter().all(|line| !line.contains("\"session_id\":\"src\"")));
    // fold 语义：截断后最后一条 mode_set 是 single，与 fork 点一致。
    let modes: Vec<String> = kept
        .iter()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|value| value["kind"] == "mode_set")
        .filter_map(|value| {
            value["detail"]["mode"]
                .as_str()
                .map(ToOwned::to_owned)
        })
        .collect();
    assert_eq!(modes.last().map(String::as_str), Some("single"));
}

fn make_session(conn: &Connection, id: &str) {
    create_chat_session(
        conn,
        &CreateChatSessionInput {
            id: id.to_string(),
            title: format!("会话 {id}"),
            status: "done".to_string(),
            agent_id: None,
            agent_snapshot_json: None,
            bot_target_json: None,
            session_llm_provider_id: None,
            session_llm_model: None,
            workspace_id: None,
        },
    )
    .unwrap();
}

fn append_turn(conn: &Connection, session_id: &str, id: &str, prompt: &str) {
    append_turn_row(
        conn,
        &AppendChatTurnInput {
            id: id.to_string(),
            session_id: session_id.to_string(),
            turn_index: 0,
            prompt: prompt.to_string(),
            answer: format!("回答 {id}"),
            thinking: String::new(),
            status: "done".to_string(),
            usage_json: None,
            response_segments_json: None,
            tool_calls_json: None,
            activity_json: None,
            speaker_agent_id: None,
        },
    )
    .unwrap();
}

fn fork_params(mode: &str) -> ChatForkParams {
    ChatForkParams {
        source_session_id: "src".to_string(),
        fork_turn_id: "t1".to_string(),
        new_session_id: "branch-1".to_string(),
        workspace_mode: mode.to_string(),
    }
}

#[test]
fn fork_chat_session_copies_turns_and_jsonl() {
    let _guard = crate::workspace_env_test_lock();
    let root = temp_root("core");
    std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);
    let conn = open_in_memory().unwrap();
    make_session(&conn, "src");
    append_turn(&conn, "src", "t0", "第一问");
    append_turn(&conn, "src", "t1", "第二问");
    append_turn(&conn, "src", "t2", "第三问");
    // 源会话先初始化工作区，share 模式下分支应共用同一目录。
    let src_topic = crate::session_workspace::create_default_for_new_session(
        &conn,
        "src",
        None,
    )
    .unwrap()
    .topic_workspace_dir;

    let source_jsonl = root.join("source.jsonl");
    let raw = [
        header_line_json("src"),
        user_line("u0", None, "第一问"),
        assistant_line("a0", "u0", "回答 t0"),
        user_line("u1", Some("a0"), "第二问"),
        assistant_line("a1", "u1", "回答 t1"),
        user_line("u2", Some("a1"), "第三问"),
        assistant_line("a2", "u2", "回答 t2"),
    ]
    .join("\n");
    fs::write(&source_jsonl, &raw).unwrap();
    let target_jsonl = root.join("branch.jsonl");

    let outcome = fork_chat_session(
        &conn,
        &fork_params(WORKSPACE_MODE_SHARE),
        &ChatForkRuntimePaths {
            source_jsonl: source_jsonl.clone(),
            target_jsonl: target_jsonl.clone(),
            fallback_cwd: root.clone(),
        },
    )
    .unwrap();

    assert_eq!(outcome.copied_turns, 2);
    // SQLite：新会话两轮，标题带分支前缀，源会话不受影响。
    let branch_turns = list_chat_turns(&conn, "branch-1").unwrap();
    assert_eq!(branch_turns.len(), 2);
    assert_eq!(branch_turns[0].prompt, "第一问");
    assert_eq!(branch_turns[1].prompt, "第二问");
    assert_eq!(branch_turns[1].turn_index, 1);
    assert_eq!(list_chat_turns(&conn, "src").unwrap().len(), 3);
    let branch_session =
        storage::chat_history::get_chat_session(&conn, "branch-1").unwrap().unwrap();
    assert!(branch_session.title.starts_with("分支 · 会话 src"));
    // JSONL：新 header + 截断条目，不含第三轮。
    let target_raw = fs::read_to_string(&target_jsonl).unwrap();
    let first_line = target_raw.lines().next().unwrap();
    let header: Value = serde_json::from_str(first_line).unwrap();
    assert_eq!(header["type"], "session");
    assert_eq!(header["id"], "branch-1");
    assert_eq!(
        header["parentSession"].as_str(),
        Some(source_jsonl.to_string_lossy().as_ref())
    );
    assert!(target_raw.contains("回答 t1"));
    assert!(!target_raw.contains("第三问"));
    // share 模式：新会话沿用源会话目录。
    let dirs = storage::session_workspace::get_session_workspace_dirs(&conn, "branch-1")
        .unwrap()
        .unwrap();
    assert_eq!(dirs.topic_workspace_dir.as_deref(), Some(src_topic.as_str()));

    let _ = fs::remove_dir_all(root);
    std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
}

#[test]
fn forked_session_detail_includes_copied_turns() {
    let _guard = crate::workspace_env_test_lock();
    let root = temp_root("detail");
    std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);
    let conn = open_in_memory().unwrap();
    make_session(&conn, "src");
    append_turn(&conn, "src", "t0", "第一问");
    append_turn(&conn, "src", "t1", "第二问");
    append_turn(&conn, "src", "t2", "第三问");

    fork_chat_session(
        &conn,
        &fork_params(WORKSPACE_MODE_SHARE),
        &ChatForkRuntimePaths {
            source_jsonl: root.join("none.jsonl"),
            target_jsonl: root.join("branch.jsonl"),
            fallback_cwd: root.clone(),
        },
    )
    .unwrap();

    // 命令层返回给前端的详情必须带上复制的轮次，否则新分支会话渲染为空。
    // 按序列化后的 IPC 载荷断言，同时锁定前端依赖的字段名。
    let detail = forked_session_detail(&conn, "branch-1").unwrap();
    let payload: Value =
        serde_json::to_value(&detail).expect("serialize ChatSessionDetail");
    assert_eq!(payload["id"], "branch-1");
    assert!(payload["title"].as_str().unwrap().starts_with("分支 · 会话 src"));
    let turns = payload["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0]["prompt"], "第一问");
    assert_eq!(turns[1]["prompt"], "第二问");
    assert_eq!(turns[1]["turn_index"], 1);

    let _ = fs::remove_dir_all(root);
    std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
}

#[test]
fn fork_chat_session_copy_mode_duplicates_workspace_files() {
    let _guard = crate::workspace_env_test_lock();
    let root = temp_root("copy");
    std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);
    let conn = open_in_memory().unwrap();
    make_session(&conn, "src");
    append_turn(&conn, "src", "t0", "第一问");
    append_turn(&conn, "src", "t1", "第二问");
    // 先用 share 流程给源会话建默认工作区，再塞一个文件。
    let src_state = crate::session_workspace::create_default_for_new_session(
        &conn,
        "src",
        None,
    )
    .unwrap();
    fs::write(
        Path::new(&src_state.topic_workspace_dir).join("artifact.md"),
        "产物",
    )
    .unwrap();

    let outcome = fork_chat_session(
        &conn,
        &fork_params(WORKSPACE_MODE_COPY),
        &ChatForkRuntimePaths {
            source_jsonl: root.join("missing-source.jsonl"),
            target_jsonl: root.join("branch.jsonl"),
            fallback_cwd: root.clone(),
        },
    )
    .unwrap();
    assert_eq!(outcome.copied_turns, 2);
    // 源 JSONL 缺失也能 fork：只写 header 的空会话文件。
    let target_raw = fs::read_to_string(root.join("branch.jsonl")).unwrap();
    let header: Value =
        serde_json::from_str(target_raw.lines().next().unwrap()).unwrap();
    assert_eq!(header["cwd"].as_str(), Some(root.to_string_lossy().as_ref()));
    // 工作区是独立目录且文件被复制。
    let branch_dirs =
        storage::session_workspace::get_session_workspace_dirs(&conn, "branch-1")
            .unwrap()
            .unwrap();
    let branch_topic = branch_dirs.topic_workspace_dir.clone().unwrap();
    assert_ne!(branch_topic, src_state.topic_workspace_dir);
    assert_eq!(
        fs::read_to_string(Path::new(&branch_topic).join("artifact.md")).unwrap(),
        "产物"
    );
    // 源文件仍在（独立拷贝不迁移）。
    assert!(Path::new(&src_state.topic_workspace_dir)
        .join("artifact.md")
        .is_file());

    let _ = fs::remove_dir_all(root);
    std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
}

#[test]
fn fork_chat_session_rejects_bad_input() {
    let conn = open_in_memory().unwrap();
    make_session(&conn, "src");
    append_turn(&conn, "src", "t0", "第一问");
    let paths = ChatForkRuntimePaths {
        source_jsonl: PathBuf::from("/nonexistent/source.jsonl"),
        target_jsonl: PathBuf::from("/nonexistent/target.jsonl"),
        fallback_cwd: PathBuf::from("/tmp"),
    };
    // fork 点不存在。
    let mut params = fork_params(WORKSPACE_MODE_SHARE);
    params.fork_turn_id = "missing".to_string();
    assert!(fork_chat_session(&conn, &params, &paths).is_err());
    // 未知工作区模式。
    let mut params = fork_params("wild");
    params.fork_turn_id = "t0".to_string();
    assert!(fork_chat_session(&conn, &params, &paths).is_err());
    // 与源会话同 id。
    let mut params = fork_params(WORKSPACE_MODE_SHARE);
    params.fork_turn_id = "t0".to_string();
    params.new_session_id = "src".to_string();
    assert!(fork_chat_session(&conn, &params, &paths).is_err());
    // 目标 id 已存在。
    make_session(&conn, "branch-1");
    let mut params = fork_params(WORKSPACE_MODE_SHARE);
    params.fork_turn_id = "t0".to_string();
    assert!(fork_chat_session(&conn, &params, &paths).is_err());
}

#[test]
fn fork_chat_session_copies_multimodal_summaries_upto_cutoff() {
    let conn = open_in_memory().unwrap();
    make_session(&conn, "src");
    append_turn(&conn, "src", "t0", "看图");
    append_turn(&conn, "src", "t1", "追问");
    let rows = list_chat_turns(&conn, "src").unwrap();
    // t0 完成于 t1 之前；把 t1 的完成时间往后拨，制造 cutoff 差异。
    conn.execute(
        "UPDATE chat_turns SET completed_at = ?1 WHERE id = 't1'",
        rusqlite::params![rows[1].created_at + 10_000],
    )
    .unwrap();

    let source_key = crate::session_summary_key("src");
    storage::core_memory::insert_runtime_multimodal_summary(
        &conn,
        &source_key,
        "看图",
        "图里是封面",
    )
    .unwrap();
    // 让第二条摘要晚于 cutoff。
    conn.execute(
        "UPDATE runtime_multimodal_summaries SET created_at = ?1 WHERE user_prompt = '看图'",
        rusqlite::params![rows[0].created_at + 1],
    )
    .unwrap();
    storage::core_memory::insert_runtime_multimodal_summary(
        &conn,
        &source_key,
        "更晚的图片问答",
        "应被截断",
    )
    .unwrap();
    conn.execute(
        "UPDATE runtime_multimodal_summaries SET created_at = ?1 WHERE user_prompt = '更晚的图片问答'",
        rusqlite::params![rows[1].created_at + 20_000],
    )
    .unwrap();

    let root = temp_root("summary");
    let outcome = fork_chat_session(
        &conn,
        &fork_params(WORKSPACE_MODE_SHARE),
        &ChatForkRuntimePaths {
            source_jsonl: root.join("none.jsonl"),
            target_jsonl: root.join("branch.jsonl"),
            fallback_cwd: root.clone(),
        },
    )
    .unwrap();
    assert_eq!(outcome.copied_turns, 2);
    let branch_rows = storage::core_memory::list_runtime_multimodal_summaries(
        &conn,
        &crate::session_summary_key("branch-1"),
        10,
    )
    .unwrap();
    assert_eq!(branch_rows.len(), 1);
    assert_eq!(branch_rows[0].user_prompt, "看图");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn fork_chat_session_copies_truncated_event_log_for_agent_session() {
    let _guard = crate::workspace_env_test_lock();
    let root = temp_root("events");
    std::env::set_var("NINECLAW_WORKSPACE_ROOT", &root);
    let conn = open_in_memory().unwrap();
    // 带 agent 的会话。
    create_chat_session(
        &conn,
        &CreateChatSessionInput {
            id: "src".to_string(),
            title: "agent 会话".to_string(),
            status: "done".to_string(),
            agent_id: Some("agent-9".to_string()),
            agent_snapshot_json: None,
            bot_target_json: None,
            session_llm_provider_id: None,
            session_llm_model: None,
            workspace_id: None,
        },
    )
    .unwrap();
    append_turn(&conn, "src", "t0", "第一问");
    append_turn(&conn, "src", "t1", "第二问");
    append_turn(&conn, "src", "t2", "第三问");

    let agent_home = root.join("agents").join("agent-9");
    let log_path = crate::managed_runtime::session_log_path_for(&agent_home, "src");
    fs::create_dir_all(log_path.parent().unwrap()).unwrap();
    fs::write(
        &log_path,
        [
            r#"{"id":"e1","session_id":"src","kind":"mode_set","summary":"execution_mode: single","detail":{"mode":"single"},"created_at":1}"#,
            r#"{"id":"e2","session_id":"src","kind":"prompt","summary":"第一问","created_at":2}"#,
            r#"{"id":"e3","session_id":"src","kind":"mode_set","summary":"execution_mode: worker","detail":{"mode":"worker"},"created_at":3}"#,
            r#"{"id":"e4","session_id":"src","kind":"prompt","summary":"第二问","created_at":4}"#,
            r#"{"id":"e5","session_id":"src","kind":"mode_set","summary":"execution_mode: worker2","detail":{"mode":"worker2"},"created_at":5}"#,
            r#"{"id":"e6","session_id":"src","kind":"prompt","summary":"第三问","created_at":6}"#,
        ]
        .join("\n"),
    )
    .unwrap();

    let outcome = fork_chat_session(
        &conn,
        &fork_params(WORKSPACE_MODE_SHARE),
        &ChatForkRuntimePaths {
            source_jsonl: root.join("none.jsonl"),
            target_jsonl: root.join("branch.jsonl"),
            fallback_cwd: agent_home.clone(),
        },
    )
    .unwrap();
    assert_eq!(outcome.copied_event_lines, 4);

    let branch_log =
        crate::managed_runtime::session_log_path_for(&agent_home, "branch-1");
    let content = fs::read_to_string(&branch_log).unwrap();
    assert!(content.contains("\"session_id\":\"branch-1\""));
    assert!(!content.contains("第三问"));
    // fold 出的模式与 fork 点（第二轮）一致：worker。
    let events = crate::execution_mode_fold::load_session_events(&agent_home, "branch-1");
    assert_eq!(
        crate::execution_mode_fold::fold_execution_mode(&events, None).as_deref(),
        Some("worker")
    );

    let _ = fs::remove_dir_all(root);
    std::env::remove_var("NINECLAW_WORKSPACE_ROOT");
}
