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
const MEMORY_INDEX_FILE: &str = "INDEX.md";
const LEGACY_MEMORY_INDEX_FILE: &str = "WIKI_INDEX.md";
const SOURCE_INDEX_FILE: &str = "SOURCE_INDEX.md";
const LOG_FILE: &str = "LOG.md";
const LINT_FILE: &str = "LINT.md";
const REVIEW_QUEUE_FILE: &str = "REVIEW_QUEUE.md";
const WIKI_DIR: &str = "wiki";
const WIKI_INDEX_FILE: &str = "INDEX.md";
const MEMORY_WIKI_FILES: &[&str] = &[
    MEMORY_INDEX_FILE,
    SOURCE_INDEX_FILE,
    LOG_FILE,
    LINT_FILE,
    REVIEW_QUEUE_FILE,
];

pub(super) fn ensure_memory_wiki_scaffold(agent_home: &Path) -> Result<(), String> {
    let memory_dir = agent_home.join("memory");
    fs::create_dir_all(memory_dir.join("raw"))
        .map_err(|error| format!("创建 raw source 目录失败: {error}"))?;
    fs::create_dir_all(agent_home.join(WIKI_DIR))
        .map_err(|error| format!("创建 wiki 目录失败: {error}"))?;

    cleanup_legacy_memory_index(agent_home)?;

    for (relative_path, fallback) in [
        (
            PathBuf::from("memory").join(MEMORY_INDEX_FILE),
            super::fallback_template("memory/INDEX.md"),
        ),
        (
            PathBuf::from("memory").join(LINT_FILE),
            super::fallback_template("memory/LINT.md"),
        ),
        (
            PathBuf::from("memory").join(SOURCE_INDEX_FILE),
            super::fallback_template("memory/SOURCE_INDEX.md"),
        ),
        (
            PathBuf::from("memory").join(LOG_FILE),
            super::fallback_template("memory/LOG.md"),
        ),
        (
            PathBuf::from("memory").join(REVIEW_QUEUE_FILE),
            super::fallback_template("memory/REVIEW_QUEUE.md"),
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
        "入口：先看 `memory/{}`；查来源看 `memory/{}`；查外部知识看 `wiki/{}`。",
        MEMORY_INDEX_FILE, SOURCE_INDEX_FILE, WIKI_INDEX_FILE
    ));

    let related_categories = super::select_memory_categories_for_query(current_prompt)
        .into_iter()
        .filter(|category| category.key != "general")
        .take(3)
        .map(|category| format!("`memory/categories/{}.md`", category.key))
        .collect::<Vec<_>>();
    if !related_categories.is_empty() {
        sections.push(format!("本轮优先分类：{}。", related_categories.join("、")));
    }

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
        sections.push(
            "涉及承诺或阻塞时，同时查看 `memory/categories/commitments.md` 与 `memory/REVIEW_QUEUE.md`。"
                .to_string(),
        );
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
        sections.push("涉及用户风格或隐含意图时，优先查 `USER_MODEL.md` 与 `memory/categories/preferences.md`。".to_string());
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
        sections.push(
            "涉及容易犯错或纠正规则时，优先查 `PITFALLS.md` 与 `memory/categories/pitfalls.md`。"
                .to_string(),
        );
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
        sections.push("涉及人物身份或关系时，优先查 `RELATIONSHIP_MAP.md` 与 `memory/categories/relationships.md`。".to_string());
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

    Ok(MEMORY_WIKI_FILES
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

    rebuild_memory_index(agent_home)?;
    rewrite_lint_report(agent_home)?;
    Ok(raw_relative_path)
}

pub(super) fn refresh_memory_wiki(agent_home: &Path) -> Result<(), String> {
    ensure_memory_wiki_scaffold(agent_home)?;
    rebuild_memory_index(agent_home)?;
    rewrite_lint_report(agent_home)
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
    rebuild_memory_index(agent_home)?;
    rewrite_lint_report(agent_home)
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

fn append_log_entry(
    agent_home: &Path,
    timestamp: &str,
    operation: &str,
    title: &str,
    details: &[String],
) -> Result<(), String> {
    let path = agent_home.join("memory").join(LOG_FILE);
    let existing =
        fs::read_to_string(&path).unwrap_or_else(|_| super::fallback_template("memory/LOG.md"));
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

fn rebuild_memory_index(agent_home: &Path) -> Result<(), String> {
    let path = agent_home.join("memory").join(MEMORY_INDEX_FILE);
    let content = build_memory_index_content(agent_home)?;
    fs::write(&path, content).map_err(|error| format!("写入 memory/INDEX.md 失败: {error}"))
}

fn rewrite_lint_report(agent_home: &Path) -> Result<(), String> {
    let path = agent_home.join("memory").join(LINT_FILE);
    let content = build_lint_content(agent_home)?;
    fs::write(&path, content).map_err(|error| format!("写入 LINT.md 失败: {error}"))
}

fn build_memory_index_content(agent_home: &Path) -> Result<String, String> {
    let daily_log_count = count_daily_logs(agent_home)?;
    let raw_source_count = count_raw_sources(agent_home)?;
    let wiki_page_count = count_wiki_pages(agent_home)?;
    let category_count = count_category_files(agent_home)?;

    let mut content = String::from(
        "# INDEX.md\n\nThis is the root entry for the agent memory system. Use it to choose the right layer before reading or writing.\n\n## Read Order\n\n1. `MEMORY.md` for startup identity, core principles, and the most stable anchors.\n2. `WORKING.md` for current execution state, `Current Focus`, and `OPEN_LOOPS`.\n3. `DECISIONS.md` and `PITFALLS.md` for confirmed rules and high-risk mistakes.\n4. `USER_MODEL.md` and `RELATIONSHIP_MAP.md` for long-term human model and important people.\n5. `memory/categories/*.md` for curated long-term memory.\n6. `memory/REVIEW_QUEUE.md` for facts that need re-checking.\n7. `memory/YYYY-MM-DD.md` for daily summaries.\n8. `memory/SOURCE_INDEX.md` -> `memory/raw/...` or `inbox/...` for immutable evidence.\n9. `wiki/INDEX.md` for external knowledge, research notes, and methodology.\n\n## Fast Routes\n\n- 了解身份与稳定原则 -> `MEMORY.md`\n- 看当前执行态与未完成承诺 -> `WORKING.md`\n- 查已确认规则 -> `DECISIONS.md`\n- 查高风险坑点 -> `PITFALLS.md`\n- 看用户长期模型 -> `USER_MODEL.md`\n- 查重要人物关系 -> `RELATIONSHIP_MAP.md`\n- 看用户画像 -> `memory/categories/user_profile.md`\n- 看用户偏好 -> `memory/categories/preferences.md`\n- 看项目上下文 -> `memory/categories/projects.md`\n- 查承诺与待办 -> `memory/categories/commitments.md`\n- 查人物关系细节 -> `memory/categories/relationships.md`\n- 查坑点归档 -> `memory/categories/pitfalls.md`\n- 查推断但不要当成事实 -> `memory/categories/inferences.md`\n- 查复查队列 -> `memory/REVIEW_QUEUE.md`\n- 查原始来源 -> `memory/SOURCE_INDEX.md`\n- 查外部知识 -> `wiki/INDEX.md`\n\n## Query Route\n\n- 先判断是否真的需要查记忆。\n- 任务推进先看 `WORKING.md` 的 `Current Focus` / `OPEN_LOOPS`。\n- 规则与避免翻车先看 `DECISIONS.md` / `PITFALLS.md`。\n- 用户风格与隐含意图先看 `MEMORY.md` / `USER_MODEL.md`。\n- 人物身份和上下文先看 `RELATIONSHIP_MAP.md` / `memory/categories/relationships.md`。\n- 如果没有证据，就直接承认不记得。\n\n## Boundaries\n\n- Primary memory: `MEMORY.md`, `DECISIONS.md`, `PITFALLS.md`, `USER_MODEL.md`, `RELATIONSHIP_MAP.md`, `memory/categories/*.md`\n- Execution memory: `WORKING.md`, `memory/REVIEW_QUEUE.md`\n- Daily digest: `memory/YYYY-MM-DD.md`\n- Evidence layer: `memory/raw/...`, `memory/SOURCE_INDEX.md`, `inbox/...`\n- Knowledge wiki: `wiki/...`\n- Rule: raw is evidence, daily is digest, categories are curated memory, wiki is external knowledge.\n\n",
    );

    content.push_str("## Core Pages\n\n");
    for file_name in [
        "MEMORY.md",
        "WORKING.md",
        "DECISIONS.md",
        "PITFALLS.md",
        "USER_MODEL.md",
        "RELATIONSHIP_MAP.md",
        "PUBLIC_CONTEXT.md",
    ] {
        let path = agent_home.join(file_name);
        if !path.exists() {
            continue;
        }
        let summary = summarize_markdown_file(&path)
            .unwrap_or_else(|_| "No summary available yet.".to_string());
        let _ = writeln!(content, "- `{}`: {}", file_name, summary);
    }

    content.push_str("\n## Coverage\n\n");
    let _ = writeln!(content, "- Category shards: {} files", category_count);
    let _ = writeln!(content, "- Daily logs: {} files", daily_log_count);
    let _ = writeln!(content, "- Raw sources: {} files", raw_source_count);
    let _ = writeln!(content, "- Wiki pages: {} files", wiki_page_count);

    Ok(content)
}

#[derive(Clone)]
struct LintCheck {
    label: &'static str,
    points: u32,
    passed: bool,
    detail: String,
}

fn build_lint_content(agent_home: &Path) -> Result<String, String> {
    let checks = lint_checks(agent_home)?;
    let total = checks
        .iter()
        .filter(|check| check.passed)
        .map(|check| check.points)
        .sum::<u32>();
    let max = checks.iter().map(|check| check.points).sum::<u32>();
    let status = if total >= 90 {
        "healthy"
    } else if total >= 75 {
        "watch"
    } else {
        "needs_attention"
    };

    let mut content = String::from(
        "# LINT.md\n\n## Health Checklist\n\n- Keep `memory/INDEX.md` aligned with the actual directory layout.\n- Keep commitments structured and reviewable.\n- Keep inferences separate from confirmed facts.\n- Keep raw evidence, daily summaries, curated memory, and wiki content in their own layers.\n- Remove obsolete bootstrap state and stale generated entrypoints.\n\n## Last Pass\n\n",
    );

    let _ = writeln!(content, "### {}", current_date_label());
    for check in &checks {
        let verdict = if check.passed { "PASS" } else { "WARN" };
        let _ = writeln!(
            content,
            "- {} [{}/{}] {}: {}",
            verdict,
            if check.passed { check.points } else { 0 },
            check.points,
            check.label,
            check.detail
        );
    }

    let _ = writeln!(content, "\n## Scorecard\n");
    let _ = writeln!(content, "- Total: {}/{}", total, max);
    let _ = writeln!(content, "- Status: {}", status);

    Ok(content)
}

fn lint_checks(agent_home: &Path) -> Result<Vec<LintCheck>, String> {
    let memory_index =
        fs::read_to_string(agent_home.join("memory").join(MEMORY_INDEX_FILE)).unwrap_or_default();
    let commitments = fs::read_to_string(
        agent_home
            .join("memory")
            .join("categories")
            .join("commitments.md"),
    )
    .unwrap_or_default();
    let inferences = fs::read_to_string(
        agent_home
            .join("memory")
            .join("categories")
            .join("inferences.md"),
    )
    .unwrap_or_default();
    let working = fs::read_to_string(agent_home.join("WORKING.md")).unwrap_or_default();
    let review_queue =
        fs::read_to_string(agent_home.join("memory").join(REVIEW_QUEUE_FILE)).unwrap_or_default();
    let wiki_index =
        fs::read_to_string(agent_home.join(WIKI_DIR).join(WIKI_INDEX_FILE)).unwrap_or_default();
    let user_model = fs::read_to_string(agent_home.join("USER_MODEL.md")).unwrap_or_default();
    let pitfalls = fs::read_to_string(agent_home.join("PITFALLS.md")).unwrap_or_default();
    let relationship_map =
        fs::read_to_string(agent_home.join("RELATIONSHIP_MAP.md")).unwrap_or_default();
    let categories_dir = category_memory_dir(agent_home);

    let category_schema_ok = category_dir_schema_ok(&categories_dir)?;
    let bootstrap_missing = !agent_home.join("BOOTSTRAP.md").exists();
    let legacy_index_missing = !agent_home
        .join("memory")
        .join(LEGACY_MEMORY_INDEX_FILE)
        .exists();

    Ok(vec![
        LintCheck {
            label: "Structure",
            points: 10,
            passed: [
                agent_home.join("memory").join(MEMORY_INDEX_FILE).exists(),
                agent_home.join("memory").join(SOURCE_INDEX_FILE).exists(),
                agent_home.join("memory").join(LOG_FILE).exists(),
                agent_home.join("memory").join(REVIEW_QUEUE_FILE).exists(),
                agent_home.join(WIKI_DIR).join(WIKI_INDEX_FILE).exists(),
                agent_home.join("USER_MODEL.md").exists(),
                agent_home.join("PITFALLS.md").exists(),
                agent_home.join("RELATIONSHIP_MAP.md").exists(),
            ]
            .into_iter()
            .all(|flag| flag),
            detail: "核心入口、来源索引、日志、复查队列、用户模型、坑点清单和关系图都已就位。"
                .to_string(),
        },
        LintCheck {
            label: "Boundaries",
            points: 10,
            passed: contains_all(
                &memory_index,
                &[
                    "memory/raw/...",
                    "memory/YYYY-MM-DD.md",
                    "memory/categories/*.md",
                    "wiki/INDEX.md",
                ],
            ),
            detail: "memory/INDEX.md 明确了 raw / daily / categories / wiki 的边界。".to_string(),
        },
        LintCheck {
            label: "Category Schema",
            points: 10,
            passed: category_schema_ok,
            detail: "所有分类文件都包含 Schema 和 Entries 区块。".to_string(),
        },
        LintCheck {
            label: "Commitment Ledger",
            points: 10,
            passed: contains_all(
                &commitments,
                &["status:", "owner:", "created:", "next_check:", "source:"],
            ),
            detail: "commitments.md 已具备账本字段。".to_string(),
        },
        LintCheck {
            label: "Inference Separation",
            points: 10,
            passed: contains_all(&inferences, &["tentative", "confidence:", "review_at:"]),
            detail: "inferences.md 明确标识了 tentative / confidence / review_at。".to_string(),
        },
        LintCheck {
            label: "Review Lifecycle",
            points: 10,
            passed: contains_all(
                &review_queue,
                &["status:", "review_at:", "reason:", "source:"],
            ),
            detail: "REVIEW_QUEUE.md 具备复查闭环字段。".to_string(),
        },
        LintCheck {
            label: "Working Context",
            points: 10,
            passed: contains_all(
                &working,
                &["## Current Focus", "## OPEN_LOOPS", "## IM Latest Context"],
            ),
            detail: "WORKING.md 同时保留当前 focus、open loops 和最新对话上下文。".to_string(),
        },
        LintCheck {
            label: "Evidence Index",
            points: 10,
            passed: agent_home.join("memory").join(SOURCE_INDEX_FILE).exists()
                && agent_home.join("memory").join(LOG_FILE).exists(),
            detail: "来源索引和操作日志保持可追溯。".to_string(),
        },
        LintCheck {
            label: "Wiki Boundary",
            points: 10,
            passed: contains_all(&wiki_index, &["external knowledge", "memory", "wiki"]),
            detail: "wiki/INDEX.md 明确区分外部知识与人格记忆。".to_string(),
        },
        LintCheck {
            label: "Legacy Cleanup",
            points: 10,
            passed: bootstrap_missing
                && legacy_index_missing
                && contains_all(&user_model, &["## Interaction Style"])
                && contains_all(&pitfalls, &["## Active Pitfalls"])
                && contains_all(&relationship_map, &["## Key People"]),
            detail:
                "旧协议入口已清理，且 USER_MODEL / PITFALLS / RELATIONSHIP_MAP 已形成专项记忆入口。"
                    .to_string(),
        },
    ])
}

fn category_dir_schema_ok(category_dir: &Path) -> Result<bool, String> {
    if !category_dir.exists() {
        return Ok(false);
    }

    for entry in fs::read_dir(category_dir).map_err(|error| format!("读取分类目录失败: {error}"))?
    {
        let entry = entry.map_err(|error| format!("读取分类条目失败: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|item| item.to_str()) != Some("md") {
            continue;
        }
        if path.file_name().and_then(|item| item.to_str()) == Some("INDEX.md") {
            continue;
        }
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("读取分类文件失败 {}: {error}", path.display()))?;
        if !contains_all(&content, &["## Schema", "## Entries"]) {
            return Ok(false);
        }
    }

    Ok(true)
}

fn cleanup_legacy_memory_index(agent_home: &Path) -> Result<(), String> {
    let legacy_path = agent_home.join("memory").join(LEGACY_MEMORY_INDEX_FILE);
    if !legacy_path.exists() {
        return Ok(());
    }

    fs::remove_file(&legacy_path).map_err(|error| format!("删除旧 WIKI_INDEX.md 失败: {error}"))
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
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("- **")
            || trimmed.starts_with("```")
        {
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
        .filter(|line| line.trim_start().starts_with("- item:"))
        .count())
}

fn count_category_files(agent_home: &Path) -> Result<usize, String> {
    let category_dir = category_memory_dir(agent_home);
    if !category_dir.exists() {
        return Ok(0);
    }

    Ok(fs::read_dir(&category_dir)
        .map_err(|error| format!("读取分类目录失败: {error}"))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|item| item.to_str()) == Some("md"))
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

fn count_wiki_pages(agent_home: &Path) -> Result<usize, String> {
    let wiki_dir = agent_home.join(WIKI_DIR);
    if !wiki_dir.exists() {
        return Ok(0);
    }

    let mut count = 0usize;
    let mut stack = vec![wiki_dir];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|error| format!("读取 wiki 目录失败: {error}"))?
        {
            let entry = entry.map_err(|error| format!("读取 wiki 条目失败: {error}"))?;
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

fn contains_any(content: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| content.contains(needle))
}

fn contains_all(content: &str, needles: &[&str]) -> bool {
    needles.iter().all(|needle| content.contains(needle))
}
