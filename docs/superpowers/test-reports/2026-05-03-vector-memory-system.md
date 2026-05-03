# Vector Memory System — Test Report

**Date:** 2026-05-03
**Plan:** `docs/superpowers/plans/2026-05-03-vector-memory-system.md`
**Spec:** `docs/superpowers/specs/2026-05-03-vector-memory-system-design.md`

## Summary

| Metric | Value |
|--------|-------|
| Total tests | 280 |
| Passed | 279 |
| Failed | 1 (pre-existing, unrelated) |
| Pass rate | 99.6% |

## Pre-existing Failure

- `agent_workspace::tests::register_agent_attachment_source_records_indexes` — DirectoryNotEmpty cleanup error, existed before vector memory work began.

## Vector Memory Integration Tests (21 tests)

File: `src-tauri/tests/vector_memory_integration.rs`

| # | Test | Status |
|---|------|--------|
| 1 | test_full_crud_lifecycle | PASS |
| 2 | test_cosine_search_returns_similar | PASS |
| 3 | test_cosine_search_with_threshold | PASS |
| 4 | test_search_scoped_to_workspace | PASS |
| 5 | test_search_empty_workspace_returns_nothing | PASS |
| 6 | test_find_memories_without_vectors | PASS |
| 7 | test_different_dimensions_in_same_workspace | PASS |
| 8 | test_embedding_blob_roundtrip | PASS |
| 9 | test_multiple_vectors_ranked_by_similarity | PASS |
| 10 | test_update_memory_preserves_vector | PASS |
| 11 | test_find_similar_above_threshold | PASS |
| 12 | test_find_similar_below_threshold | PASS |
| 13 | test_three_layer_system_scope | PASS |
| 14 | test_three_layer_agent_isolation | PASS |
| 15 | test_three_layer_supervisor_sees_all | PASS |
| 16 | test_backfill_finds_unindexed | PASS |
| 17 | test_backfill_no_missing | PASS |
| 18 | test_delete_nonexistent | PASS |
| 19 | test_get_nonexistent | PASS |
| 20 | test_workspace_memory_storage_crud | PASS |
| 21 | test_three_layer_merge_all_scopes | PASS |

## Unit Tests (covered modules)

- `storage::workspaces` — workspace memory CRUD, KV memory CRUD
- `storage::migrations` — schema migration idempotency
- `memory_vector` — vector upsert, search, cosine similarity
- `memory_vector::embedding` — ONNX bge-small-zh provider, remote API provider
- `memory_vector::embedding::remote_api` — provider config, dimension, error handling
- `workspace_memory_extraction` — LLM output parsing, memory dedup, gate routing
- `team_supervisor` — agent input construction
- `team_workspace` — team member context
- `widget_runtime` — widget status updates

## Memory Tool Coverage

| Tool | Route | Vector Indexed |
|------|-------|---------------|
| memory_update | managed_runtime.rs | YES — MEMORY.md → workspace_memories + memory_vectors |
| memory_search | managed_runtime.rs | YES — cosine search via embedding |
| memory_read | managed_runtime.rs | N/A — reads MEMORY.md file |
| memory_delete | managed_runtime.rs | YES — removes from DB + vectors |
| memory_store | commands_workspace_kv_memory.rs | YES — KV → workspace_memories + memory_vectors |
| memory_get | commands_workspace_kv_memory.rs | N/A — reads by key |
| memory_forget | commands_workspace_kv_memory.rs | YES — removes KV + vector |
| memory_list | commands_workspace_kv_memory.rs | N/A — lists keys |
| chat_search | managed_runtime.rs | N/A — keyword search on chat history |

## Key Implementation Fix

**Problem:** `memory_update` wrote content to MEMORY.md file only. `memory_search` queried workspace_memories + memory_vectors database tables. Content saved by agents was invisible to semantic search.

**Fix:** `memory_update_handler` now also indexes content into workspace_memories and auto-computes embeddings + upserts into memory_vectors. Uses stable `memory_md_{agent_id}` record ID and same workspace resolution as `memory_search_handler` for consistency.

## Conclusion

All 12 plan tasks completed. Vector memory system fully functional with 21 integration tests covering CRUD, cosine search, threshold filtering, workspace scoping, three-layer scope (system/workspace/agent), backfill, and edge cases.
