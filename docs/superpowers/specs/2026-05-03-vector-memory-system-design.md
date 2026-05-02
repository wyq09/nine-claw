# Vector Memory System Design

## Overview

为 NineClaw 添加基于向量数据库的记忆系统。内置 bge-small-zh ONNX 模型实现开箱即用的语义搜索，支持插件式多 Embedding Provider，提供 memory_update、memory_search、memory_read、memory_delete 四个系统工具。

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Tool form | .mjs runtime tools via credential proxy, same as web_search/web_fetch | Consistent with existing architecture |
| Vector storage | SQLite + sqlite-vec, independent table + FK | Swappable when migrating to dedicated vector DB |
| Built-in model | ONNX Runtime + bge-small-zh | Cross-platform, runs on CPU, ~90MB |
| Custom embedding | Plugin-style multi-provider (local ONNX / OpenAI / self-hosted) | Users can freely switch per workspace |
| Callers | LLM runtime + Rust backend internal | Both agent tools and memory extraction pipeline |
| Index granularity | Whole memory (title + content) | LLM-extracted memories are short enough |
| Search result format | Hybrid: snippet + score, LLM calls memory_read for full content | Saves context window |
| Internal calls | Direct Rust function calls, no HTTP overhead | Backend paths bypass proxy |
| LLM runtime calls | Through credential proxy, same as existing tools | Consistent tool interface |

## Part 1: Data Layer

### memory_vectors table

```sql
CREATE TABLE memory_vectors (
    id TEXT PRIMARY KEY,
    memory_id TEXT NOT NULL REFERENCES workspace_memories(id) ON DELETE CASCADE,
    workspace_id TEXT NOT NULL,
    embedding BLOB NOT NULL,
    embedding_model TEXT NOT NULL,
    dimension INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX idx_mv_workspace ON memory_vectors(workspace_id);
CREATE INDEX idx_mv_memory ON memory_vectors(memory_id);
```

### embedding_providers config table

```sql
CREATE TABLE embedding_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider_type TEXT NOT NULL,      -- "onnx_local" | "remote_api"
    endpoint TEXT,
    model_name TEXT NOT NULL,
    api_key_ref TEXT,
    dimension INTEGER NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0
);
```

App startup registers `bge-small-zh-local` as default (ONNX local, 512-dim, is_default=1).

### Constraints

- All vectors in a workspace must have the same dimension. Switching provider requires rebuilding the index.
- CASCADE delete on `workspace_memories` removes corresponding vectors.
- Deduplication reuses existing title+content fingerprint, checked before vector insertion.

## Part 2: Embedding Provider Plugin Architecture

### Rust trait

```rust
#[async_trait]
trait EmbeddingProvider: Send + Sync {
    fn id(&self) -> &str;
    fn dimension(&self) -> usize;
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>>;
}
```

### Implementations

**OnnxLocalProvider** (built-in):
- Loads bge-small-zh ONNX model on startup (~90MB, bundled in resources)
- CPU inference, ~20ms per text
- No network required

**RemoteApiProvider** (remote API):
- Generic HTTP client, compatible with OpenAI embedding API format
- Supports custom endpoint / model / api_key
- Auto-adapts to returned dimension

**ProviderRegistry** (singleton):
- `get_provider(provider_id)` - lookup by ID
- `get_default()` - returns default provider
- `get_provider_for_workspace(workspace_id)` - returns workspace-specific provider
- Notifies index rebuild on config change

### Model resource management

```
src-tauri/resources/
  embedding-models/
    bge-small-zh-v1.5/
      model.onnx
      tokenizer.json
```

Startup checks file integrity; restores from bundled resources if corrupted.

## Part 3: Four Memory Tools

### memory_update

Write or update a memory with vector index.

```json
// Params
{
  "title": "string (required)",
  "content": "string (required)",
  "tags": ["string"],
  "memory_id": "string (optional, update if present)"
}
```

Flow:
1. Write/update `workspace_memories` table (reuse existing logic)
2. Call workspace embedding provider to generate vector
3. Upsert `memory_vectors`
4. Sync write `.md` file (keep filesystem consistency)

### memory_search

Vector semantic search across memories.

```json
// Params
{
  "query": "string (required)",
  "limit": "number (default 10)",
  "threshold": "number (default 0.5, cosine similarity minimum)",
  "tags": ["string (optional, filter)"]
}
```

```json
// Response
{
  "results": [
    {
      "memory_id": "string",
      "title": "string",
      "content_snippet": "first 200 chars",
      "score": 0.87,
      "tags": ["fact", "constraint"],
      "updated_at": 1714700000
    }
  ],
  "provider": "bge-small-zh-local",
  "query_embedding_dim": 512
}
```

Flow:
1. Generate embedding for query
2. sqlite-vec cosine similarity search
3. Optional tag filtering via SQL WHERE
4. Return snippet + score, not full content

### memory_read

Get full memory content by ID.

```json
// Params
{ "memory_id": "string (required)" }
// Returns: full title, content, tags, created_at, updated_at
```

### memory_delete

Delete memory and its vector index.

```json
// Params
{ "memory_id": "string (required)" }
// Flow: CASCADE removes vector, also deletes .md file
```

### Tool registration

Add tool IDs to `agents.rs` default tool list. Generate `.mjs` bridge files in `managed_runtime_extension.rs`. Tools call back to Rust backend via credential proxy.

## Part 4: Rust Backend Internal Call Paths

### New modules

```
src-tauri/src/
  embedding/
    mod.rs              # EmbeddingProvider trait + ProviderRegistry
    onnx_local.rs       # OnnxLocalProvider implementation
    remote_api.rs       # RemoteApiProvider implementation
  memory_vector/
    mod.rs              # Vector table CRUD + search
    vector_search.rs    # sqlite-vec query logic
    migration.rs        # Table migration
```

### Internal call scenarios (direct function calls, no HTTP)

**1. Auto-index on memory extraction**

After `write_team_memory_entry()` in `workspace_memory_extraction.rs`:
```
→ embedding_provider.embed(title + "\n" + content)
→ memory_vector::upsert_vector(memory_id, embedding, provider_id)
```

**2. Vector deduplication upgrade**

Current dedup is title+content fingerprint exact match. Add vector similarity dedup:
```
Compute new memory embedding
→ memory_vector::search_similar(embedding, threshold=0.95)
→ Above threshold = duplicate, update instead of create
```

**3. memory_wiki.rs keyword match replacement**

```
Current: keyword match prompt content → return file hints
New: prompt content → embedding → vector search → return relevant memory snippets
```

### Credential proxy new routes (for LLM runtime only)

```
POST /proxy/memory/update
POST /proxy/memory/search
POST /proxy/memory/read
POST /proxy/memory/delete
```

## Part 5: Startup and Performance

### Startup sequence

```
1. Check embedding model file integrity
   ├── OK → load ONNX model into memory
   └── Corrupted → restore from bundled resources, then load
2. Initialize OnnxLocalProvider, register in ProviderRegistry
3. SQLite migration: create memory_vectors + embedding_providers tables
4. Register default bge-small-zh-local provider if not registered
5. Check for missing vectors (memories without embeddings)
   └── Background async rebuild if needed
```

### Vector index rebuild

Existing `workspace_memories` records may lack vectors. Background task:
- Batch fetch un-vectorized memories (50 per batch)
- Batch embed (OnnxLocalProvider supports batch)
- Batch insert into memory_vectors
- Non-blocking, does not block conversation

### Performance strategy

| Operation | Strategy |
|-----------|----------|
| Write memory | Sync embed + write vector (~20ms, imperceptible) |
| Batch rebuild | Background thread, batch embed, 50 per batch |
| Search | sqlite-vec cosine similarity, 512-dim, <5ms for 10K rows |
| ONNX model | Load once at startup, resident in memory (~200MB) |
| Embedding cache | Same content hash = skip redundant embed |

### New Cargo dependencies

```toml
[dependencies]
ort = { version = "2", features = ["load-dynamic"] }
sqlite-vec = "0.1"
tokenizers = "0.20"
```

`ort` uses `load-dynamic` for runtime ONNX Runtime loading. ONNX Runtime shared libraries (.so/.dylib/.dll) bundled in app resources.
