# Three-Layer Memory Architecture Design

## Overview

在现有 workspace 级向量记忆基础上，增加 system 级（全局）和 agent 级（智能体私有）两层记忆。三层记忆共享同一个向量存储基础设施，通过 `scope` 字段区分层级。同时新增前端记忆查看模块。

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Data model | Add `scope` column to existing tables | No new tables, backward compatible |
| Scope values | `system` / `workspace` / `agent` | Three distinct sharing boundaries |
| Agent search visibility | Agent sees own + supervisor sees all agents | Supervisor needs visibility for coordination |
| System write permission | LLM extraction + user manual | Both paths can write system-level |
| Migration | Default `workspace` + manual adjustment | Safest, no LLM reclassification risk |
| Memory viewer UI | Frontend panel listing all memories | Users need to see and manage what's stored |

## Part 1: Data Layer Changes

### workspace_memories table modification

```sql
ALTER TABLE workspace_memories ADD COLUMN scope TEXT NOT NULL DEFAULT 'workspace';
ALTER TABLE workspace_memories ADD COLUMN scope_agent_id TEXT;
-- scope values: 'system' | 'workspace' | 'agent'
-- scope_agent_id: only set when scope = 'agent', references agent ID
```

### memory_vectors table — no schema change

`memory_vectors` already has `workspace_id`. Search filtering by scope is done by joining with `workspace_memories.scope`.

### New index

```sql
CREATE INDEX IF NOT EXISTS idx_wm_scope ON workspace_memories(scope, workspace_id);
CREATE INDEX IF NOT EXISTS idx_wm_agent ON workspace_memories(scope_agent_id) WHERE scope = 'agent';
```

### Scope semantics

| Scope | workspace_id | scope_agent_id | Who can see |
|-------|-------------|----------------|-------------|
| `system` | ignored | NULL | All agents in all workspaces |
| `workspace` | workspace ID | NULL | All agents in that workspace |
| `agent` | workspace ID | agent ID | That agent only + supervisor |

## Part 2: Memory Extraction Changes

### LLM prompt extension

The extraction prompt in `prompts/workspace_memory_extraction.rs` gains a `scope` field for each extracted memory. The LLM determines scope based on these rules:

| Signal | Scope | Example |
|--------|-------|---------|
| Cross-project user preference | `system` | "用户偏好简洁回复，不用 emoji" |
| Global user profile info | `system` | "用户叫张三，后端负责人" |
| Project-specific fact | `workspace` | "项目用 Rust + React + Tauri" |
| Project decision | `workspace` | "向量存储选择 sqlite-vec" |
| Agent-specific experience | `agent` | "XX 参数需要调 3 次才能成功" |
| Agent capability note | `agent` | "这个 agent 擅长代码审查" |

LLM output format change:
```json
{
  "memories": [
    {
      "route": "preference",
      "title": "用户偏好简洁回复",
      "content": "不用 emoji，直接给结论",
      "tags": ["preference"],
      "scope": "system"
    }
  ]
}
```

### Extraction logic changes

In `workspace_memory_extraction.rs`, after parsing each memory:
1. Read the `scope` field from LLM output (default to `"workspace"` if missing)
2. If `scope == "agent"`, set `scope_agent_id` to the current agent's ID
3. Pass scope info to `write_team_memory_entry`

### `write_team_memory_entry` signature change

```rust
pub fn write_team_memory_entry(
    app: &AppHandle,
    workspace_id: &str,
    title: String,
    content: String,
    author_agent_id: Option<String>,
    tags: Vec<String>,
    scope: &str,              // NEW: "system" | "workspace" | "agent"
    scope_agent_id: Option<&str>, // NEW: agent ID when scope = "agent"
) -> Result<WorkspaceMemoryRecord, String>
```

### Deduplication scope-aware

Vector dedup respects scope:
- `system` memories dedup against all system memories
- `workspace` memories dedup against same workspace
- `agent` memories dedup against same agent in same workspace

## Part 3: Search Layer Changes

### Three-layer search merge

`memory_search` handler searches all applicable layers and merges results:

For **agent A in workspace W**:
1. Search system memories (scope = 'system')
2. Search workspace memories (scope = 'workspace', workspace_id = W)
3. Search agent memories (scope = 'agent', scope_agent_id = A, workspace_id = W)
4. Merge by score, dedup by memory_id

For **supervisor in workspace W**:
1. Search system memories
2. Search workspace memories
3. Search ALL agent memories in workspace W (all scope_agent_ids)
4. Merge by score, dedup by memory_id

Response includes scope for each result:
```json
{
  "results": [
    {
      "memory_id": "...",
      "title": "...",
      "content_snippet": "...",
      "score": 0.92,
      "scope": "system",
      "tags": ["preference"],
      "updated_at": 1714700000
    }
  ]
}
```

### `memory_wiki.rs` vector hints

`vector_memory_hints` also searches all 3 applicable layers, limited to top 3 across all scopes.

## Part 4: Memory Tools Changes

### memory_update

Add optional `scope` parameter:
```json
{
  "title": "string",
  "content": "string",
  "tags": ["string"],
  "memory_id": "string (optional)",
  "scope": "string (optional, default 'workspace')",
  "scope_agent_id": "string (optional, for agent scope)"
}
```

### memory_search

No parameter change — scope is auto-determined by caller identity.

### memory_read / memory_delete

No change — operate by memory_id regardless of scope.

## Part 5: Startup and Migration

### Schema migration

In `migration.rs`, use `add_column_if_missing` pattern (already established in codebase):

```rust
// Add scope column if missing
if !column_exists(conn, "workspace_memories", "scope") {
    conn.execute_batch(
        "ALTER TABLE workspace_memories ADD COLUMN scope TEXT NOT NULL DEFAULT 'workspace';
         ALTER TABLE workspace_memories ADD COLUMN scope_agent_id TEXT;
         CREATE INDEX IF NOT EXISTS idx_wm_scope ON workspace_memories(scope, workspace_id);
         CREATE INDEX IF NOT EXISTS idx_wm_agent ON workspace_memories(scope_agent_id) WHERE scope = 'agent';"
    )?;
}
```

### Index backfill

Existing memories default to `scope = 'workspace'`. No automatic reclassification. Users can manually adjust via the memory viewer UI.

## Part 6: Memory Viewer UI

### Frontend component

New `MemoryPanel` component accessible from settings or workspace sidebar:

**Features:**
- List all memories grouped by scope (System / Workspace / Agent)
- Filter by scope, tags, date range
- Search memories (text search, calls memory_search backend)
- View individual memory detail (calls memory_read)
- Edit memory scope (move between system/workspace/agent)
- Delete memory
- Memory count stats per scope

**Backend support:**
- New Tauri command `workspace_list_memories_with_scope` — returns memories grouped by scope
- New Tauri command `workspace_update_memory_scope` — changes a memory's scope
- Reuse existing `workspace_write_memory`, `workspace_delete_memory` commands

### UI layout (text description)

```
┌─ 记忆管理 ─────────────────────────────┐
│ [全部] [系统级] [团队级] [智能体级]      │
│                                         │
│ 🔍 搜索记忆...                          │
│                                         │
│ ── 系统级 (12) ──                       │
│  · 用户偏好简洁回复          pref  87%   │
│  · 用户叫张三，后端负责人     people 92% │
│                                         │
│ ── 团队级 (34) ──                       │
│  · 项目用 Rust + React       fact  --   │
│  · 向量存储选择 sqlite-vec   dec   --   │
│                                         │
│ ── 智能体级 (5) ──                      │
│  · code-review agent 擅长... fact  --   │
│                                         │
│            [← 1 2 3 →]                  │
└─────────────────────────────────────────┘
```

## Part 7: Cargo and Dependency Changes

No new Cargo dependencies needed. All changes are within existing infrastructure.

## Part 8: File Change Summary

### New files:
| File | Responsibility |
|------|---------------|
| `src/app/settings/MemoryPanel.tsx` | Memory viewer UI component |
| `src-tauri/src/commands_memory.rs` | Tauri commands for memory viewer |

### Modified files:
| File | Change |
|------|--------|
| `src-tauri/src/storage/workspaces.rs` | Add scope to insert/update/list, add_column_if_missing migration |
| `src-tauri/src/storage/db.rs` | Call migration |
| `src-tauri/src/prompts/workspace_memory_extraction.rs` | Add scope to extraction prompt |
| `src-tauri/src/workspace_memory_extraction.rs` | Parse scope from LLM output, pass to write |
| `src-tauri/src/team_workspace.rs` | Accept scope params in write_team_memory_entry |
| `src-tauri/src/memory_vector/mod.rs` | Scope-aware search |
| `src-tauri/src/memory_vector/vector_search.rs` | Scope-aware cosine_search |
| `src-tauri/src/managed_runtime.rs` | Update proxy handlers for scope |
| `src-tauri/src/agent_workspace/memory_wiki.rs` | Three-layer vector hints |
| `src/runtime-tools/memory_update_tool.mjs` | Add scope parameter |
| `src/runtime-tools/memory_search_tool.mjs` | Include scope in results |
| `src/components/SettingsModal.tsx` | Add MemoryPanel tab |
| `src-tauri/src/lib.rs` | Register new Tauri commands |
