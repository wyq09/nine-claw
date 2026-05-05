# Three-Layer Memory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Add system/workspace/agent three-layer memory scope to the existing vector memory system.

**Architecture:** Add `scope` and `scope_agent_id` columns to existing `workspace_memories` table. LLM extraction auto-classifies scope. Search merges all applicable layers. New frontend memory viewer panel.

**Tech Stack:** Rust (rusqlite), TypeScript/React (Tauri frontend)

---

## File Structure

### New files:
| File | Responsibility |
|------|---------------|
| `src-tauri/src/commands_memory.rs` | Tauri commands for memory viewer (list by scope, update scope) |

### Modified files:
| File | Change |
|------|--------|
| `src-tauri/src/storage/workspaces.rs` | Add scope/scope_agent_id to insert/update/list, migration helper |
| `src-tauri/src/storage/db.rs` | No change (migration called from workspaces ensure_schema) |
| `src-tauri/src/prompts/workspace_memory_extraction.rs` | Add scope to extraction prompt |
| `src-tauri/src/workspace_memory_extraction.rs` | Parse scope from LLM output, pass to write |
| `src-tauri/src/team_workspace.rs` | Accept scope params in write_team_memory_entry |
| `src-tauri/src/memory_vector/mod.rs` | Scope-aware search, scope-aware upsert |
| `src-tauri/src/memory_vector/vector_search.rs` | Scope-aware cosine_search |
| `src-tauri/src/managed_runtime.rs` | Update proxy handlers for scope |
| `src-tauri/src/agent_workspace/memory_wiki.rs` | Three-layer vector hints |
| `src/runtime-tools/memory_update_tool.mjs` | Add scope parameter |
| `src/runtime-tools/memory_search_tool.mjs` | Include scope in results |
| `src-tauri/src/lib.rs` | Register new Tauri commands |
| `src/components/SettingsModal.tsx` | Add MemoryPanel tab |

---

## Task 1: Schema Migration — Add scope columns

**Files:**
- Modify: `src-tauri/src/storage/workspaces.rs`

- [ ] **Step 1: Add migration in ensure_schema**

In `src-tauri/src/storage/workspaces.rs`, find `ensure_schema` function. After the existing CREATE TABLE for workspace_memories, add:

```rust
// Add scope columns if missing (three-layer memory migration)
add_column_if_missing(conn, "workspace_memories", "scope", "TEXT NOT NULL DEFAULT 'workspace'")?;
add_column_if_missing(conn, "workspace_memories", "scope_agent_id", "TEXT")?;
conn.execute_batch(
    "CREATE INDEX IF NOT EXISTS idx_wm_scope ON workspace_memories(scope, workspace_id);
     CREATE INDEX IF NOT EXISTS idx_wm_agent ON workspace_memories(scope_agent_id) WHERE scope = 'agent';"
).map_err(|e| format!("创建 scope 索引失败: {e}"))?;
```

The `add_column_if_missing` helper already exists in this file (lines 14-38).

- [ ] **Step 2: Update WorkspaceMemoryRecord struct**

Add two fields to `WorkspaceMemoryRecord`:

```rust
pub scope: String,
pub scope_agent_id: Option<String>,
```

Update `row_to_workspace_memory` mapping function to read these columns. Use `.get()` which returns default for missing columns during migration.

- [ ] **Step 3: Update insert_workspace_memory**

Add `scope` and `scope_agent_id` parameters to the INSERT statement.

- [ ] **Step 4: Update update_workspace_memory**

Add optional `scope` and `scope_agent_id` parameters to the UPDATE statement.

- [ ] **Step 5: Update list_workspace_memories**

Add optional `scope` filter parameter.

- [ ] **Step 6: Add new query functions**

```rust
pub fn list_memories_by_scope(conn, workspace_id, scope, limit) -> Result<Vec<WorkspaceMemoryRecord>, String>
pub fn update_memory_scope(conn, memory_id, scope, scope_agent_id) -> Result<(), String>
pub fn count_memories_by_scope(conn, workspace_id) -> Result<(i64, i64, i64), String>  // (system, workspace, agent)
```

- [ ] **Step 7: Run tests**

Run: `cd src-tauri && cargo test storage::workspaces -- --nocapture 2>&1 | tail -20`

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/storage/workspaces.rs
git commit -m "feat: add scope columns to workspace_memories for three-layer memory"
```

---

## Task 2: Update team_workspace write function

**Files:**
- Modify: `src-tauri/src/team_workspace.rs`

- [ ] **Step 1: Update write_team_memory_entry signature**

Add `scope` and `scope_agent_id` parameters:

```rust
pub fn write_team_memory_entry(
    app: &AppHandle,
    workspace_id: &str,
    title: String,
    content: String,
    author_agent_id: Option<String>,
    tags: Vec<String>,
    scope: &str,                  // "system" | "workspace" | "agent"
    scope_agent_id: Option<&str>, // agent ID when scope = "agent"
) -> Result<WorkspaceMemoryRecord, String>
```

Pass scope and scope_agent_id through to `insert_workspace_memory`.

- [ ] **Step 2: Update delete_team_memory_entry**

No change needed (operates by memory_id regardless of scope).

- [ ] **Step 3: Update all callers of write_team_memory_entry**

Search for all call sites and add `scope` and `scope_agent_id` arguments. For the existing extraction path, default to `"workspace"` and `None`. The extraction logic will be updated in Task 4.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo check 2>&1 | tail -10`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/team_workspace.rs
git commit -m "feat: update write_team_memory_entry with scope parameters"
```

---

## Task 3: Scope-aware vector search

**Files:**
- Modify: `src-tauri/src/memory_vector/vector_search.rs`
- Modify: `src-tauri/src/memory_vector/mod.rs`

- [ ] **Step 1: Update cosine_search to accept scope filter**

In `vector_search.rs`, update `cosine_search` to accept a `scopes: Option<Vec<String>>` parameter. When filtering vectors, join with `workspace_memories` and filter by `scope IN (...)`:

For agent-scoped search, also filter by `scope_agent_id`.

- [ ] **Step 2: Update search_vectors wrapper**

Update `mod.rs::search_vectors` to accept scope filter params:

```rust
pub fn search_vectors(
    conn: &Connection,
    workspace_id: &str,
    query_embedding: &[f32],
    limit: usize,
    threshold: f32,
    tag_filter: Option<&[String]>,
    scope_filter: Option<&[String]>,       // NEW
    scope_agent_id: Option<&str>,          // NEW: for agent scope
) -> Result<Vec<SearchHit>, String>
```

- [ ] **Step 3: Add three_layer_search function**

```rust
pub fn three_layer_search(
    conn: &Connection,
    workspace_id: &str,
    agent_id: Option<&str>,
    is_supervisor: bool,
    query_embedding: &[f32],
    limit: usize,
    threshold: f32,
) -> Result<Vec<SearchHit>, String>
```

This function:
1. Searches system scope (scope = 'system', all workspaces)
2. Searches workspace scope (scope = 'workspace', workspace_id = W)
3. Searches agent scope (scope = 'agent', workspace_id = W, scope_agent_id = A, or all agents if supervisor)
4. Merges by score, deduplicates by memory_id

- [ ] **Step 4: Update tests**

Update existing tests to pass `None, None` for new params. Add new tests:
- test_search_system_scope_only
- test_search_agent_scope_isolated
- test_three_layer_search_merges_all

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test memory_vector -- --nocapture 2>&1 | tail -20`

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/memory_vector/
git commit -m "feat: scope-aware vector search with three-layer merge"
```

---

## Task 4: Update extraction prompt and logic

**Files:**
- Modify: `src-tauri/src/prompts/workspace_memory_extraction.rs`
- Modify: `src-tauri/src/workspace_memory_extraction.rs`

- [ ] **Step 1: Update extraction prompt**

In `prompts/workspace_memory_extraction.rs`, extend the prompt to include scope in the output format. Add guidance:

```
每条记忆需要判断所属层级:
- "system": 跨项目通用的用户偏好、个人特征、通用知识
- "workspace": 项目级别的决策、事实、约束、计划
- "agent": 特定智能体的执行经验、专属能力描述

输出格式增加 scope 字段:
{"route":"preference","title":"...","content":"...","tags":[...],"scope":"system"}
```

- [ ] **Step 2: Parse scope from LLM output**

In `workspace_memory_extraction.rs`, update memory parsing to extract `scope` field. Default to `"workspace"` if missing.

- [ ] **Step 3: Pass scope to write_team_memory_entry**

Update the extraction loop to pass the parsed scope and agent_id:

```rust
let scope = memory.scope.as_deref().unwrap_or("workspace");
let scope_agent_id = if scope == "agent" { Some(agent_id.as_str()) } else { None };

team_workspace::write_team_memory_entry(
    app, &request.workspace_id, memory.title, memory.content,
    Some(workspace.supervisor_agent_id.clone()), memory.tags,
    scope, scope_agent_id,
)?;
```

- [ ] **Step 4: Update deduplication to be scope-aware**

When checking for duplicates, only compare within the same scope.

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test workspace_memory_extraction -- --nocapture 2>&1 | tail -20`

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/prompts/workspace_memory_extraction.rs src-tauri/src/workspace_memory_extraction.rs
git commit -m "feat: LLM extraction auto-classifies memory scope (system/workspace/agent)"
```

---

## Task 5: Update memory tools (.mjs + proxy handlers)

**Files:**
- Modify: `src/runtime-tools/memory_update_tool.mjs`
- Modify: `src/runtime-tools/memory_search_tool.mjs`
- Modify: `src-tauri/src/managed_runtime.rs`

- [ ] **Step 1: Add scope param to memory_update_tool.mjs**

Add optional `scope` parameter to schema and include in request body.

- [ ] **Step 2: Add scope to memory_search results**

Display scope in search results. No new parameters needed (scope is auto-determined).

- [ ] **Step 3: Update memory_update proxy handler**

Read `scope` from request body, pass to `write_team_memory_entry`.

- [ ] **Step 4: Update memory_search proxy handler**

Use `three_layer_search` instead of plain `search_vectors`. Determine `agent_id` and `is_supervisor` from session config.

- [ ] **Step 5: Run cargo check**

Run: `cd src-tauri && cargo check 2>&1 | tail -10`

- [ ] **Step 6: Commit**

```bash
git add src/runtime-tools/memory_update_tool.mjs src/runtime-tools/memory_search_tool.mjs src-tauri/src/managed_runtime.rs
git commit -m "feat: memory tools support scope parameter and three-layer search"
```

---

## Task 6: Update memory_wiki vector hints

**Files:**
- Modify: `src-tauri/src/agent_workspace/memory_wiki.rs`

- [ ] **Step 1: Use three_layer_search in vector_memory_hints**

Replace `search_vectors` call with `three_layer_search`, passing agent_id from context.

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/agent_workspace/memory_wiki.rs
git commit -m "feat: memory_wiki uses three-layer vector search"
```

---

## Task 7: Backend Tauri commands for memory viewer

**Files:**
- Create: `src-tauri/src/commands_memory.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create commands_memory.rs**

```rust
// List memories grouped by scope
pub(crate) fn memory_list(
    app: AppHandle,
    workspace_id: String,
    scope: Option<String>,     // filter by scope
    limit: Option<usize>,
    page: Option<usize>,
) -> Result<...>

// Update memory scope
pub(crate) fn memory_update_scope(
    app: AppHandle,
    memory_id: String,
    scope: String,
    scope_agent_id: Option<String>,
) -> Result<...>

// Count memories by scope
pub(crate) fn memory_stats(
    app: AppHandle,
    workspace_id: String,
) -> Result<(i64, i64, i64), String>  // (system, workspace, agent)

// Search memories (text-based, for UI)
pub(crate) fn memory_search_text(
    app: AppHandle,
    workspace_id: String,
    query: String,
    scope: Option<String>,
    limit: Option<usize>,
) -> Result<...>
```

Each command delegates to `storage::workspaces` functions.

- [ ] **Step 2: Register commands in lib.rs**

Add `invoke_handler` registrations for the new commands.

- [ ] **Step 3: Run cargo check**

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands_memory.rs src-tauri/src/lib.rs
git commit -m "feat: Tauri commands for memory viewer (list, scope update, stats)"
```

---

## Task 8: Frontend Memory Viewer Panel

**Files:**
- Create: `src/components/MemoryPanel.tsx`
- Modify: `src/components/SettingsModal.tsx`

- [ ] **Step 1: Create MemoryPanel.tsx**

React component with:
- Tab bar: [全部] [系统级] [团队级] [智能体级]
- Search input
- Memory list grouped by scope
- Each memory card shows: title, content preview, tags, scope badge, updated_at
- Click to expand full content
- Dropdown to change scope
- Delete button with confirmation
- Pagination

- [ ] **Step 2: Add MemoryPanel to SettingsModal**

Add a "记忆" tab in the settings modal that renders MemoryPanel.

- [ ] **Step 3: Commit**

```bash
git add src/components/MemoryPanel.tsx src/components/SettingsModal.tsx
git commit -m "feat: memory viewer UI panel with scope filtering"
```

---

## Task 9: Integration Tests

**Files:**
- Modify: `src-tauri/src/memory_vector/mod.rs` (add tests)

- [ ] **Step 1: Add three-layer specific tests**

Tests to add:
- test_scope_default_is_workspace — insert without scope, verify scope = 'workspace'
- test_scope_system_searchable_from_any_workspace — system memory found in all workspaces
- test_scope_agent_isolated — agent A's memories not found by agent B
- test_scope_agent_visible_to_supervisor — supervisor sees all agent memories
- test_three_layer_search_priority — merge results from all 3 layers
- test_scope_update_migration — verify existing memories keep workspace scope

- [ ] **Step 2: Run full test suite**

Run: `cd src-tauri && cargo test 2>&1 | grep -E "test result|FAILED"`

- [ ] **Step 3: Generate test report**

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_vector/mod.rs
git commit -m "test: three-layer memory integration tests"
```

---

## Task 10: Final Verification

- [ ] **Step 1: Run full test suite**
- [ ] **Step 2: Verify cargo check --release**
- [ ] **Step 3: Generate final test report**
- [ ] **Step 4: Commit any fixes**
