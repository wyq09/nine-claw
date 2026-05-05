//! 向量搜索工具函数：blob 序列化、余弦相似度、暴力搜索。

use rusqlite::{params, Connection};

/// 将 f32 切片转为小端字节 blob。
pub fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(embedding.len() * 4);
    for &v in embedding {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

/// 将 blob 还原为 Vec<f32>。长度必须是 4 的倍数。
pub fn blob_to_embedding(blob: &[u8]) -> Result<Vec<f32>, String> {
    if blob.len() % 4 != 0 {
        return Err(format!(
            "blob 长度 {} 不是 4 的倍数，无法还原为 f32 数组",
            blob.len()
        ));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|chunk| {
            let bytes: [u8; 4] = chunk.try_into().unwrap();
            f32::from_le_bytes(bytes)
        })
        .collect())
}

/// 计算两个向量的余弦相似度。
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        return 0.0;
    }
    dot / denom
}

/// 搜索结果条目。
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub memory_id: String,
    pub score: f32,
    pub embedding_model: String,
    pub dimension: usize,
    pub updated_at: i64,
    pub metadata_json: Option<String>,
    pub content_text: Option<String>,
}

/// 在指定工作空间内暴力余弦搜索。
///
/// - `tag_filter`: 若提供，则只返回 workspace_memories.tags_json 包含全部指定 tag 的记录。
/// - `scope_filter`: 若提供，则只返回 scope 在指定列表中的记录（如 `["system", "workspace"]`）。
///   若为 None，则搜索所有 scope（向后兼容）。
/// - `scope_agent_id`: 若提供，则 agent scope 的记忆只返回属于该 agent 的记录。
///   同时也始终包含非 agent scope（system/workspace）的记忆。
pub fn cosine_search(
    conn: &Connection,
    workspace_id: &str,
    query_embedding: &[f32],
    limit: usize,
    threshold: f32,
    tag_filter: Option<&[String]>,
    scope_filter: Option<&[String]>,
    scope_agent_id: Option<&str>,
) -> Result<Vec<SearchHit>, String> {
    // Always JOIN with workspace_memories for scope filtering
    let mut sql = String::from(
        "SELECT mv.memory_id, mv.embedding, mv.embedding_model, mv.dimension, mv.updated_at,
                mv.metadata_json, mv.content_text
         FROM memory_vectors mv
         JOIN workspace_memories wm ON wm.id = mv.memory_id
         WHERE (mv.workspace_id = ?1 OR wm.scope = 'system')",
    );

    // Scope filter: restrict to specified scopes
    if let Some(scopes) = scope_filter {
        if !scopes.is_empty() {
            let placeholders: Vec<&str> = scopes.iter().map(|_| "?").collect();
            sql.push_str(&format!(" AND wm.scope IN ({})", placeholders.join(",")));
        }
    }

    // Agent scope filter: agent-scoped memories only visible to their owner (or supervisor if scope_agent_id is None)
    if let Some(agent_id) = scope_agent_id {
        sql.push_str(&format!(
            " AND (wm.scope != 'agent' OR wm.scope_agent_id = '{}')",
            agent_id.replace('\'', "''")
        ));
    }

    // Build params: workspace_id first, then scope filter values
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(workspace_id.to_string())];
    if let Some(scopes) = scope_filter {
        for s in scopes {
            param_values.push(Box::new(s.clone()));
        }
    }

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("准备向量搜索失败: {e}"))?;

    let rows: Vec<(
        String,
        Vec<u8>,
        String,
        i32,
        i64,
        Option<String>,
        Option<String>,
    )> = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|e| format!("执行向量搜索失败: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    let mut hits: Vec<SearchHit> = Vec::new();

    for (memory_id, blob, embedding_model, dim, updated_at, metadata_json, content_text) in rows {
        // 若提供 tag_filter，在 Rust 侧检查 tags_json
        if let Some(tags) = tag_filter {
            let tags_json: String = conn
                .query_row(
                    "SELECT tags_json FROM workspace_memories WHERE id = ?1",
                    params![memory_id],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| "[]".to_string());

            let memory_tags: Vec<String> =
                serde_json::from_str(&tags_json).unwrap_or_else(|_| Vec::new());
            let all_match = tags.iter().all(|t| memory_tags.contains(t));
            if !all_match {
                continue;
            }
        }

        let vec = blob_to_embedding(&blob)?;
        let score = cosine_similarity(query_embedding, &vec);
        if score >= threshold {
            hits.push(SearchHit {
                memory_id,
                score,
                embedding_model,
                dimension: dim as usize,
                updated_at,
                metadata_json,
                content_text,
            });
        }
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);
    Ok(hits)
}

/// K/V 记忆向量：`memory_id` 形如 `kv::<key>`，不参与 `workspace_memories` JOIN。
pub fn find_similar_kv(
    conn: &Connection,
    workspace_id: &str,
    query_embedding: &[f32],
    threshold: f32,
) -> Result<Option<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT memory_id, embedding FROM memory_vectors
             WHERE workspace_id = ?1 AND memory_id LIKE 'kv::%' AND embedding IS NOT NULL",
        )
        .map_err(|e| format!("准备 K/V 向量查询失败: {e}"))?;
    let rows = stmt
        .query_map(params![workspace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(|e| format!("执行 K/V 向量查询失败: {e}"))?;

    let mut best_id: Option<String> = None;
    let mut best_score: f32 = 0.0;
    for row in rows {
        let Ok((memory_id, blob)) = row else {
            continue;
        };
        let vec = blob_to_embedding(&blob)?;
        let score = cosine_similarity(query_embedding, &vec);
        if score > best_score && score >= threshold {
            best_score = score;
            best_id = Some(memory_id);
        }
    }
    Ok(best_id)
}

/// 在工作空间中查找与给定嵌入最相似的记忆 ID（仅 `workspace_memories`）。若最高分低于 threshold 则返回 None。
pub fn find_similar(
    conn: &Connection,
    workspace_id: &str,
    embedding: &[f32],
    threshold: f32,
) -> Result<Option<String>, String> {
    let hits = cosine_search(
        conn,
        workspace_id,
        embedding,
        1,
        threshold,
        None,
        None,
        None,
    )?;
    Ok(hits.into_iter().next().map(|h| h.memory_id))
}

/// 在 memory_vectors 中搜索不依赖 workspace_memories JOIN 的独立向量（chat turns, saved memories 等）。
/// 按前缀过滤 memory_id，支持时间范围过滤（通过 metadata_json 中的 timestamp）。
pub fn standalone_vector_search(
    conn: &Connection,
    workspace_id: &str,
    query_embedding: &[f32],
    limit: usize,
    threshold: f32,
    memory_id_prefix: &str,
    time_range_start: Option<i64>,
    time_range_end: Option<i64>,
    tag_filter: Option<&[String]>,
) -> Result<Vec<SearchHit>, String> {
    let mut sql = format!(
        "SELECT mv.memory_id, mv.embedding, mv.embedding_model, mv.dimension, mv.updated_at,
                mv.metadata_json, mv.content_text
         FROM memory_vectors mv
         WHERE mv.workspace_id = ?1 AND mv.memory_id LIKE '{}%'",
        memory_id_prefix.replace('\'', "''")
    );

    if time_range_start.is_some() || time_range_end.is_some() {
        // Filter by created_at in metadata_json — uses content_text column as a proxy if no metadata
        // For chat turns we store timestamp in metadata_json
        if let Some(start) = time_range_start {
            sql.push_str(&format!(" AND mv.updated_at >= {}", start));
        }
        if let Some(end) = time_range_end {
            sql.push_str(&format!(" AND mv.updated_at <= {}", end));
        }
    }

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("准备独立向量搜索失败: {e}"))?;

    let rows: Vec<(
        String,
        Vec<u8>,
        String,
        i32,
        i64,
        Option<String>,
        Option<String>,
    )> = stmt
        .query_map(params![workspace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|e| format!("执行独立向量搜索失败: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    let mut hits: Vec<SearchHit> = Vec::new();
    for (memory_id, blob, embedding_model, dim, updated_at, metadata_json, content_text) in rows {
        // Tag filter: check tags_json in metadata
        if let Some(tags) = tag_filter {
            let row_tags: Vec<String> = if let Some(ref tj) = metadata_json {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(tj) {
                    meta.get("tags")
                        .and_then(|t| {
                            serde_json::from_str::<Vec<String>>(
                                &serde_json::to_string(t).unwrap_or_default(),
                            )
                            .ok()
                        })
                        .unwrap_or_default()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            if !tags.iter().all(|t| row_tags.contains(t)) {
                continue;
            }
        }

        let vec = blob_to_embedding(&blob)?;
        let score = cosine_similarity(query_embedding, &vec);
        if score >= threshold {
            hits.push(SearchHit {
                memory_id,
                score,
                embedding_model,
                dimension: dim as usize,
                updated_at,
                metadata_json,
                content_text,
            });
        }
    }

    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);
    Ok(hits)
}

/// Search saved memories (memory_id prefix "saved::") across workspaces.
pub fn saved_memory_search(
    conn: &Connection,
    workspace_ids: &[String],
    query_embedding: &[f32],
    limit: usize,
    threshold: f32,
    tag_filter: Option<&[String]>,
) -> Result<Vec<SearchHit>, String> {
    let mut merged = Vec::new();
    for ws in workspace_ids {
        let mut hits = standalone_vector_search(
            conn,
            ws,
            query_embedding,
            limit,
            threshold,
            "saved::",
            None,
            None,
            tag_filter,
        )?;
        merged.append(&mut hits);
    }
    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut dedup = std::collections::HashSet::new();
    let mut out = Vec::new();
    for hit in merged {
        if dedup.insert(hit.memory_id.clone()) {
            out.push(hit);
        }
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_blob_roundtrip() {
        let embedding: Vec<f32> = vec![0.1, -0.2, 0.3, 1.0, -1.0, 0.0];
        let blob = embedding_to_blob(&embedding);
        assert_eq!(blob.len(), embedding.len() * 4);
        let restored = blob_to_embedding(&blob).unwrap();
        assert_eq!(restored.len(), embedding.len());
        for (a, b) in embedding.iter().zip(restored.iter()) {
            assert!(float_eq(*a, *b));
        }
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!(
            float_eq(sim, 1.0),
            "identical vectors should have similarity 1.0, got {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            float_eq(sim, 0.0),
            "orthogonal vectors should have similarity 0.0, got {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            float_eq(sim, -1.0),
            "opposite vectors should have similarity -1.0, got {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            float_eq(sim, 0.0),
            "zero vector should yield 0.0, got {sim}"
        );
    }

    #[test]
    fn test_blob_to_embedding_invalid_length() {
        let blob = vec![0u8, 1, 2]; // 3 bytes — not divisible by 4
        let result = blob_to_embedding(&blob);
        assert!(result.is_err(), "should fail for non-multiple-of-4 length");
    }

    #[test]
    fn test_cosine_similarity_known_angle() {
        // cos(45°) = sqrt(2)/2 ≈ 0.7071
        let angle = std::f32::consts::FRAC_PI_4;
        let a = vec![1.0, 0.0];
        let b = vec![angle.cos(), angle.sin()];
        let sim = cosine_similarity(&a, &b);
        let expected = 2f32.sqrt() / 2.0;
        assert!(
            (sim - expected).abs() < 1e-5,
            "cos(45°) should be ≈ {expected}, got {sim}"
        );
    }

    fn float_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }
}
