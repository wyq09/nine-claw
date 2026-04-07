#![allow(dead_code)]

use super::{
    category_memory_dir, current_date_label, current_timestamp_file_label, read_workspace_file,
    sanitize_workspace_file_name, sanitize_workspace_segment, trim_to_char_limit,
    truncate_for_memory, AgentWorkspaceFile, MemoryCategoryDefinition,
};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const RAW_SOURCE_DIR: &str = "memory/raw";
const WIKI_INDEX_FILE: &str = "WIKI_INDEX.md";
const SOURCE_INDEX_FILE: &str = "SOURCE_INDEX.md";
const LOG_FILE: &str = "LOG.md";
const LINT_FILE: &str = "LINT.md";
const MEMORY_WIKI_FILES: &[&str] = &[WIKI_INDEX_FILE, SOURCE_INDEX_FILE, LOG_FILE, LINT_FILE];

pub(super) fn ensure_memory_wiki_scaffold(agent_home: &Path) -> Result<(), String> {
    let memory_dir = agent_home.join("memory");
    fs::create_dir_all(memory_dir.join("raw"))
        .map_err(|error| format!("创建 raw source 目录失败: {error}"))?;

    let wiki_index_path = memory_dir.join(WIKI_INDEX_FILE);
    let mut needs_wiki_rebuild = !wiki_index_path.exists();

    let lint_path = memory_dir.join(LINT_FILE);
    if !lint_path.exists() {
        fs::write(&lint_path, build_lint_template())
            .map_err(|error| format!("写入 LINT.md 失败: {error}"))?;
        needs_wiki_rebuild = true;
    }

    let source_index_path = memory_dir.join(SOURCE_INDEX_FILE);
    if !source_index_path.exists() {
        fs::write(&source_index_path, build_source_index_template())
            .map_err(|error| format!("写入 SOURCE_INDEX.md 失败: {error}"))?;
        needs_wiki_rebuild = true;
    }

    let log_path = memory_dir.join(LOG_FILE);
    if !log_path.exists() {
        fs::write(&log_path, build_log_template())
            .map_err(|error| format!("写入 LOG.md 失败: {error}"))?;
        needs_wiki_rebuild = true;
    }

    if needs_wiki_rebuild {
        rebuild_wiki_index(agent_home)?;
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
        "索引入口：先看 `memory/{}`；需要历史来源时再查 `memory/{}`。",
        WIKI_INDEX_FILE, SOURCE_INDEX_FILE
    ));

    let related_categories = super::select_memory_categories_for_query(current_prompt)
        .into_iter()
        .filter(|category| category.key != "general")
        .take(2)
        .map(|category| format!("`memory/categories/{}.md`", category.key))
        .collect::<Vec<_>>();
    if !related_categories.is_empty() {
        sections.push(format!(
            "本轮优先分类：{}。",
            related_categories.join("、")
        ));
    }

    if should_inline_source_index(current_prompt) {
        sections.push(
            "若问题涉及附件、来源或历史原文，按 `memory/SOURCE_INDEX.md` 里的路径打开对应 `memory/raw/...` 或 `inbox/...`。"
                .to_string(),
        );
    }

    if sections.is_empty() {
        Ok(None)
    } else {
        Ok(Some(sections.join("\n")))
    }
}

fn should_inline_source_index(current_prompt: Option<&str>) -> bool {
    let prompt = current_prompt.unwrap_or_default().trim().to_lowercase();
    if prompt.is_empty() {
        return false;
    }

    [
        "附件", "文件", "来源", "source", "path", "路径", "pdf", "doc", "docx", "xls", "xlsx",
        "txt", "图片", "视频", "语音", "上传",
    ]
    .iter()
    .any(|keyword| prompt.contains(keyword))
}

pub(super) fn read_memory_wiki_files(
    root: &Path,
    agent_id: &str,
) -> Result<Vec<AgentWorkspaceFile>, String> {
    let agent_home = root.join("agents").join(agent_id);
    ensure_memory_wiki_scaffold(&agent_home)?;

    Ok(MEMORY_WIKI_FILES
        .iter()
        .map(|file_name| {
            let relative_path = PathBuf::from("agents")
                .join(agent_id)
                .join("memory")
                .join(file_name);
            read_workspace_file(
                "agent",
                "memoryWiki",
                file_name,
                relative_path,
                agent_home.join("memory").join(file_name),
                false,
            )
        })
        .collect())
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

    append_log_entry(
        agent_home,
        timestamp,
        "ingest",
        summary,
        &[
            format!("- source: `{raw_relative_path}`"),
            format!(
                "- categories: {}",
                format_category_titles(categories).unwrap_or_else(|| "GENERAL_MEMORY".to_string())
            ),
        ],
    )?;

    rebuild_wiki_index(agent_home)?;
    Ok(raw_relative_path)
}

pub(super) fn refresh_memory_wiki(agent_home: &Path) -> Result<(), String> {
    ensure_memory_wiki_scaffold(agent_home)?;
    rebuild_wiki_index(agent_home)
}

pub(super) fn record_attachment_source(
    agent_home: &Path,
    timestamp: &str,
    title: &str,
    file_path: &Path,
    mime_type: Option<&str>,
    note: Option<&str>,
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

    let mut details = vec![format!("- source: `{relative_path}`")];
    if let Some(mime) = mime_type.and_then(trim_non_empty) {
        details.push(format!("- mime: `{mime}`"));
    }
    if let Some(note) = note.and_then(trim_non_empty) {
        details.push(format!("- note: {}", trim_to_char_limit(note, 220)));
    }

    append_log_entry(agent_home, timestamp, "source", &summary, &details)?;
    rebuild_wiki_index(agent_home)
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
    let existing = fs::read_to_string(&path).unwrap_or_else(|_| build_source_index_template());
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
        source_type,
        timestamp,
        cats_keys
    );
    if let Some(categories) = categories {
        if let Some(labels) = format_category_titles(categories) {
            let _ = writeln!(next, "  - Categories: {}", labels);
        }
    }
    next.push('\n');

    fs::write(&path, next).map_err(|error| format!("写入 SOURCE_INDEX.md 失败: {error}"))
}

fn append_log_entry(
    agent_home: &Path,
    timestamp: &str,
    operation: &str,
    title: &str,
    details: &[String],
) -> Result<(), String> {
    let path = agent_home.join("memory").join(LOG_FILE);
    let existing = fs::read_to_string(&path).unwrap_or_else(|_| build_log_template());
    let mut next = existing.trim_end().to_string();
    if !next.contains("## Entries") {
        next.push_str("\n\n## Entries\n");
    }

    let _ = writeln!(
        next,
        "\n## [{}] {} | {}",
        timestamp,
        operation,
        truncate_for_memory(title, 140)
    );
    for detail in details {
        let _ = writeln!(next, "{}", detail);
    }
    next.push('\n');

    fs::write(&path, next).map_err(|error| format!("写入 LOG.md 失败: {error}"))
}

fn rebuild_wiki_index(agent_home: &Path) -> Result<(), String> {
    let path = agent_home.join("memory").join(WIKI_INDEX_FILE);
    let content = build_wiki_index_content(agent_home)?;
    fs::write(&path, content).map_err(|error| format!("写入 WIKI_INDEX.md 失败: {error}"))
}

fn build_wiki_index_content(agent_home: &Path) -> Result<String, String> {
    let mut content = String::from(
        "# WIKI_INDEX.md\n\nThis file is the curated map of the current agent wiki. Read this first, then drill into the specific memory pages you need.\n\n## Layers\n\n- Raw sources: `memory/raw/` and uploaded artifacts under `inbox/` are immutable source-of-truth records.\n- Curated wiki: `MEMORY.md`, `WORKING.md`, category shards, decisions, and daily logs are the maintained synthesis layer.\n- Schema: `AGENTS.md` plus the agent's private markdown files define how to ingest, query, and lint this wiki.\n\n",
    );

    content.push_str("## Core Pages\n\n");
    for file_name in [
        "IDENTITY.md",
        "ROLE.md",
        "MEMORY.md",
        "WORKING.md",
        "DECISIONS.md",
        "PUBLIC_CONTEXT.md",
        "TOOLS.md",
    ] {
        let path = agent_home.join(file_name);
        if !path.exists() {
            continue;
        }
        let summary = summarize_markdown_file(&path)
            .unwrap_or_else(|_| "No summary available yet.".to_string());
        let _ = writeln!(content, "- `{}`: {}", file_name, summary);
    }

    content.push_str("\n## Topic map (headings)\n\n");
    let nav_files = [
        "MEMORY.md",
        "WORKING.md",
        "DECISIONS.md",
        "PUBLIC_CONTEXT.md",
    ];
    let mut any_topic = false;
    for file_name in nav_files {
        let path = agent_home.join(file_name);
        if !path.exists() {
            continue;
        }
        let titles = extract_h2_titles(&path, 12).unwrap_or_default();
        if titles.is_empty() {
            continue;
        }
        any_topic = true;
        let _ = writeln!(content, "### `{}`\n", file_name);
        for title in titles {
            let _ = writeln!(content, "- {}", title);
        }
        content.push('\n');
    }
    if !any_topic {
        content.push_str(
            "No `##` section titles found in core pages yet. Add headings to MEMORY.md / WORKING.md / DECISIONS.md so this map becomes useful.\n\n",
        );
    }

    content.push_str("## Category Shards\n\n");
    let category_dir = category_memory_dir(agent_home);
    if category_dir.exists() {
        let mut category_entries = fs::read_dir(&category_dir)
            .map_err(|error| format!("读取分类记忆目录失败: {error}"))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|item| item.to_str()) == Some("md"))
            .collect::<Vec<_>>();
        category_entries.sort();

        if category_entries.is_empty() {
            content.push_str("- No category memory yet.\n");
        } else {
            for path in category_entries {
                let file_name = path
                    .file_name()
                    .and_then(|item| item.to_str())
                    .unwrap_or("category.md");
                let entry_count = count_entries_in_markdown(&path).unwrap_or(0);
                let summary = summarize_markdown_file(&path)
                    .unwrap_or_else(|_| "No summary available yet.".to_string());
                let _ = writeln!(
                    content,
                    "- `{}`: {} ({} entries)",
                    file_name, summary, entry_count
                );
            }
        }
    } else {
        content.push_str("- No category memory yet.\n");
    }

    let daily_log_count = count_daily_logs(agent_home)?;
    let raw_source_count = count_raw_sources(agent_home)?;
    content.push_str("\n## Indices\n\n");
    let _ = writeln!(
        content,
        "- `memory/{}`: raw source catalog",
        SOURCE_INDEX_FILE
    );
    let _ = writeln!(
        content,
        "- `memory/{}`: append-only ingest/query/source log",
        LOG_FILE
    );
    let _ = writeln!(
        content,
        "- `memory/{}`: lint checklist and health policy",
        LINT_FILE
    );
    let _ = writeln!(content, "- Daily logs: {} files", daily_log_count);
    let _ = writeln!(content, "- Raw sources: {} files", raw_source_count);

    Ok(content)
}

fn extract_h2_titles(path: &Path, limit: usize) -> Result<Vec<String>, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("读取 markdown 文件失败 {}: {error}", path.display()))?;
    let mut titles = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("## ") else {
            continue;
        };
        if rest.starts_with('#') {
            continue;
        }
        let title = rest.trim();
        if title.is_empty() || title.eq_ignore_ascii_case("entries") {
            continue;
        }
        titles.push(title.to_string());
        if titles.len() >= limit {
            break;
        }
    }
    Ok(titles)
}

fn summarize_markdown_file(path: &Path) -> Result<String, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("读取 markdown 文件失败 {}: {error}", path.display()))?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("- **") {
            continue;
        }
        return Ok(trim_to_char_limit(trimmed, 110));
    }
    Ok("No summary available yet.".to_string())
}

fn count_entries_in_markdown(path: &Path) -> Result<usize, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("读取 markdown 文件失败 {}: {error}", path.display()))?;
    Ok(content
        .lines()
        .filter(|line| line.trim_start().starts_with("- "))
        .count())
}

fn count_daily_logs(agent_home: &Path) -> Result<usize, String> {
    let memory_dir = agent_home.join("memory");
    if !memory_dir.exists() {
        return Ok(0);
    }

    Ok(fs::read_dir(&memory_dir)
        .map_err(|error| format!("读取 memory 目录失败: {error}"))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|item| item.to_str()) == Some("md") && is_daily_log_file(path)
        })
        .count())
}

fn count_raw_sources(agent_home: &Path) -> Result<usize, String> {
    let raw_dir = agent_home.join(RAW_SOURCE_DIR);
    if !raw_dir.exists() {
        return Ok(0);
    }

    let mut count = 0usize;
    let mut stack = vec![raw_dir];
    while let Some(dir) = stack.pop() {
        for entry in
            fs::read_dir(&dir).map_err(|error| format!("读取 raw source 目录失败: {error}"))?
        {
            let entry = entry.map_err(|error| format!("读取 raw source 条目失败: {error}"))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|item| item.to_str()) == Some("md") {
                count += 1;
            }
        }
    }

    Ok(count)
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

fn build_source_index_template() -> String {
    "# SOURCE_INDEX.md\n\nThis file registers immutable raw sources and uploaded artifacts. The LLM should never rewrite the underlying source files; it should only update the curated wiki around them.\n\nEach entry includes an `Index:` line (`type=… ts=… cats=…`) for quick filtering (e.g. `rg \"Index: type=conversation\"`).\n\n## Entries\n".to_string()
}

fn build_log_template() -> String {
    "# LOG.md\n\nAppend-only operational log. Use headings like `## [timestamp] ingest | title` so simple grep/tail commands stay useful.\n\n## Entries\n".to_string()
}

fn build_lint_template() -> String {
    "# LINT.md\n\n## Health Checklist\n\n- Periodically merge important lines from `memory/YYYY-MM-DD.md` and `memory/raw/` into category shards, DECISIONS.md, or MEMORY.md (ingest no longer appends categories by default).\n- Check for contradictions between category shards and the main MEMORY.md.\n- Flag stale claims that newer sources or newer decisions supersede.\n- Look for orphan pages or concepts mentioned repeatedly without a dedicated page.\n- Add missing cross references when a new source changes multiple topics.\n- Suggest the next source or question when the wiki has a clear gap.\n\n## Last Pass\n\n- No lint pass recorded yet.\n".to_string()
}
