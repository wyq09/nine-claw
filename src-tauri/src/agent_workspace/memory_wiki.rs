#![allow(dead_code)]

use super::{
    current_date_label, current_timestamp_file_label, read_workspace_file,
    sanitize_workspace_file_name, sanitize_workspace_segment, truncate_for_memory,
    AgentWorkspaceFile, MemoryCategoryDefinition,
};
use rusqlite::Connection;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const RAW_SOURCE_DIR: &str = "memory/raw";
const SOURCE_INDEX_FILE: &str = "SOURCE_INDEX.md";
const DAILY_INDEX_FILE: &str = "DAILY_INDEX.md";
const WIKI_DIR: &str = "wiki";
const WIKI_INDEX_FILE: &str = "INDEX.md";
const MEMORY_WIKI_FILES: &[&str] = &[SOURCE_INDEX_FILE];

pub(super) fn ensure_memory_wiki_scaffold(agent_home: &Path) -> Result<(), String> {
    let memory_dir = agent_home.join("memory");
    fs::create_dir_all(memory_dir.join("raw"))
        .map_err(|error| format!("创建 raw source 目录失败: {error}"))?;
    fs::create_dir_all(agent_home.join(WIKI_DIR))
        .map_err(|error| format!("创建 wiki 目录失败: {error}"))?;

    for (relative_path, fallback) in [
        (
            PathBuf::from("memory").join(SOURCE_INDEX_FILE),
            super::fallback_template("memory/SOURCE_INDEX.md"),
        ),
        (
            PathBuf::from("memory").join(DAILY_INDEX_FILE),
            super::fallback_template("memory/DAILY_INDEX.md"),
        ),
        (
            PathBuf::from(WIKI_DIR).join(WIKI_INDEX_FILE),
            super::fallback_template("wiki/INDEX.md"),
        ),
    ] {
        let path = agent_home.join(&relative_path);
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("创建记忆脚手架目录失败 {}: {error}", parent.display()))?;
        }
        fs::write(&path, fallback)
            .map_err(|error| format!("写入记忆脚手架文件 {} 失败: {error}", path.display()))?;
    }

    Ok(())
}

pub(super) fn build_memory_wiki_snapshot(
    agent_home: &Path,
    current_prompt: Option<&str>,
) -> Result<Option<String>, String> {
    ensure_memory_wiki_scaffold(agent_home)?;

    let mut sections = Vec::new();
    sections.push(format!(
        "查来源看 `memory/{}`；**日记检索**看 `memory/{}`（`DAILY|` 行含 date/cats/summary，再下钻 `memory/YYYY-MM-DD.md`）；查外部知识看 `wiki/{}`。",
        SOURCE_INDEX_FILE, DAILY_INDEX_FILE, WIKI_INDEX_FILE
    ));

    let prompt = current_prompt.unwrap_or_default().trim().to_lowercase();
    if contains_any(
        &prompt,
        &[
            "待办",
            "承诺",
            "next",
            "follow up",
            "blocker",
            "提醒",
            "继续",
            "follow-up",
        ],
    ) {
        sections.push("涉及承诺或阻塞时，查看 `WORKING.md` 的 OPEN_LOOPS 与复查项。".to_string());
    }
    if contains_any(
        &prompt,
        &[
            "文章",
            "github",
            "repo",
            "文档",
            "research",
            "方法",
            "技术方案",
        ],
    ) {
        sections.push("涉及外部知识或方法论时，优先查 `wiki/INDEX.md`。".to_string());
    }
    if should_inline_source_index(current_prompt) {
        sections.push(
            "若问题涉及附件、来源或历史原文，按 `memory/SOURCE_INDEX.md` 里的路径打开对应 `memory/raw/...` 或 `inbox/...`。"
                .to_string(),
        );
    }
    if contains_any(
        &prompt,
        &[
            "偏好",
            "风格",
            "喜欢",
            "讨厌",
            "习惯",
            "称呼",
            "style",
            "preference",
        ],
    ) {
        sections.push("涉及用户风格或隐含意图时，优先查 `USER_MODEL.md`。".to_string());
    }
    if contains_any(
        &prompt,
        &[
            "坑",
            "翻车",
            "别再",
            "时间线",
            "误判",
            "pitfall",
            "mistake",
            "error",
        ],
    ) {
        sections.push("涉及容易犯错或纠正规则时，优先查 `PITFALLS.md`。".to_string());
    }
    if contains_any(
        &prompt,
        &[
            "谁",
            "关系",
            "联系人",
            "团队",
            "老板",
            "客户",
            "合作方",
            "owner",
            "contact",
        ],
    ) {
        sections.push("涉及人物身份或关系时，优先查 `RELATIONSHIP_MAP.md`。".to_string());
    }

    // Append vector semantic hints (optional — graceful no-op on any failure)
    if let Some(hints) = append_vector_semantic_hints(agent_home, &prompt) {
        sections.push(hints);
    }

    Ok(Some(sections.join("\n")))
}

fn should_inline_source_index(current_prompt: Option<&str>) -> bool {
    let prompt = current_prompt.unwrap_or_default().trim().to_lowercase();
    if prompt.is_empty() {
        return false;
    }

    contains_any(
        &prompt,
        &[
            "附件", "文件", "来源", "source", "path", "路径", "pdf", "doc", "docx", "xls", "xlsx",
            "txt", "图片", "视频", "语音", "上传",
        ],
    )
}

pub(super) fn read_memory_wiki_files(
    root: &Path,
    agent_id: &str,
) -> Result<Vec<AgentWorkspaceFile>, String> {
    let agent_home = root.join("agents").join(agent_id);
    ensure_memory_wiki_scaffold(&agent_home)?;

    let mut files: Vec<AgentWorkspaceFile> = MEMORY_WIKI_FILES
        .iter()
        .map(|file_name| {
            let relative_path = PathBuf::from("agents")
                .join(agent_id)
                .join("memory")
                .join(file_name);
            read_workspace_file(
                "agent",
                "memoryIndex",
                file_name,
                relative_path,
                agent_home.join("memory").join(file_name),
                false,
                false,
            )
        })
        .collect();

    let daily_index_path = agent_home.join("memory").join(DAILY_INDEX_FILE);
    let daily_rel = PathBuf::from("agents")
        .join(agent_id)
        .join("memory")
        .join(DAILY_INDEX_FILE);
    files.push(read_workspace_file(
        "agent",
        "memoryIndex",
        DAILY_INDEX_FILE,
        daily_rel,
        daily_index_path,
        false,
        true,
    ));

    Ok(files)
}

/// 每条 ingest 对应一行，供 `rg`/运行时按 `cats` 与摘要过滤，避免通读所有 `YYYY-MM-DD.md`。
pub(super) fn append_daily_digest_index_line(
    agent_home: &Path,
    date_label: &str,
    timestamp: &str,
    user_id: &str,
    summary: &str,
    categories: &[MemoryCategoryDefinition],
) -> Result<(), String> {
    ensure_memory_wiki_scaffold(agent_home)?;
    let path = agent_home.join("memory").join(DAILY_INDEX_FILE);
    let existing = fs::read_to_string(&path)
        .unwrap_or_else(|_| super::fallback_template("memory/DAILY_INDEX.md"));
    let mut next = existing.trim_end().to_string();
    if !next.contains("## Lines") {
        next.push_str("\n\n## Lines\n");
    }
    let cats = if categories.is_empty() {
        "general".to_string()
    } else {
        format_category_keys(categories)
    };
    let safe_user = sanitize_workspace_segment(user_id, "user");
    let one_line = sanitize_daily_index_text(summary, 220);
    let _ = writeln!(
        next,
        "DAILY|{}|{}|{}|{}|{}",
        sanitize_daily_index_text(date_label, 12),
        sanitize_daily_index_text(timestamp, 40),
        safe_user,
        sanitize_daily_index_text(&cats, 120),
        one_line
    );
    fs::write(&path, next).map_err(|error| format!("写入 DAILY_INDEX.md 失败: {error}"))
}

fn sanitize_daily_index_text(value: &str, max_chars: usize) -> String {
    let collapsed: String = value
        .chars()
        .map(|ch| {
            if ch == '|' || ch.is_control() {
                ' '
            } else {
                ch
            }
        })
        .collect();
    truncate_for_memory(collapsed.trim(), max_chars)
}

pub(super) fn record_conversation_ingest(
    agent_home: &Path,
    user_id: &str,
    timestamp: &str,
    user_message: &str,
    assistant_message: &str,
    summary: &str,
    categories: &[MemoryCategoryDefinition],
) -> Result<String, String> {
    ensure_memory_wiki_scaffold(agent_home)?;

    let raw_source_path = write_raw_conversation_source(
        agent_home,
        user_id,
        timestamp,
        user_message,
        assistant_message,
        summary,
        categories,
    )?;
    let raw_relative_path = relative_to_agent_home(agent_home, &raw_source_path);

    append_source_index_entry(
        agent_home,
        timestamp,
        "conversation",
        &format!("desktop-or-im/{user_id}"),
        &raw_relative_path,
        summary,
        Some(categories),
    )?;

    Ok(raw_relative_path)
}

pub(super) fn record_attachment_source(
    agent_home: &Path,
    timestamp: &str,
    title: &str,
    file_path: &Path,
    mime_type: Option<&str>,
    _note: Option<&str>,
) -> Result<(), String> {
    ensure_memory_wiki_scaffold(agent_home)?;

    let relative_path = relative_to_agent_home(agent_home, file_path);
    let summary = match mime_type.and_then(trim_non_empty) {
        Some(mime) => format!("附件 `{title}` 已导入工作区，mime={mime}"),
        None => format!("附件 `{title}` 已导入工作区"),
    };

    append_source_index_entry(
        agent_home,
        timestamp,
        "attachment",
        title,
        &relative_path,
        &summary,
        None,
    )?;

    Ok(())
}

fn write_raw_conversation_source(
    agent_home: &Path,
    user_id: &str,
    timestamp: &str,
    user_message: &str,
    assistant_message: &str,
    summary: &str,
    categories: &[MemoryCategoryDefinition],
) -> Result<PathBuf, String> {
    let raw_dir = agent_home.join(RAW_SOURCE_DIR).join(current_date_label());
    fs::create_dir_all(&raw_dir)
        .map_err(|error| format!("创建 raw source 日期目录失败: {error}"))?;

    let file_name = format!(
        "{}-{}.md",
        current_timestamp_file_label(),
        sanitize_workspace_segment(user_id, "user")
    );
    let path = raw_dir.join(sanitize_workspace_file_name(&file_name, "conversation.md"));

    let mut content = String::new();
    let _ = writeln!(content, "# Raw Conversation Source");
    let _ = writeln!(content);
    let _ = writeln!(content, "- Timestamp: {}", timestamp);
    let _ = writeln!(content, "- User: `{}`", user_id);
    let _ = writeln!(content, "- Summary: {}", summary);
    let _ = writeln!(
        content,
        "- Categories: {}",
        format_category_titles(categories).unwrap_or_else(|| "GENERAL_MEMORY".to_string())
    );
    let _ = writeln!(content);
    let _ = writeln!(content, "## User Message");
    let _ = writeln!(content);
    let _ = writeln!(content, "{}", user_message.trim());
    let _ = writeln!(content);
    let _ = writeln!(content, "## Assistant Message");
    let _ = writeln!(content);
    let _ = writeln!(content, "{}", assistant_message.trim());
    let _ = writeln!(content);

    fs::write(&path, content)
        .map_err(|error| format!("写入 raw conversation source 失败: {error}"))?;
    Ok(path)
}

fn append_source_index_entry(
    agent_home: &Path,
    timestamp: &str,
    source_type: &str,
    title: &str,
    relative_path: &str,
    summary: &str,
    categories: Option<&[MemoryCategoryDefinition]>,
) -> Result<(), String> {
    let path = agent_home.join("memory").join(SOURCE_INDEX_FILE);
    let existing = fs::read_to_string(&path)
        .unwrap_or_else(|_| super::fallback_template("memory/SOURCE_INDEX.md"));
    let mut next = existing.trim_end().to_string();
    if !next.contains("## Entries") {
        next.push_str("\n\n## Entries\n");
    }

    let _ = writeln!(
        next,
        "\n- {} | `{}` | {}",
        timestamp,
        source_type,
        truncate_for_memory(title, 120)
    );
    let _ = writeln!(next, "  - Path: `{}`", relative_path);
    let _ = writeln!(next, "  - Summary: {}", truncate_for_memory(summary, 220));
    let cats_keys = categories.map(format_category_keys).unwrap_or_default();
    let _ = writeln!(
        next,
        "  - Index: type={} ts=`{}` cats=`{}`",
        source_type, timestamp, cats_keys
    );
    if let Some(categories) = categories {
        if let Some(labels) = format_category_titles(categories) {
            let _ = writeln!(next, "  - Categories: {}", labels);
        }
    }
    next.push('\n');

    fs::write(&path, next).map_err(|error| format!("写入 SOURCE_INDEX.md 失败: {error}"))
}

pub(super) fn is_daily_log_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|item| item.to_str()) else {
        return false;
    };

    let Some(stem) = file_name.strip_suffix(".md") else {
        return false;
    };

    if stem.len() != 10 {
        return false;
    }

    stem.chars().enumerate().all(|(index, ch)| match index {
        4 | 7 => ch == '-',
        _ => ch.is_ascii_digit(),
    })
}

fn relative_to_agent_home(agent_home: &Path, path: &Path) -> String {
    path.strip_prefix(agent_home)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn format_category_titles(categories: &[MemoryCategoryDefinition]) -> Option<String> {
    let titles = categories
        .iter()
        .map(|category| category.title)
        .collect::<Vec<_>>();
    if titles.is_empty() {
        None
    } else {
        Some(titles.join("、"))
    }
}

fn format_category_keys(categories: &[MemoryCategoryDefinition]) -> String {
    categories
        .iter()
        .map(|category| category.key)
        .collect::<Vec<_>>()
        .join(",")
}

fn trim_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn contains_any(content: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| content.contains(needle))
}

fn contains_all(content: &str, needles: &[&str]) -> bool {
    needles.iter().all(|needle| content.contains(needle))
}

/// Extract agent_id from the agent_home path (`…/agents/<agent_id>`).
fn agent_id_from_home(agent_home: &Path) -> Option<String> {
    agent_home
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Look up workspace_id(s) that the given agent belongs to.
fn find_workspace_ids_for_agent(conn: &Connection, agent_id: &str) -> Vec<String> {
    let mut stmt = match conn
        .prepare("SELECT DISTINCT workspace_id FROM workspace_members WHERE agent_id = ?1")
    {
        Ok(s) => s,
        Err(e) => {
            log::warn!("vector_memory_hints: prepare workspace lookup failed: {e}");
            return Vec::new();
        }
    };
    let mut rows = match stmt.query(rusqlite::params![agent_id]) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("vector_memory_hints: query workspace lookup failed: {e}");
            return Vec::new();
        }
    };
    let mut ids = Vec::new();
    while let Ok(Some(row)) = rows.next() {
        if let Ok(id) = row.get::<_, String>(0) {
            ids.push(id);
        }
    }
    ids
}

/// Generate semantic search hints using vector similarity.
///
/// Returns `None` if anything fails (no registry, no provider, no hits, etc.).
fn vector_memory_hints(
    conn: &Connection,
    workspace_id: &str,
    agent_id: Option<&str>,
    prompt: &str,
) -> Option<String> {
    let registry = crate::managed_runtime::get_embedding_registry()?;

    let embeddings = match tokio::runtime::Handle::try_current() {
        Ok(handle) => tokio::task::block_in_place(|| {
            handle.block_on(async {
                let guard = registry.read().await;
                if let Some(provider) = guard.default_provider() {
                    provider.embed(vec![prompt.to_string()]).await
                } else {
                    Err("no default embedding provider".to_string())
                }
            })
        }),
        Err(_) => {
            // No Tokio runtime available (e.g. called from spawn_blocking)
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()?;
            rt.block_on(async {
                let guard = registry.read().await;
                if let Some(provider) = guard.default_provider() {
                    provider.embed(vec![prompt.to_string()]).await
                } else {
                    Err("no default embedding provider".to_string())
                }
            })
        }
    }
    .ok()?;

    let query_vec = &embeddings.get(0)?;
    let hits = match crate::memory_vector::three_layer_search(
        conn,
        workspace_id,
        agent_id,
        false, // not supervisor — this is an individual agent
        query_vec,
        3,
        0.6,
    ) {
        Ok(h) => h,
        Err(e) => {
            log::warn!("vector_memory_hints: three_layer_search failed: {e}");
            return None;
        }
    };

    if hits.is_empty() {
        return None;
    }

    let mut hints = String::from("[语义相关记忆]");
    for hit in hits {
        match crate::storage::workspaces::get_workspace_memory(conn, &hit.memory_id) {
            Ok(Some(mem)) => {
                let preview: String = mem.content.chars().take(80).collect();
                let _ = writeln!(
                    hints,
                    "- {} (相似度: {:.0}%): {}",
                    mem.title,
                    hit.score * 100.0,
                    preview
                );
            }
            Ok(None) => {
                log::warn!(
                    "vector_memory_hints: memory_id {} not found, skipping",
                    hit.memory_id
                );
            }
            Err(e) => {
                log::warn!(
                    "vector_memory_hints: get memory failed for {}: {e}",
                    hit.memory_id
                );
            }
        }
    }
    Some(hints)
}

/// Append vector semantic hints for the given agent_home and prompt.
/// Completely optional — returns `None` on any failure, existing keyword matching still works.
fn append_vector_semantic_hints(agent_home: &Path, prompt: &str) -> Option<String> {
    if prompt.is_empty() {
        return None;
    }

    let agent_id = agent_id_from_home(agent_home)?;

    // Obtain a DB connection via the global APP_HANDLE
    let app_handle = crate::managed_runtime::injected_app_handle()?;
    let conn = match crate::storage_conn(&app_handle) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("vector_memory_hints: storage_conn failed: {e}");
            return None;
        }
    };

    // Find workspace(s) this agent belongs to
    let workspace_ids = find_workspace_ids_for_agent(&conn, &agent_id);
    if workspace_ids.is_empty() {
        return None;
    }

    // Try each workspace until we get hints
    for ws_id in &workspace_ids {
        if let Some(hints) = vector_memory_hints(&conn, ws_id, Some(&agent_id), prompt) {
            return Some(hints);
        }
    }
    None
}
