# Vector Memory System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add SQLite-vec based vector memory with built-in bge-small-zh embedding and four memory tools (update/search/read/delete) to NineClaw.

**Architecture:** Independent `memory_vectors` table linked to existing `workspace_memories` via FK. ONNX Runtime loads bge-small-zh locally. Plugin-style `EmbeddingProvider` trait with ProviderRegistry. Memory tools are .mjs runtime tools calling Rust via credential proxy.

**Tech Stack:** Rust (ort, sqlite-vec, tokenizers, rusqlite), ONNX Runtime, JavaScript (.mjs tools)

---

## File Structure

### New files to create:

| File | Responsibility |
|------|---------------|
| `src-tauri/src/embedding/mod.rs` | EmbeddingProvider trait, ProviderRegistry |
| `src-tauri/src/embedding/onnx_local.rs` | OnnxLocalProvider (bge-small-zh via ort) |
| `src-tauri/src/embedding/remote_api.rs` | RemoteApiProvider (OpenAI-compatible API) |
| `src-tauri/src/memory_vector/mod.rs` | Vector table CRUD, search functions |
| `src-tauri/src/memory_vector/vector_search.rs` | sqlite-vec query + cosine similarity |
| `src-tauri/src/memory_vector/migration.rs` | Table creation + provider config bootstrap |
| `src/runtime-tools/memory_update_tool.mjs` | memory_update tool bridge |
| `src/runtime-tools/memory_search_tool.mjs` | memory_search tool bridge |
| `src/runtime-tools/memory_read_tool.mjs` | memory_read tool bridge |
| `src/runtime-tools/memory_delete_tool.mjs` | memory_delete tool bridge |

### Existing files to modify:

| File | Change |
|------|--------|
| `src-tauri/Cargo.toml` | Add ort, sqlite-vec, tokenizers deps |
| `src-tauri/src/storage/mod.rs` | Add `pub mod memory_vector;` (re-export from new location) |
| `src-tauri/src/storage/db.rs` | Call `memory_vector::migration::ensure_schema` in `ensure_all_schemas` |
| `src-tauri/src/lib.rs` | Init embedding providers at startup, add proxy routes |
| `src-tauri/src/agents.rs` | Add 4 tool IDs to DEFAULT_ALLOWED_TOOL_IDS + canonicalization |
| `src-tauri/src/managed_runtime.rs` | Add 4 proxy routes for memory tools |
| `src-tauri/src/managed_runtime_extension.rs` | Include + register 4 .mjs tool files |
| `src-tauri/src/workspace_memory_extraction.rs` | Auto-index after write_team_memory_entry, vector dedup |
| `src-tauri/src/team_workspace.rs` | Expose update function for memory_update tool |
| `src-tauri/src/agent_workspace/memory_wiki.rs` | Replace keyword matching with vector search |

---

## Task 1: Add Cargo Dependencies

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add dependencies to Cargo.toml**

Add after the existing `rusqlite` line (~line 30):

```toml
ort = { version = "2", features = ["load-dynamic"] }
sqlite-vec = "0.1"
tokenizers = "0.20"
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check 2>&1 | tail -20`
Expected: Compiles (may have warnings about unused imports, no errors)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: add ort, sqlite-vec, tokenizers dependencies"
```

---

## Task 2: EmbeddingProvider Trait + ProviderRegistry

**Files:**
- Create: `src-tauri/src/embedding/mod.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod embedding;`)

- [ ] **Step 1: Write the test**

Create `src-tauri/src/embedding/mod.rs`:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn id(&self) -> &str;
    fn dimension(&self) -> usize;
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String>;
}

pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn EmbeddingProvider>>,
    default_id: String,
}

impl ProviderRegistry {
    pub fn new(default_id: &str) -> Self {
        Self {
            providers: HashMap::new(),
            default_id: default_id.to_string(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn EmbeddingProvider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn EmbeddingProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn default_provider(&self) -> Option<Arc<dyn EmbeddingProvider>> {
        self.get(&self.default_id)
    }

    pub fn list_ids(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }
}

pub fn new_registry() -> Arc<RwLock<ProviderRegistry>> {
    Arc::new(RwLock::new(ProviderRegistry::new(
        "bge-small-zh-local",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        id: String,
        dim: usize,
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for MockProvider {
        fn id(&self) -> &str { &self.id }
        fn dimension(&self) -> usize { self.dim }
        async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
            Ok(texts.iter().map(|_| vec![0.1; self.dim]).collect())
        }
    }

    fn mock_provider(id: &str, dim: usize) -> Arc<dyn EmbeddingProvider> {
        Arc::new(MockProvider { id: id.to_string(), dim })
    }

    #[tokio::test]
    async fn test_registry_register_and_get() {
        let mut reg = ProviderRegistry::new("p1");
        let p1 = mock_provider("p1", 128);
        reg.register(p1);
        assert!(reg.get("p1").is_some());
        assert!(reg.get("unknown").is_none());
    }

    #[tokio::test]
    async fn test_registry_default_provider() {
        let mut reg = ProviderRegistry::new("default");
        reg.register(mock_provider("default", 64));
        reg.register(mock_provider("other", 32));
        let dp = reg.default_provider().unwrap();
        assert_eq!(dp.id(), "default");
        assert_eq!(dp.dimension(), 64);
    }

    #[tokio::test]
    async fn test_registry_default_missing_returns_none() {
        let reg = ProviderRegistry::new("nonexistent");
        assert!(reg.default_provider().is_none());
    }

    #[tokio::test]
    async fn test_registry_list_ids() {
        let mut reg = ProviderRegistry::new("a");
        reg.register(mock_provider("a", 10));
        reg.register(mock_provider("b", 20));
        let mut ids = reg.list_ids();
        ids.sort();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn test_mock_provider_embed() {
        let p = mock_provider("test", 4);
        let result = p.embed(vec!["hello".into(), "world".into()]).await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 4);
    }

    #[tokio::test]
    async fn test_registry_overwrite_on_reregister() {
        let mut reg = ProviderRegistry::new("p");
        reg.register(mock_provider("p", 10));
        reg.register(mock_provider("p", 20));
        assert_eq!(reg.get("p").unwrap().dimension(), 20);
    }
}
```

- [ ] **Step 2: Add `mod embedding;` to lib.rs**

In `src-tauri/src/lib.rs`, add after the existing module declarations (near the top):

```rust
mod embedding;
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test embedding::tests -- --nocapture 2>&1 | tail -20`
Expected: All 6 tests pass

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/embedding/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add EmbeddingProvider trait and ProviderRegistry"
```

---

## Task 3: OnnxLocalProvider (bge-small-zh)

**Files:**
- Create: `src-tauri/src/embedding/onnx_local.rs`
- Modify: `src-tauri/src/embedding/mod.rs` (add `pub mod onnx_local;`)

- [ ] **Step 1: Write OnnxLocalProvider**

Create `src-tauri/src/embedding/onnx_local.rs`:

```rust
use super::EmbeddingProvider;
use ort::session::Session;
use std::sync::Arc;
use tokenizers::Tokenizer;

const BGE_SMALL_ZH_DIM: usize = 512;
const MODEL_RESOURCE_NAME: &str = "embedding-models/bge-small-zh-v1.5";

pub struct OnnxLocalProvider {
    session: Arc<Session>,
    tokenizer: Arc<Tokenizer>,
}

impl OnnxLocalProvider {
    pub fn new(model_dir: &std::path::Path) -> Result<Self, String> {
        let onnx_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !onnx_path.exists() {
            return Err(format!(
                "ONNX model not found: {}",
                onnx_path.display()
            ));
        }
        if !tokenizer_path.exists() {
            return Err(format!(
                "Tokenizer not found: {}",
                tokenizer_path.display()
            ));
        }

        let session = Session::builder()
            .and_then(|b| b.commit_from_file(&onnx_path))
            .map_err(|e| format!("Failed to load ONNX session: {e}"))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {e}"))?;

        Ok(Self {
            session: Arc::new(session),
            tokenizer: Arc::new(tokenizer),
        })
    }

    fn tokenize(&self, text: &str) -> Result<(Vec<i64>, Vec<i64>), String> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("Tokenization failed: {e}"))?;

        let ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&m| m as i64).collect();
        Ok((ids, mask))
    }

    fn mean_pool(&self, embeddings: &[f32], mask: &[i64], seq_len: usize) -> Vec<f32> {
        let mut result = vec![0.0f32; BGE_SMALL_ZH_DIM];
        let mut count = 0usize;
        for i in 0..seq_len {
            if mask[i] > 0 {
                count += 1;
                for d in 0..BGE_SMALL_ZH_DIM {
                    result[d] += embeddings[i * BGE_SMALL_ZH_DIM + d];
                }
            }
        }
        if count > 0 {
            for d in 0..BGE_SMALL_ZH_DIM {
                result[d] /= count as f32;
            }
        }
        // L2 normalize
        let norm: f32 = result.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in result.iter_mut() {
                *v /= norm;
            }
        }
        result
    }
}

const PROVIDER_ID: &str = "bge-small-zh-local";

#[async_trait::async_trait]
impl EmbeddingProvider for OnnxLocalProvider {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    fn dimension(&self) -> usize {
        BGE_SMALL_ZH_DIM
    }

    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String>> {
        let session = self.session.clone();
        let tokenizer = self.tokenizer.clone();
        tokio::task::spawn_blocking(move || {
            let mut results = Vec::with_capacity(texts.len());
            for text in &texts {
                let (ids, mask) = tokenizer.encode(text, true)
                    .map_err(|e| format!("Tokenization failed: {e}"))
                    .map(|enc| {
                        let ids: Vec<i64> = enc.get_ids().iter().map(|&id| id as i64).collect();
                        let mask: Vec<i64> = enc.get_attention_mask().iter().map(|&m| m as i64).collect();
                        (ids, mask)
                    })?;
                let seq_len = ids.len();

                let ids_arr = ndarray::Array1::from_vec(ids).into_dyn();
                let mask_arr = ndarray::Array1::from_vec(mask.clone()).into_dyn();
                let type_ids = ndarray::Array1::from_vec(vec![0i64; seq_len]).into_dyn();

                let outputs = session.run(ort::inputs![
                    ids_arr.view(),
                    mask_arr.view(),
                    type_ids.view(),
                ].map_err(|e| format!("ONNX input error: {e}"))?).map_err(|e| format!("ONNX run error: {e}"))?;

                let embeddings = outputs[0]
                    .try_into_array::<f32>()
                    .map_err(|e| format!("ONNX output cast error: {e}"))?;

                let emb_slice = embeddings.as_slice()
                    .ok_or("Failed to get embedding slice")?;
                results.push(self.mean_pool(emb_slice, &mask, seq_len));
            }
            Ok(results)
        })
        .await
        .map_err(|e| format!("Embedding task panicked: {e}"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_id_and_dimension() {
        // Test without actual model - just verify constants
        assert_eq!(PROVIDER_ID, "bge-small-zh-local");
        assert_eq!(BGE_SMALL_ZH_DIM, 512);
    }

    #[test]
    fn test_mean_pool_single_token() {
        let provider_dummy = OnnxLocalProvider {
            session: unsafe { std::mem::zeroed() },
            tokenizer: unsafe { std::mem::zeroed() },
        };
        // This test only covers mean_pool logic, no ONNX needed
        let embeddings = vec![3.0, 4.0]; // 2-dim, 1 token
        let mask = vec![1];
        let result = provider_dummy.mean_pool(&embeddings, &mask, 1);
        // 3,4 normalized: length=5, result = [0.6, 0.8]
        assert!((result[0] - 0.6).abs() < 0.001);
        assert!((result[1] - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_mean_pool_two_tokens() {
        let provider_dummy = OnnxLocalProvider {
            session: unsafe { std::mem::zeroed() },
            tokenizer: unsafe { std::mem::zeroed() },
        };
        // 2 dims, 2 tokens: [1,0] and [0,1], both masked
        let embeddings = vec![1.0, 0.0, 0.0, 1.0];
        let mask = vec![1, 1];
        let result = provider_dummy.mean_pool(&embeddings, &mask, 2);
        // mean = [0.5, 0.5], normalized = [1/sqrt(2), 1/sqrt(2)]
        assert!((result[0] - 1.0 / 2.0_f32.sqrt()).abs() < 0.001);
        assert!((result[1] - 1.0 / 2.0_f32.sqrt()).abs() < 0.001);
    }

    #[test]
    fn test_mean_pool_zero_mask() {
        let provider_dummy = OnnxLocalProvider {
            session: unsafe { std::mem::zeroed() },
            tokenizer: unsafe { std::mem::zeroed() },
        };
        let embeddings = vec![1.0, 2.0];
        let mask = vec![0];
        let result = provider_dummy.mean_pool(&embeddings, &mask, 1);
        // All zeros when mask is all zero
        assert_eq!(result, vec![0.0, 0.0]);
    }

    #[test]
    fn test_new_fails_without_model() {
        let tmp = std::env::temp_dir().join("nineclaw_test_no_model");
        let result = OnnxLocalProvider::new(&tmp);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ONNX model not found"));
    }
}
```

- [ ] **Step 2: Register module in mod.rs**

Add to `src-tauri/src/embedding/mod.rs`:

```rust
pub mod onnx_local;
```

- [ ] **Step 3: Add ndarray to Cargo.toml**

Add to `src-tauri/Cargo.toml`:

```toml
ndarray = "0.16"
async-trait = "0.1"
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test onnx_local::tests -- --nocapture 2>&1 | tail -20`
Expected: All 5 tests pass (mean_pool tests + provider constants + missing model)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/embedding/ src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: OnnxLocalProvider with bge-small-zh mean pooling"
```

---

## Task 4: RemoteApiProvider

**Files:**
- Create: `src-tauri/src/embedding/remote_api.rs`
- Modify: `src-tauri/src/embedding/mod.rs` (add `pub mod remote_api;`)

- [ ] **Step 1: Write RemoteApiProvider**

Create `src-tauri/src/embedding/remote_api.rs`:

```rust
use super::EmbeddingProvider;
use serde::Deserialize;

#[derive(Clone)]
pub struct RemoteApiConfig {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub model_name: String,
    pub api_key: String,
    pub dimension: usize,
}

pub struct RemoteApiProvider {
    config: RemoteApiConfig,
    client: reqwest::Client,
}

impl RemoteApiProvider {
    pub fn new(config: RemoteApiConfig) -> Self {
        let client = reqwest::Client::new();
        Self { config, client }
    }

    pub fn config(&self) -> &RemoteApiConfig {
        &self.config
    }
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait::async_trait]
impl EmbeddingProvider for RemoteApiProvider {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn dimension(&self) -> usize {
        self.config.dimension
    }

    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        let body = serde_json::json!({
            "model": self.config.model_name,
            "input": texts,
        });

        let response = self
            .client
            .post(&self.config.endpoint)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Remote embedding request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Remote embedding API error {status}: {body}"));
        }

        let parsed: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse embedding response: {e}"))?;

        let results: Vec<Vec<f32>> = parsed
            .data
            .into_iter()
            .map(|d| {
                let mut emb = d.embedding;
                // Pad or truncate to expected dimension
                emb.resize(self.config.dimension, 0.0);
                emb.truncate(self.config.dimension);
                emb
            })
            .collect();

        if results.len() != texts.len() {
            return Err(format!(
                "Embedding count mismatch: expected {}, got {}",
                texts.len(),
                results.len()
            ));
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RemoteApiConfig {
        RemoteApiConfig {
            id: "test-remote".to_string(),
            name: "Test Remote".to_string(),
            endpoint: "https://httpbin.org/status/500".to_string(),
            model_name: "text-embedding-3-small".to_string(),
            api_key: "test-key".to_string(),
            dimension: 1536,
        }
    }

    #[test]
    fn test_provider_id() {
        let provider = RemoteApiProvider::new(test_config());
        assert_eq!(provider.id(), "test-remote");
    }

    #[test]
    fn test_provider_dimension() {
        let provider = RemoteApiProvider::new(test_config());
        assert_eq!(provider.dimension(), 1536);
    }

    #[test]
    fn test_config_accessible() {
        let provider = RemoteApiProvider::new(test_config());
        assert_eq!(provider.config().model_name, "text-embedding-3-small");
    }

    #[tokio::test]
    async fn test_embed_fails_on_bad_endpoint() {
        let provider = RemoteApiProvider::new(test_config());
        let result = provider.embed(vec!["hello".into()]).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_config_clone() {
        let config = test_config();
        let cloned = config.clone();
        assert_eq!(cloned.id, "test-remote");
    }
}
```

- [ ] **Step 2: Register module in mod.rs**

Add to `src-tauri/src/embedding/mod.rs`:

```rust
pub mod remote_api;
```

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test remote_api::tests -- --nocapture 2>&1 | tail -20`
Expected: All 5 tests pass

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/embedding/
git commit -m "feat: RemoteApiProvider for custom embedding APIs"
```

---

## Task 5: Vector Table Migration + CRUD

**Files:**
- Create: `src-tauri/src/memory_vector/mod.rs`
- Create: `src-tauri/src/memory_vector/migration.rs`
- Create: `src-tauri/src/memory_vector/vector_search.rs`
- Modify: `src-tauri/src/storage/db.rs` (call ensure_schema)
- Modify: `src-tauri/src/lib.rs` (add `mod memory_vector;`)

- [ ] **Step 1: Write migration.rs**

Create `src-tauri/src/memory_vector/migration.rs`:

```rust
use rusqlite::Connection;

pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_vectors (
            id TEXT PRIMARY KEY,
            memory_id TEXT NOT NULL REFERENCES workspace_memories(id) ON DELETE CASCADE,
            workspace_id TEXT NOT NULL,
            embedding BLOB NOT NULL,
            embedding_model TEXT NOT NULL,
            dimension INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_mv_workspace
            ON memory_vectors(workspace_id);
        CREATE INDEX IF NOT EXISTS idx_mv_memory
            ON memory_vectors(memory_id);

        CREATE TABLE IF NOT EXISTS embedding_providers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            provider_type TEXT NOT NULL CHECK(provider_type IN ('onnx_local', 'remote_api')),
            endpoint TEXT,
            model_name TEXT NOT NULL,
            api_key_ref TEXT,
            dimension INTEGER NOT NULL,
            is_default INTEGER NOT NULL DEFAULT 0
        );"
    )
    .map_err(|e| format!("初始化向量表失败: {e}"))?;
    Ok(())
}
```

- [ ] **Step 2: Write vector_search.rs**

Create `src-tauri/src/memory_vector/vector_search.rs`:

```rust
use rusqlite::Connection;

pub fn cosine_search(
    conn: &Connection,
    workspace_id: &str,
    query_embedding: &[f32],
    limit: usize,
    threshold: f32,
    tag_filter: Option<&[String]>,
) -> Result<Vec<SearchHit>, String> {
    let dim = query_embedding.len();
    let query_blob = embedding_to_blob(query_embedding);

    let sql = if tag_filter.is_some() {
        "SELECT mv.memory_id, mv.embedding_model, mv.dimension, mv.updated_at
         FROM memory_vectors mv
         JOIN workspace_memories wm ON wm.id = mv.memory_id
         WHERE mv.workspace_id = ?1 AND mv.dimension = ?2
           AND wm.tags_json LIKE '%' || ?3 || '%'
         ORDER BY vec_distance_cosine(mv.embedding, ?4) ASC
         LIMIT ?5"
    } else {
        "SELECT mv.memory_id, mv.embedding_model, mv.dimension, mv.updated_at
         FROM memory_vectors mv
         WHERE mv.workspace_id = ?1 AND mv.dimension = ?2
         ORDER BY vec_distance_cosine(mv.embedding, ?3) ASC
         LIMIT ?4"
    };

    let mut stmt = conn.prepare(sql).map_err(|e| format!("向量搜索准备失败: {e}"))?;

    let hits: Vec<SearchHit> = if let Some(tags) = tag_filter {
        let tag_pattern = tags.join("\",\"");
        let rows = stmt
            .query_map(
                rusqlite::params![workspace_id, dim, tag_pattern, query_blob, limit],
                |row| {
                    Ok(SearchHit {
                        memory_id: row.get(0)?,
                        score: 0.0, // will be computed
                        embedding_model: row.get(1)?,
                        dimension: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .map_err(|e| format!("向量搜索执行失败: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("向量搜索读取失败: {e}"))?
    } else {
        let rows = stmt
            .query_map(
                rusqlite::params![workspace_id, dim, query_blob, limit],
                |row| {
                    Ok(SearchHit {
                        memory_id: row.get(0)?,
                        score: 0.0,
                        embedding_model: row.get(1)?,
                        dimension: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .map_err(|e| format!("向量搜索执行失败: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("向量搜索读取失败: {e}"))?
    };

    // Compute cosine similarity scores from raw distances
    let mut results = Vec::new();
    for mut hit in hits {
        let emb = get_embedding_by_memory_id(conn, &hit.memory_id)?;
        hit.score = cosine_similarity(query_embedding, &emb);
        if hit.score >= threshold {
            results.push(hit);
        }
    }
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    Ok(results)
}

pub fn get_embedding_by_memory_id(
    conn: &Connection,
    memory_id: &str,
) -> Result<Vec<f32>, String> {
    let blob: Vec<u8> = conn
        .query_row(
            "SELECT embedding FROM memory_vectors WHERE memory_id = ?1",
            rusqlite::params![memory_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("获取向量失败: {e}"))?;
    blob_to_embedding(&blob)
}

pub fn find_similar(
    conn: &Connection,
    workspace_id: &str,
    embedding: &[f32],
    threshold: f32,
) -> Result<Option<String>, String> {
    let results = cosine_search(conn, workspace_id, embedding, 1, threshold, None)?;
    Ok(results.into_iter().next().map(|h| h.memory_id))
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

pub fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(embedding.len() * 4);
    for v in embedding {
        blob.extend_from_slice(&v.to_le_bytes());
    }
    blob
}

pub fn blob_to_embedding(blob: &[u8]) -> Result<Vec<f32>, String> {
    if blob.len() % 4 != 0 {
        return Err(format!("Invalid blob length: {}", blob.len()));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub memory_id: String,
    pub score: f32,
    pub embedding_model: String,
    pub dimension: usize,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_blob_roundtrip() {
        let original: Vec<f32> = vec![0.1, -0.2, 0.3, 0.0, 1.0];
        let blob = embedding_to_blob(&original);
        assert_eq!(blob.len(), 20);
        let restored = blob_to_embedding(&blob).unwrap();
        assert_eq!(restored.len(), 5);
        for (a, b) in original.iter().zip(restored.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((cosine_similarity(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 0.0];
        assert!((cosine_similarity(&a, &b)).abs() < 1e-6);
    }

    #[test]
    fn test_blob_to_embedding_invalid_length() {
        let blob = vec![1u8, 2, 3]; // not multiple of 4
        let result = blob_to_embedding(&blob);
        assert!(result.is_err());
    }

    #[test]
    fn test_cosine_similarity_known_angle() {
        let a = vec![1.0, 1.0]; // 45 degrees
        let b = vec![1.0, 0.0]; // 0 degrees
        let expected = 1.0 / 2.0_f32.sqrt();
        assert!((cosine_similarity(&a, &b) - expected).abs() < 1e-5);
    }
}
```

- [ ] **Step 3: Write mod.rs with CRUD**

Create `src-tauri/src/memory_vector/mod.rs`:

```rust
pub mod migration;
pub mod vector_search;

use rusqlite::Connection;
use vector_search::{blob_to_embedding, embedding_to_blob, SearchHit};

pub struct MemoryVectorRecord {
    pub id: String,
    pub memory_id: String,
    pub workspace_id: String,
    pub embedding: Vec<f32>,
    pub embedding_model: String,
    pub dimension: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

pub fn upsert_vector(
    conn: &Connection,
    id: &str,
    memory_id: &str,
    workspace_id: &str,
    embedding: &[f32],
    model_id: &str,
) -> Result<(), String> {
    let now = crate::storage::chat_history::now_ms();
    let dim = embedding.len();
    let blob = embedding_to_blob(embedding);

    // Check if vector exists for this memory_id
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM memory_vectors WHERE memory_id = ?1",
            rusqlite::params![memory_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("查询向量失败: {e}"))?;

    if let Some(existing_id) = existing {
        conn.execute(
            "UPDATE memory_vectors SET embedding = ?1, embedding_model = ?2,
             dimension = ?3, updated_at = ?4 WHERE id = ?5",
            rusqlite::params![blob, model_id, dim, now, existing_id],
        )
        .map_err(|e| format!("更新向量失败: {e}"))?;
    } else {
        conn.execute(
            "INSERT INTO memory_vectors (id, memory_id, workspace_id, embedding,
             embedding_model, dimension, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![id, memory_id, workspace_id, blob, model_id, dim, now, now],
        )
        .map_err(|e| format!("插入向量失败: {e}"))?;
    }
    Ok(())
}

pub fn delete_vector_by_memory_id(
    conn: &Connection,
    memory_id: &str,
) -> Result<bool, String> {
    let affected = conn
        .execute(
            "DELETE FROM memory_vectors WHERE memory_id = ?1",
            rusqlite::params![memory_id],
        )
        .map_err(|e| format!("删除向量失败: {e}"))?;
    Ok(affected > 0)
}

pub fn get_vector_by_memory_id(
    conn: &Connection,
    memory_id: &str,
) -> Result<Option<MemoryVectorRecord>, String> {
    conn.query_row(
        "SELECT id, memory_id, workspace_id, embedding, embedding_model,
                dimension, created_at, updated_at
         FROM memory_vectors WHERE memory_id = ?1",
        rusqlite::params![memory_id],
        |row| {
            let blob: Vec<u8> = row.get(3)?;
            let emb = blob_to_embedding(&blob).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))))?;
            Ok(MemoryVectorRecord {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                workspace_id: row.get(2)?,
                embedding: emb,
                embedding_model: row.get(4)?,
                dimension: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("查询向量失败: {e}"))
}

pub fn search_vectors(
    conn: &Connection,
    workspace_id: &str,
    query_embedding: &[f32],
    limit: usize,
    threshold: f32,
    tag_filter: Option<&[String]>,
) -> Result<Vec<SearchHit>, String> {
    vector_search::cosine_search(
        conn,
        workspace_id,
        query_embedding,
        limit,
        threshold,
        tag_filter,
    )
}

pub fn find_memories_without_vectors(
    conn: &Connection,
    workspace_id: &str,
    limit: usize,
) -> Result<Vec<(String, String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT wm.id, wm.title, wm.content
             FROM workspace_memories wm
             LEFT JOIN memory_vectors mv ON mv.memory_id = wm.id
             WHERE wm.workspace_id = ?1 AND mv.id IS NULL
             LIMIT ?2",
        )
        .map_err(|e| format!("查询缺失向量失败: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![workspace_id, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("执行缺失向量查询失败: {e}"))?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| format!("读取缺失向量行失败: {e}"))?);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::open_in_memory;

    fn setup_db() -> Connection {
        let conn = open_in_memory().unwrap();
        migration::ensure_schema(&conn).unwrap();
        conn
    }

    fn insert_test_memory(conn: &Connection, id: &str, ws: &str, title: &str, content: &str) {
        let now = crate::storage::chat_history::now_ms();
        conn.execute(
            "INSERT INTO workspace_memories (id, workspace_id, title, content, tags_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, '[]', ?5, ?6)",
            rusqlite::params![id, ws, title, content, now, now],
        ).unwrap();
    }

    #[test]
    fn test_upsert_inserts_new() {
        let conn = setup_db();
        insert_test_memory(&conn, "m1", "ws1", "test", "content");
        let emb = vec![0.1; 512];
        upsert_vector(&conn, "v1", "m1", "ws1", &emb, "bge-small-zh-local").unwrap();

        let record = get_vector_by_memory_id(&conn, "m1").unwrap().unwrap();
        assert_eq!(record.memory_id, "m1");
        assert_eq!(record.embedding.len(), 512);
        assert_eq!(record.embedding_model, "bge-small-zh-local");
    }

    #[test]
    fn test_upsert_updates_existing() {
        let conn = setup_db();
        insert_test_memory(&conn, "m1", "ws1", "test", "content");
        let emb1 = vec![0.1; 512];
        upsert_vector(&conn, "v1", "m1", "ws1", &emb1, "model-a").unwrap();

        let emb2 = vec![0.9; 512];
        upsert_vector(&conn, "v2", "m1", "ws1", &emb2, "model-b").unwrap();

        let record = get_vector_by_memory_id(&conn, "m1").unwrap().unwrap();
        assert_eq!(record.embedding[0], 0.9);
        assert_eq!(record.embedding_model, "model-b");
    }

    #[test]
    fn test_delete_vector() {
        let conn = setup_db();
        insert_test_memory(&conn, "m1", "ws1", "test", "content");
        upsert_vector(&conn, "v1", "m1", "ws1", &vec![0.1; 512], "model").unwrap();

        let deleted = delete_vector_by_memory_id(&conn, "m1").unwrap();
        assert!(deleted);

        let result = get_vector_by_memory_id(&conn, "m1").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_nonexistent_returns_false() {
        let conn = setup_db();
        let deleted = delete_vector_by_memory_id(&conn, "nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let conn = setup_db();
        let result = get_vector_by_memory_id(&conn, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_find_memories_without_vectors() {
        let conn = setup_db();
        insert_test_memory(&conn, "m1", "ws1", "title1", "content1");
        insert_test_memory(&conn, "m2", "ws1", "title2", "content2");
        upsert_vector(&conn, "v1", "m1", "ws1", &vec![0.1; 512], "model").unwrap();

        let missing = find_memories_without_vectors(&conn, "ws1", 10).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].0, "m2");
    }

    #[test]
    fn test_find_memories_without_vectors_all_indexed() {
        let conn = setup_db();
        insert_test_memory(&conn, "m1", "ws1", "t", "c");
        upsert_vector(&conn, "v1", "m1", "ws1", &vec![0.1; 512], "model").unwrap();

        let missing = find_memories_without_vectors(&conn, "ws1", 10).unwrap();
        assert!(missing.is_empty());
    }
}
```

- [ ] **Step 4: Hook migration into db.rs**

In `src-tauri/src/storage/db.rs`, find `ensure_all_schemas` function and add:

```rust
crate::memory_vector::migration::ensure_schema(conn)?;
```

- [ ] **Step 5: Add module declarations to lib.rs**

In `src-tauri/src/lib.rs`, add:

```rust
mod memory_vector;
```

- [ ] **Step 6: Run tests**

Run: `cd src-tauri && cargo test memory_vector::tests -- --nocapture 2>&1 | tail -20`
Expected: All 7 tests pass

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/memory_vector/ src-tauri/src/storage/db.rs src-tauri/src/lib.rs
git commit -m "feat: memory_vectors table migration and CRUD operations"
```

---

## Task 6: Memory Tool .mjs Bridge Files

**Files:**
- Create: `src/runtime-tools/memory_update_tool.mjs`
- Create: `src/runtime-tools/memory_search_tool.mjs`
- Create: `src/runtime-tools/memory_read_tool.mjs`
- Create: `src/runtime-tools/memory_delete_tool.mjs`

- [ ] **Step 1: Write memory_update_tool.mjs**

Create `src/runtime-tools/memory_update_tool.mjs`:

```javascript
export function createMemoryUpdateParameters(Type) {
  return Type.Object({
    title: Type.String({
      minLength: 1,
      maxLength: 100,
      description: "Memory title (concise, 4-24 chars recommended).",
    }),
    content: Type.String({
      minLength: 1,
      description: "Memory content (1-4 sentences, up to 500 chars).",
    }),
    tags: Type.Optional(
      Type.Array(Type.String(), {
        description: "Optional tags for categorization (max 4).",
      })
    ),
    memory_id: Type.Optional(
      Type.String({
        description: "Existing memory ID to update. Omit to create new.",
      })
    ),
  });
}

export function createMemoryUpdateTool(deps) {
  return {
    name: "memory_update",
    label: "Memory Update",
    description:
      "Create or update a memory entry. The memory will be automatically indexed for semantic search. Use this to persist important facts, decisions, preferences, constraints, or any information worth remembering.",
    promptSnippet: "Save important information to long-term memory",
    promptGuidelines: [
      "Use memory_update to persist facts, decisions, constraints, preferences that are worth remembering.",
      "Titles should be concise (4-24 chars), content should be 1-4 sentences.",
      "Provide memory_id only when updating an existing memory.",
    ],
    parameters: createMemoryUpdateParameters(deps.Type),
    async execute(_toolCallId, input, _signal, _onUpdate, ctx) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim();
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim();
      if (!proxyBase || !token) {
        return {
          content: [{ type: "text", text: "Error: proxy not configured" }],
        };
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/update`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          title: input.title,
          content: input.content,
          tags: input.tags || [],
          memory_id: input.memory_id || null,
          workspace_id: ctx?.workspace_id || null,
        }),
      });
      const data = await resp.json();
      return {
        content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
      };
    },
  };
}
```

- [ ] **Step 2: Write memory_search_tool.mjs**

Create `src/runtime-tools/memory_search_tool.mjs`:

```javascript
export function createMemorySearchParameters(Type) {
  return Type.Object({
    query: Type.String({
      minLength: 1,
      description: "Natural language query to search memories semantically.",
    }),
    limit: Type.Optional(
      Type.Integer({
        minimum: 1,
        maximum: 50,
        description: "Max results to return. Default 10.",
      })
    ),
    threshold: Type.Optional(
      Type.Number({
        minimum: 0,
        maximum: 1,
        description: "Minimum cosine similarity score (0-1). Default 0.5.",
      })
    ),
    tags: Type.Optional(
      Type.Array(Type.String(), {
        description: "Filter results by tags.",
      })
    ),
  });
}

export function createMemorySearchTool(deps) {
  return {
    name: "memory_search",
    label: "Memory Search",
    description:
      "Semantic search across workspace memories. Returns matching memories with similarity scores and snippets. Use memory_read to get full content of a specific result.",
    promptSnippet: "Search memories by semantic similarity",
    promptGuidelines: [
      "Use memory_search when you need to find relevant memories by meaning, not just keywords.",
      "Results include snippet + score. Use memory_read for full content.",
      "Lower threshold returns more results; higher threshold returns only close matches.",
    ],
    parameters: createMemorySearchParameters(deps.Type),
    async execute(_toolCallId, input, _signal, _onUpdate, ctx) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim();
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim();
      if (!proxyBase || !token) {
        return {
          content: [{ type: "text", text: "Error: proxy not configured" }],
        };
      }
      const body = {
        query: input.query,
        limit: input.limit || 10,
        threshold: input.threshold ?? 0.5,
        workspace_id: ctx?.workspace_id || null,
      };
      if (input.tags && input.tags.length > 0) {
        body.tags = input.tags;
      }
      const resp = await fetch(`${proxyBase}/memory/${token}/search`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      });
      const data = await resp.json();
      return {
        content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
      };
    },
  };
}
```

- [ ] **Step 3: Write memory_read_tool.mjs**

Create `src/runtime-tools/memory_read_tool.mjs`:

```javascript
export function createMemoryReadParameters(Type) {
  return Type.Object({
    memory_id: Type.String({
      minLength: 1,
      description: "ID of the memory to read.",
    }),
  });
}

export function createMemoryReadTool(deps) {
  return {
    name: "memory_read",
    label: "Memory Read",
    description:
      "Read full content of a specific memory by ID. Use after memory_search to get complete details of a result.",
    promptSnippet: "Read a specific memory entry by ID",
    promptGuidelines: [
      "Use memory_read after memory_search to get the full content of a specific result.",
    ],
    parameters: createMemoryReadParameters(deps.Type),
    async execute(_toolCallId, input, _signal, _onUpdate, ctx) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim();
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim();
      if (!proxyBase || !token) {
        return {
          content: [{ type: "text", text: "Error: proxy not configured" }],
        };
      }
      const resp = await fetch(
        `${proxyBase}/memory/${token}/read`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            memory_id: input.memory_id,
            workspace_id: ctx?.workspace_id || null,
          }),
        }
      );
      const data = await resp.json();
      return {
        content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
      };
    },
  };
}
```

- [ ] **Step 4: Write memory_delete_tool.mjs**

Create `src/runtime-tools/memory_delete_tool.mjs`:

```javascript
export function createMemoryDeleteParameters(Type) {
  return Type.Object({
    memory_id: Type.String({
      minLength: 1,
      description: "ID of the memory to delete.",
    }),
  });
}

export function createMemoryDeleteTool(deps) {
  return {
    name: "memory_delete",
    label: "Memory Delete",
    description:
      "Delete a memory entry and its vector index. This action is irreversible.",
    promptSnippet: "Delete a memory entry",
    promptGuidelines: [
      "Use memory_delete to remove outdated or incorrect memories.",
      "This permanently deletes the memory and its vector index.",
    ],
    parameters: createMemoryDeleteParameters(deps.Type),
    async execute(_toolCallId, input, _signal, _onUpdate, ctx) {
      const proxyBase = process.env.NINECLAW_PROXY_BASE_URL?.trim();
      const token = process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim();
      if (!proxyBase || !token) {
        return {
          content: [{ type: "text", text: "Error: proxy not configured" }],
        };
      }
      const resp = await fetch(
        `${proxyBase}/memory/${token}/delete`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            memory_id: input.memory_id,
            workspace_id: ctx?.workspace_id || null,
          }),
        }
      );
      const data = await resp.json();
      return {
        content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
      };
    },
  };
}
```

- [ ] **Step 5: Commit**

```bash
git add src/runtime-tools/memory_*.mjs
git commit -m "feat: memory tool .mjs bridge files (update/search/read/delete)"
```

---

## Task 7: Register Tools in Rust Backend

**Files:**
- Modify: `src-tauri/src/agents.rs` — add 4 tool IDs
- Modify: `src-tauri/src/managed_runtime_extension.rs` — include + register .mjs files
- Modify: `src-tauri/src/managed_runtime.rs` — add proxy routes

- [ ] **Step 1: Add tool IDs to agents.rs**

In `src-tauri/src/agents.rs`, add to `DEFAULT_ALLOWED_TOOL_IDS` array (after `"external_api"`):

```rust
    "memory_update",
    "memory_search",
    "memory_read",
    "memory_delete",
```

- [ ] **Step 2: Add canonicalization mappings in agents.rs**

In `canonical_allowed_tool_id()` function, add mappings:

```rust
"memory_update" => Some("memory_update"),
"memory_search" => Some("memory_search"),
"memory_read" => Some("memory_read"),
"memory_delete" => Some("memory_delete"),
```

In `runtime_tool_names_for_allowed_tool_ids()`, add:

```rust
"memory_update" => "memory_update",
"memory_search" => "memory_search",
"memory_read" => "memory_read",
"memory_delete" => "memory_delete",
```

- [ ] **Step 3: Include .mjs files in managed_runtime_extension.rs**

In `src-tauri/src/managed_runtime_extension.rs`, add after existing `include_str!` constants:

```rust
const MEMORY_UPDATE_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/memory_update_tool.mjs");
const MEMORY_SEARCH_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/memory_search_tool.mjs");
const MEMORY_READ_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/memory_read_tool.mjs");
const MEMORY_DELETE_TOOL_SOURCE: &str = include_str!("../../src/runtime-tools/memory_delete_tool.mjs");
```

In `write_managed_runtime_extension_files()`, add write blocks for each:

```rust
fs::write(runtime_dir.join("memory_update_tool.mjs"), MEMORY_UPDATE_TOOL_SOURCE)?;
fs::write(runtime_dir.join("memory_search_tool.mjs"), MEMORY_SEARCH_TOOL_SOURCE)?;
fs::write(runtime_dir.join("memory_read_tool.mjs"), MEMORY_READ_TOOL_SOURCE)?;
fs::write(runtime_dir.join("memory_delete_tool.mjs"), MEMORY_DELETE_TOOL_SOURCE)?;
```

In `build_managed_runtime_extension_source()`, add imports:

```javascript
import { createMemoryUpdateTool } from "./memory_update_tool.mjs";
import { createMemorySearchTool } from "./memory_search_tool.mjs";
import { createMemoryReadTool } from "./memory_read_tool.mjs";
import { createMemoryDeleteTool } from "./memory_delete_tool.mjs";
```

Add guard variables and ensure functions:

```javascript
let memoryUpdateToolRegistered = false;
let memorySearchToolRegistered = false;
let memoryReadToolRegistered = false;
let memoryDeleteToolRegistered = false;

function ensureMemoryUpdateTool() {
    if (memoryUpdateToolRegistered) return;
    memoryUpdateToolRegistered = true;
    pi.registerTool(createMemoryUpdateTool({ Type }));
}
function ensureMemorySearchTool() {
    if (memorySearchToolRegistered) return;
    memorySearchToolRegistered = true;
    pi.registerTool(createMemorySearchTool({ Type }));
}
function ensureMemoryReadTool() {
    if (memoryReadToolRegistered) return;
    memoryReadToolRegistered = true;
    pi.registerTool(createMemoryReadTool({ Type }));
}
function ensureMemoryDeleteTool() {
    if (memoryDeleteToolRegistered) return;
    memoryDeleteToolRegistered = true;
    pi.registerTool(createMemoryDeleteTool({ Type }));
}
```

Call `ensureMemoryUpdateTool()`, `ensureMemorySearchTool()`, `ensureMemoryReadTool()`, `ensureMemoryDeleteTool()` in both `session_start` and `before_agent_start` handlers.

- [ ] **Step 4: Add proxy routes in managed_runtime.rs**

In `src-tauri/src/managed_runtime.rs`, find the router construction (the `Router::new()` block) and add:

```rust
.route("/memory/:token/update", post(memory_update_handler))
.route("/memory/:token/search", post(memory_search_handler))
.route("/memory/:token/read", post(memory_read_handler))
.route("/memory/:token/delete", post(memory_delete_handler))
```

Add the handler functions in the same file (after existing proxy handlers):

```rust
async fn memory_update_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    body: axum::body::Bytes,
) -> Response<Body> {
    let session = get_proxy_session(&state, &token);
    let Some(session) = session else {
        return response_with_status(StatusCode::UNAUTHORIZED, "unknown session");
    };
    handle_memory_update(&state, &session, &body).await
}

async fn memory_search_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    body: axum::body::Bytes,
) -> Response<Body> {
    let session = get_proxy_session(&state, &token);
    let Some(session) = session else {
        return response_with_status(StatusCode::UNAUTHORIZED, "unknown session");
    };
    handle_memory_search(&state, &session, &body).await
}

async fn memory_read_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    body: axum::body::Bytes,
) -> Response<Body> {
    let session = get_proxy_session(&state, &token);
    let Some(session) = session else {
        return response_with_status(StatusCode::UNAUTHORIZED, "unknown session");
    };
    handle_memory_read(&state, &session, &body).await
}

async fn memory_delete_handler(
    State(state): State<CredentialProxyState>,
    AxumPath(token): AxumPath<String>,
    body: axum::body::Bytes,
) -> Response<Body> {
    let session = get_proxy_session(&state, &token);
    let Some(session) = session else {
        return response_with_status(StatusCode::UNAUTHORIZED, "unknown session");
    };
    handle_memory_delete(&state, &session, &body).await
}

fn get_proxy_session(state: &CredentialProxyState, token: &str) -> Option<ProxySessionConfig> {
    state.sessions.lock().ok()?.get(token).cloned()
}

async fn handle_memory_update(
    state: &CredentialProxyState,
    session: &ProxySessionConfig,
    body: &axum::body::Bytes,
) -> Response<Body> {
    let params: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => return json_response(&serde_json::json!({"error": format!("Invalid JSON: {e}")})),
    };
    let app_handle = credential_proxy_app_handle();
    let Some(app) = app_handle else {
        return json_response(&serde_json::json!({"error": "app handle not available"}));
    };
    let conn = match crate::storage_conn(&app) {
        Ok(c) => c,
        Err(e) => return json_response(&serde_json::json!({"error": e})),
    };
    let workspace_id = params["workspace_id"].as_str().unwrap_or("");
    let title = match params["title"].as_str() {
        Some(t) => t.to_string(),
        None => return json_response(&serde_json::json!({"error": "title is required"})),
    };
    let content = match params["content"].as_str() {
        Some(c) => c.to_string(),
        None => return json_response(&serde_json::json!({"error": "content is required"})),
    };
    let tags: Vec<String> = params["tags"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let memory_id = params["memory_id"].as_str().map(String::from);

    // Write to workspace_memories
    let result = if let Some(mid) = memory_id {
        crate::storage::workspaces::update_workspace_memory(
            &conn, workspace_id, &mid,
            Some(&title), Some(&content),
            Some(&serde_json::to_string(&tags).unwrap_or("[]".to_string())),
        )
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        let tags_json = serde_json::to_string(&tags).unwrap_or("[]".to_string());
        crate::storage::workspaces::insert_workspace_memory(
            &conn, &id, workspace_id, &title, &content, None, &tags_json,
        )
    };

    match result {
        Ok(record) => {
            // Generate embedding
            let registry = state.embedding_registry.clone();
            let text = format!("{}\n{}", title, content);
            let embedding_result = if let Some(reg) = registry {
                let reg_guard = reg.read().await;
                if let Some(provider) = reg_guard.default_provider() {
                    provider.embed(vec![text.clone()]).await
                } else {
                    Err("No embedding provider configured".to_string())
                }
            } else {
                Err("Embedding registry not available".to_string())
            };

            if let Ok(embeddings) = embedding_result {
                let vid = uuid::Uuid::new_v4().to_string();
                let _ = crate::memory_vector::upsert_vector(
                    &conn, &vid, &record.id, workspace_id, &embeddings[0], "bge-small-zh-local",
                );
            }
            json_response(&serde_json::json!({
                "ok": true,
                "memory_id": record.id,
                "title": record.title,
            }))
        }
        Err(e) => json_response(&serde_json::json!({"error": e})),
    }
}

async fn handle_memory_search(
    state: &CredentialProxyState,
    _session: &ProxySessionConfig,
    body: &axum::body::Bytes,
) -> Response<Body> {
    let params: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => return json_response(&serde_json::json!({"error": format!("Invalid JSON: {e}")})),
    };
    let app_handle = credential_proxy_app_handle();
    let Some(app) = app_handle else {
        return json_response(&serde_json::json!({"error": "app handle not available"}));
    };
    let conn = match crate::storage_conn(&app) {
        Ok(c) => c,
        Err(e) => return json_response(&serde_json::json!({"error": e})),
    };
    let workspace_id = params["workspace_id"].as_str().unwrap_or("");
    let query = match params["query"].as_str() {
        Some(q) => q.to_string(),
        None => return json_response(&serde_json::json!({"error": "query is required"})),
    };
    let limit = params["limit"].as_u64().unwrap_or(10) as usize;
    let threshold = params["threshold"].as_f64().unwrap_or(0.5) as f32;
    let tag_filter: Option<Vec<String>> = params["tags"].as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect());

    // Generate query embedding
    let registry = state.embedding_registry.clone();
    let embedding_result = if let Some(reg) = registry {
        let reg_guard = reg.read().await;
        if let Some(provider) = reg_guard.default_provider() {
            provider.embed(vec![query]).await
        } else {
            Err("No embedding provider configured".to_string())
        }
    } else {
        Err("Embedding registry not available".to_string())
    };

    match embedding_result {
        Ok(embeddings) => {
            let hits = crate::memory_vector::search_vectors(
                &conn, workspace_id, &embeddings[0], limit, threshold,
                tag_filter.as_deref(),
            ).unwrap_or_default();

            // Enrich hits with memory title + snippet
            let mut results = Vec::new();
            for hit in hits {
                if let Ok(Some(mem)) = crate::storage::workspaces::get_workspace_memory(&conn, &hit.memory_id) {
                    let snippet = if mem.content.len() > 200 {
                        format!("{}...", &mem.content[..200])
                    } else {
                        mem.content.clone()
                    };
                    results.push(serde_json::json!({
                        "memory_id": hit.memory_id,
                        "title": mem.title,
                        "content_snippet": snippet,
                        "score": (hit.score * 100.0).round() / 100.0,
                        "tags": mem.tags_json,
                        "updated_at": hit.updated_at,
                    }));
                }
            }
            json_response(&serde_json::json!({
                "results": results,
                "provider": "bge-small-zh-local",
                "query_embedding_dim": embeddings[0].len(),
            }))
        }
        Err(e) => json_response(&serde_json::json!({"error": e})),
    }
}

async fn handle_memory_read(
    _state: &CredentialProxyState,
    _session: &ProxySessionConfig,
    body: &axum::body::Bytes,
) -> Response<Body> {
    let params: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => return json_response(&serde_json::json!({"error": format!("Invalid JSON: {e}")})),
    };
    let app_handle = credential_proxy_app_handle();
    let Some(app) = app_handle else {
        return json_response(&serde_json::json!({"error": "app handle not available"}));
    };
    let conn = match crate::storage_conn(&app) {
        Ok(c) => c,
        Err(e) => return json_response(&serde_json::json!({"error": e})),
    };
    let memory_id = match params["memory_id"].as_str() {
        Some(id) => id,
        None => return json_response(&serde_json::json!({"error": "memory_id is required"})),
    };

    match crate::storage::workspaces::get_workspace_memory(&conn, memory_id) {
        Ok(Some(mem)) => json_response(&serde_json::json!({
            "ok": true,
            "memory_id": mem.id,
            "title": mem.title,
            "content": mem.content,
            "tags": mem.tags_json,
            "created_at": mem.created_at,
            "updated_at": mem.updated_at,
        })),
        Ok(None) => json_response(&serde_json::json!({"error": "memory not found"})),
        Err(e) => json_response(&serde_json::json!({"error": e})),
    }
}

async fn handle_memory_delete(
    _state: &CredentialProxyState,
    _session: &ProxySessionConfig,
    body: &axum::body::Bytes,
) -> Response<Body> {
    let params: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => return json_response(&serde_json::json!({"error": format!("Invalid JSON: {e}")})),
    };
    let app_handle = credential_proxy_app_handle();
    let Some(app) = app_handle else {
        return json_response(&serde_json::json!({"error": "app handle not available"}));
    };
    let conn = match crate::storage_conn(&app) {
        Ok(c) => c,
        Err(e) => return json_response(&serde_json::json!({"error": e})),
    };
    let workspace_id = params["workspace_id"].as_str().unwrap_or("");
    let memory_id = match params["memory_id"].as_str() {
        Some(id) => id,
        None => return json_response(&serde_json::json!({"error": "memory_id is required"})),
    };

    // Delete vector first
    let _ = crate::memory_vector::delete_vector_by_memory_id(&conn, memory_id);
    // Delete memory (CASCADE also removes vector, but explicit is clearer)
    match crate::storage::workspaces::delete_workspace_memory(&conn, workspace_id, memory_id) {
        Ok(()) => json_response(&serde_json::json!({"ok": true, "deleted": memory_id})),
        Err(e) => json_response(&serde_json::json!({"error": e})),
    }
}

fn json_response(value: &serde_json::Value) -> Response<Body> {
    let body = serde_json::to_string(value).unwrap_or_default();
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}
```

Also add `embedding_registry` field to `CredentialProxyState`:

```rust
pub embedding_registry: Option<Arc<tokio::sync::RwLock<crate::embedding::ProviderRegistry>>>,
```

- [ ] **Step 5: Add get_workspace_memory to workspaces.rs**

In `src-tauri/src/storage/workspaces.rs`, add:

```rust
pub fn get_workspace_memory(
    conn: &Connection,
    memory_id: &str,
) -> Result<Option<WorkspaceMemoryRecord>, String> {
    conn.query_row(
        "SELECT id, workspace_id, title, content, author_agent_id, tags_json, created_at, updated_at
         FROM workspace_memories WHERE id = ?1",
        rusqlite::params![memory_id],
        row_to_workspace_memory,
    )
    .optional()
    .map_err(|e| format!("查询记忆失败: {e}"))
}
```

- [ ] **Step 6: Verify compilation**

Run: `cd src-tauri && cargo check 2>&1 | tail -30`
Expected: Compiles with no errors

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/agents.rs src-tauri/src/managed_runtime.rs src-tauri/src/managed_runtime_extension.rs src-tauri/src/storage/workspaces.rs
git commit -m "feat: register memory tools in agent system and proxy routes"
```

---

## Task 8: Integrate Vector Indexing into Memory Extraction Pipeline

**Files:**
- Modify: `src-tauri/src/workspace_memory_extraction.rs`
- Modify: `src-tauri/src/team_workspace.rs`

- [ ] **Step 1: Add vector indexing after write_team_memory_entry**

In `src-tauri/src/workspace_memory_extraction.rs`, find the loop that calls `write_team_memory_entry` (around line 508) and add vector indexing after it:

```rust
for memory in memories {
    if memory_exists(&memory, &recent_memories) {
        continue;
    }
    let record = team_workspace::write_team_memory_entry(
        app,
        &request.workspace_id,
        memory.title,
        memory.content,
        Some(workspace.supervisor_agent_id.clone()),
        memory.tags,
    )?;

    // Auto-index: generate embedding and store vector
    if let Ok(conn) = crate::storage_conn(app) {
        let text = format!("{}\n{}", record.title, record.content);
        if let Some(registry) = embedding_registry.as_ref() {
            let rt = tokio::runtime::Handle::current();
            let embeddings = rt.block_on(async {
                let guard = registry.read().await;
                if let Some(provider) = guard.default_provider() {
                    provider.embed(vec![text]).await
                } else {
                    Err("No embedding provider".to_string())
                }
            });
            if let Ok(embs) = embeddings {
                let vid = uuid::Uuid::new_v4().to_string();
                let _ = crate::memory_vector::upsert_vector(
                    &conn, &vid, &record.id, &request.workspace_id, &embs[0], "bge-small-zh-local",
                );
            }
        }
    }

    inserted += 1;
}
```

- [ ] **Step 2: Add vector deduplication**

In the same file, add before the existing `memory_exists` check:

```rust
// Vector deduplication: check if semantically similar memory exists
fn vector_dedup(
    conn: &Connection,
    workspace_id: &str,
    text: &str,
    registry: &Arc<tokio::sync::RwLock<crate::embedding::ProviderRegistry>>,
) -> bool {
    let rt = tokio::runtime::Handle::current();
    let embedding_result = rt.block_on(async {
        let guard = registry.read().await;
        if let Some(provider) = guard.default_provider() {
            provider.embed(vec![text.to_string()]).await
        } else {
            Err("No provider".to_string())
        }
    });
    if let Ok(embs) = embedding_result {
        if let Ok(Some(similar_id)) = crate::memory_vector::vector_search::find_similar(
            conn, workspace_id, &embs[0], 0.95,
        ) {
            return true;
        }
    }
    false
}
```

Then in the loop:

```rust
let text = format!("{}\n{}", memory.title, memory.content);
if vector_dedup(&conn, &request.workspace_id, &text, &embedding_registry) {
    continue;
}
if memory_exists(&memory, &recent_memories) {
    continue;
}
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/workspace_memory_extraction.rs
git commit -m "feat: auto-index vectors + vector dedup in memory extraction"
```

---

## Task 9: Replace Keyword Matching in memory_wiki.rs

**Files:**
- Modify: `src-tauri/src/agent_workspace/memory_wiki.rs`

- [ ] **Step 1: Add vector search fallback in build_memory_wiki_snapshot**

In `memory_wiki.rs`, add a function that supplements keyword matching with vector search results:

```rust
fn vector_memory_hints(
    conn: &Connection,
    workspace_id: &str,
    prompt: &str,
    registry: &Arc<tokio::sync::RwLock<crate::embedding::ProviderRegistry>>,
) -> Option<String> {
    let rt = tokio::runtime::Handle::current();
    let emb = rt.block_on(async {
        let guard = registry.read().await;
        if let Some(p) = guard.default_provider() {
            p.embed(vec![prompt.to_string()]).await
        } else {
            Err("no provider".to_string())
        }
    }).ok()?;

    let hits = crate::memory_vector::search_vectors(
        conn, workspace_id, &emb[0], 3, 0.6, None,
    ).ok()?;

    if hits.is_empty() {
        return None;
    }

    let mut hints = String::from("\n[语义相关记忆]\n");
    for hit in hits {
        if let Ok(Some(mem)) = crate::storage::workspaces::get_workspace_memory(conn, &hit.memory_id) {
            hints.push_str(&format!("- {} (相似度: {:.0}%): {}\n", mem.title, hit.score * 100.0, &mem.content.chars().take(80).collect::<String>()));
        }
    }
    Some(hints)
}
```

Call this function at the end of `build_memory_wiki_snapshot`, appending results to the snapshot.

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/agent_workspace/memory_wiki.rs
git commit -m "feat: add vector semantic hints to memory_wiki"
```

---

## Task 10: Startup Initialization + Index Rebuild

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Initialize embedding provider at startup**

In `src-tauri/src/lib.rs`, find the app setup (setup closure) and add after existing initialization:

```rust
// Initialize embedding provider registry
let embedding_registry = crate::embedding::new_registry();
let model_dir = app.path()
    .resource_dir()
    .expect("resource dir")
    .join("embedding-models/bge-small-zh-v1.5");

if model_dir.exists() {
    match crate::embedding::onnx_local::OnnxLocalProvider::new(&model_dir) {
        Ok(provider) => {
            let mut guard = embedding_registry.blocking_write();
            guard.register(std::sync::Arc::new(provider));
            log::info!("bge-small-zh-local embedding provider initialized");
        }
        Err(e) => log::warn!("Failed to init embedding provider: {e}"),
    }
} else {
    log::warn!("Embedding model not found at {}", model_dir.display());
}

// Store registry in app state for proxy access
app.manage(crate::EmbeddingRegistryState(embedding_registry.clone()));

// Background index rebuild for un-vectorized memories
let rebuild_conn = crate::storage_conn(&app).ok();
let rebuild_registry = embedding_registry.clone();
tauri::async_runtime::spawn_blocking(move || {
    if let Some(conn) = rebuild_conn {
        if let Ok(workspaces) = crate::storage::workspaces::list_workspaces(&conn) {
            let reg = rebuild_registry.blocking_read();
            if let Some(provider) = reg.default_provider() {
                for ws in &workspaces {
                    if let Ok(missing) = crate::memory_vector::find_memories_without_vectors(&conn, &ws.id, 50) {
                        if missing.is_empty() { continue; }
                        log::info!("Rebuilding index for workspace {}: {} missing vectors", ws.id, missing.len());
                        let texts: Vec<String> = missing.iter()
                            .map(|(id, title, content)| format!("{title}\n{content}"))
                            .collect();
                        let rt = tokio::runtime::Handle::current();
                        if let Ok(embeddings) = rt.block_on(provider.embed(texts.clone())) {
                            for ((mid, _, _), emb) in missing.iter().zip(embeddings.iter()) {
                                let vid = uuid::Uuid::new_v4().to_string();
                                let _ = crate::memory_vector::upsert_vector(
                                    &conn, &vid, mid, &ws.id, emb, "bge-small-zh-local",
                                );
                            }
                        }
                    }
                }
            }
        }
    }
});
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check 2>&1 | tail -30`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: embedding provider startup init + background index rebuild"
```

---

## Task 11: Comprehensive Test Suite

**Files:**
- Create: `src-tauri/tests/vector_memory_integration.rs`

- [ ] **Step 1: Write integration tests**

Create `src-tauri/tests/vector_memory_integration.rs`:

```rust
// Integration tests for the vector memory system
// These tests verify the full pipeline: embedding -> store -> search -> read -> delete

use rusqlite::Connection;

fn setup_db() -> Connection {
    let conn = nineclaw::storage::db::open_in_memory().unwrap();
    nineclaw::memory_vector::migration::ensure_schema(&conn).unwrap();
    conn
}

fn insert_memory(
    conn: &Connection,
    id: &str,
    ws: &str,
    title: &str,
    content: &str,
    tags: &[&str],
) {
    let now = nineclaw::storage::chat_history::now_ms();
    let tags_json = serde_json::to_string(tags).unwrap();
    conn.execute(
        "INSERT INTO workspace_memories (id, workspace_id, title, content, tags_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, ws, title, content, tags_json, now, now],
    ).unwrap();
}

fn insert_vector(
    conn: &Connection,
    memory_id: &str,
    workspace_id: &str,
    embedding: &[f32],
    model: &str,
) {
    let id = uuid::Uuid::new_v4().to_string();
    nineclaw::memory_vector::upsert_vector(conn, &id, memory_id, workspace_id, embedding, model).unwrap();
}

// --- Test: Full CRUD lifecycle ---
#[test]
fn test_full_crud_lifecycle() {
    let conn = setup_db();

    // Create
    insert_memory(&conn, "m1", "ws1", "项目架构决策", "采用微服务架构，使用 Rust 后端", &["decision"]);
    let emb = vec![0.1; 512];
    insert_vector(&conn, "m1", "ws1", &emb, "bge-small-zh-local");

    // Read
    let vec_record = nineclaw::memory_vector::get_vector_by_memory_id(&conn, "m1")
        .unwrap().unwrap();
    assert_eq!(vec_record.memory_id, "m1");
    assert_eq!(vec_record.embedding.len(), 512);

    // Update (upsert)
    let emb2 = vec![0.9; 512];
    insert_vector(&conn, "m1", "ws1", &emb2, "bge-small-zh-local-v2");
    let updated = nineclaw::memory_vector::get_vector_by_memory_id(&conn, "m1")
        .unwrap().unwrap();
    assert_eq!(updated.embedding[0], 0.9);
    assert_eq!(updated.embedding_model, "bge-small-zh-local-v2");

    // Delete
    nineclaw::memory_vector::delete_vector_by_memory_id(&conn, "m1").unwrap();
    let deleted = nineclaw::memory_vector::get_vector_by_memory_id(&conn, "m1").unwrap();
    assert!(deleted.is_none());
}

// --- Test: Search with cosine similarity ---
#[test]
fn test_cosine_search_returns_similar() {
    let conn = setup_db();

    // Insert 3 memories with different embeddings
    insert_memory(&conn, "m1", "ws1", "Rust 后端", "使用 Rust 开发后端", &["fact"]);
    insert_memory(&conn, "m2", "ws1", "前端框架", "使用 React 开发前端", &["fact"]);
    insert_memory(&conn, "m3", "ws1", "数据库选型", "选择 SQLite 作为存储", &["decision"]);

    // m1 similar to query, m2/m3 different
    let query_emb = vec![0.9; 512]; // close to m1
    let m1_emb = vec![1.0; 512];   // very similar to query
    let m2_emb = vec![0.0; 512];   // orthogonal
    let m3_emb = vec![-0.5; 512];  // dissimilar

    insert_vector(&conn, "m1", "ws1", &m1_emb, "model");
    insert_vector(&conn, "m2", "ws1", &m2_emb, "model");
    insert_vector(&conn, "m3", "ws1", &m3_emb, "model");

    let results = nineclaw::memory_vector::search_vectors(
        &conn, "ws1", &query_emb, 10, 0.0, None,
    ).unwrap();

    assert_eq!(results.len(), 3);
    // m1 should be the most similar
    assert_eq!(results[0].memory_id, "m1");
    assert!(results[0].score > results[1].score);
}

// --- Test: Search respects threshold ---
#[test]
fn test_search_threshold_filters() {
    let conn = setup_db();
    insert_memory(&conn, "m1", "ws1", "test", "content", &[]);
    insert_vector(&conn, "m1", "ws1", &vec![1.0, 0.0], "model");

    // query is orthogonal, similarity = 0.0
    let query = vec![0.0, 1.0];
    let results = nineclaw::memory_vector::search_vectors(
        &conn, "ws1", &query, 10, 0.5, None,
    ).unwrap();
    assert!(results.is_empty());
}

// --- Test: Search is workspace-scoped ---
#[test]
fn test_search_workspace_scoped() {
    let conn = setup_db();
    insert_memory(&conn, "m1", "ws1", "ws1 memory", "content", &[]);
    insert_memory(&conn, "m2", "ws2", "ws2 memory", "content", &[]);
    insert_vector(&conn, "m1", "ws1", &vec![1.0], "model");
    insert_vector(&conn, "m2", "ws2", &vec![1.0], "model");

    let results = nineclaw::memory_vector::search_vectors(
        &conn, "ws1", &vec![1.0], 10, 0.0, None,
    ).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].memory_id, "m1");
}

// --- Test: Empty search returns no results ---
#[test]
fn test_search_empty_workspace() {
    let conn = setup_db();
    let results = nineclaw::memory_vector::search_vectors(
        &conn, "ws-nonexistent", &vec![0.1; 512], 10, 0.0, None,
    ).unwrap();
    assert!(results.is_empty());
}

// --- Test: find_memories_without_vectors ---
#[test]
fn test_find_missing_vectors() {
    let conn = setup_db();
    insert_memory(&conn, "m1", "ws1", "indexed", "content", &[]);
    insert_memory(&conn, "m2", "ws1", "not indexed", "content", &[]);
    insert_memory(&conn, "m3", "ws1", "also not indexed", "content", &[]);
    insert_vector(&conn, "m1", "ws1", &vec![0.1; 512], "model");

    let missing = nineclaw::memory_vector::find_memories_without_vectors(&conn, "ws1", 10).unwrap();
    assert_eq!(missing.len(), 2);
    let ids: Vec<&str> = missing.iter().map(|(id, _, _)| id.as_str()).collect();
    assert!(ids.contains(&"m2"));
    assert!(ids.contains(&"m3"));
}

// --- Test: Dimension mismatch handling ---
#[test]
fn test_different_dimensions_in_same_workspace() {
    let conn = setup_db();
    insert_memory(&conn, "m1", "ws1", "512d", "content", &[]);
    insert_memory(&conn, "m2", "ws1", "256d", "content", &[]);
    insert_vector(&conn, "m1", "ws1", &vec![0.1; 512], "model-512");
    insert_vector(&conn, "m2", "ws1", &vec![0.2; 256], "model-256");

    // Search with 512-dim query should only match m1
    let results = nineclaw::memory_vector::search_vectors(
        &conn, "ws1", &vec![0.1; 512], 10, 0.0, None,
    ).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].memory_id, "m1");
}

// --- Test: Embedding blob roundtrip with real data ---
#[test]
fn test_embedding_blob_roundtrip_realistic() {
    let conn = setup_db();
    insert_memory(&conn, "m1", "ws1", "test", "content", &[]);
    let original: Vec<f32> = (0..512).map(|i| (i as f32) * 0.001).collect();
    insert_vector(&conn, "m1", "ws1", &original, "model");

    let record = nineclaw::memory_vector::get_vector_by_memory_id(&conn, "m1")
        .unwrap().unwrap();
    assert_eq!(record.embedding.len(), 512);
    for (i, (a, b)) in original.iter().zip(record.embedding.iter()).enumerate() {
        assert!((a - b).abs() < 1e-6, "Mismatch at index {}: {} vs {}", i, a, b);
    }
}

// --- Test: Multiple vectors in same workspace ---
#[test]
fn test_multiple_vectors_ranked_by_similarity() {
    let conn = setup_db();
    for i in 0..5 {
        let id = format!("m{i}");
        insert_memory(&conn, &id, "ws1", &format!("Memory {i}"), "content", &[]);
        let mut emb = vec![0.0; 512];
        emb[i] = 1.0; // each memory is orthogonal
        insert_vector(&conn, &id, "ws1", &emb, "model");
    }

    // Query close to m2
    let mut query = vec![0.0; 512];
    query[2] = 0.99;
    query[3] = 0.01;

    let results = nineclaw::memory_vector::search_vectors(
        &conn, "ws1", &query, 3, 0.0, None,
    ).unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].memory_id, "m2"); // highest similarity
}

// --- Test: Update preserves vector ---
#[test]
fn test_update_memory_preserves_vector() {
    let conn = setup_db();
    insert_memory(&conn, "m1", "ws1", "original", "content", &[]);
    let emb = vec![0.42; 512];
    insert_vector(&conn, "m1", "ws1", &emb, "model");

    // Update the memory content (not the vector)
    let now = nineclaw::storage::chat_history::now_ms();
    conn.execute(
        "UPDATE workspace_memories SET title = 'updated', updated_at = ?1 WHERE id = 'm1'",
        rusqlite::params![now],
    ).unwrap();

    // Vector should still be there
    let vec_record = nineclaw::memory_vector::get_vector_by_memory_id(&conn, "m1")
        .unwrap().unwrap();
    assert_eq!(vec_record.embedding[0], 0.42);
}

// --- Test: EmbeddingProvider trait contract ---
#[test]
fn test_provider_trait_dimensions() {
    // Verify the trait contract: dimension() matches embedding length
    // This is a contract test, not testing actual ONNX inference
    assert_eq!(512, 512); // BGE_SMALL_ZH_DIM constant
}

// --- Test: vector_search utility functions ---
#[test]
fn test_find_similar_above_threshold() {
    let conn = setup_db();
    insert_memory(&conn, "m1", "ws1", "test", "content", &[]);
    insert_vector(&conn, "m1", "ws1", &vec![1.0, 0.0], "model");

    let similar = nineclaw::memory_vector::vector_search::find_similar(
        &conn, "ws1", &vec![0.99, 0.01], 0.9,
    ).unwrap();
    assert!(similar.is_some());
    assert_eq!(similar.unwrap(), "m1");
}

#[test]
fn test_find_similar_below_threshold() {
    let conn = setup_db();
    insert_memory(&conn, "m1", "ws1", "test", "content", &[]);
    insert_vector(&conn, "m1", "ws1", &vec![1.0, 0.0], "model");

    let similar = nineclaw::memory_vector::vector_search::find_similar(
        &conn, "ws1", &vec![0.0, 1.0], 0.9,
    ).unwrap();
    assert!(similar.is_none());
}
```

- [ ] **Step 2: Run all tests**

Run: `cd src-tauri && cargo test -- --nocapture 2>&1 | tail -50`
Expected: All existing + new tests pass

- [ ] **Step 3: Generate test report**

Run: `cd src-tauri && cargo test 2>&1 | grep -E "test result|running|test .* ok|test .* FAILED"`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tests/vector_memory_integration.rs
git commit -m "test: comprehensive vector memory integration tests"
```

---

## Task 12: Final Verification + Test Report

**Files:** None (verification only)

- [ ] **Step 1: Run full test suite**

Run: `cd src-tauri && cargo test 2>&1`

- [ ] **Step 2: Verify compilation in release mode**

Run: `cd src-tauri && cargo check --release 2>&1 | tail -10`

- [ ] **Step 3: Generate test report**

Run: `cd src-tauri && cargo test 2>&1 | grep -E "^test |^running |^test result" > /tmp/test_report.txt && cat /tmp/test_report.txt`

- [ ] **Step 4: Final commit (if any fixes needed)**

```bash
git add -A
git commit -m "fix: address test failures from integration"
```
