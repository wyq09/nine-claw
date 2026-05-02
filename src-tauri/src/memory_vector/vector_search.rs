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
}

/// 在指定工作空间内暴力余弦搜索。
///
/// - `tag_filter`: 若提供，则只返回 workspace_memories.tags_json 包含全部指定 tag 的记录。
pub fn cosine_search(
    conn: &Connection,
    workspace_id: &str,
    query_embedding: &[f32],
    limit: usize,
    threshold: f32,
    tag_filter: Option<&[String]>,
) -> Result<Vec<SearchHit>, String> {
    let sql = if tag_filter.is_some() {
        "SELECT mv.memory_id, mv.embedding, mv.embedding_model, mv.dimension, mv.updated_at
         FROM memory_vectors mv
         JOIN workspace_memories wm ON wm.id = mv.memory_id
         WHERE mv.workspace_id = ?1"
    } else {
        "SELECT mv.memory_id, mv.embedding, mv.embedding_model, mv.dimension, mv.updated_at
         FROM memory_vectors mv
         WHERE mv.workspace_id = ?1"
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("准备向量搜索失败: {e}"))?;

    let rows: Vec<(String, Vec<u8>, String, i32, i64)> = stmt
        .query_map(params![workspace_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|e| format!("执行向量搜索失败: {e}"))?
        .filter_map(|r| r.ok())
        .collect();

    let mut hits: Vec<SearchHit> = Vec::new();

    for (memory_id, blob, embedding_model, dim, updated_at) in rows {
        // 若提供 tag_filter，在 Rust 侧检查 tags_json
        if let Some(tags) = tag_filter {
            let tags_json: String = conn
                .query_row(
                    "SELECT tags_json FROM workspace_memories WHERE id = ?1",
                    params![memory_id],
                    |row| row.get(0),
                )
                .unwrap_or_else(|_| "[]".to_string());

            let memory_tags: Vec<String> = serde_json::from_str(&tags_json)
                .unwrap_or_else(|_| Vec::new());
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
            });
        }
    }

    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit);
    Ok(hits)
}

/// 在工作空间中查找与给定嵌入最相似的记忆 ID。若最高分低于 threshold 则返回 None。
pub fn find_similar(
    conn: &Connection,
    workspace_id: &str,
    embedding: &[f32],
    threshold: f32,
) -> Result<Option<String>, String> {
    let hits = cosine_search(conn, workspace_id, embedding, 1, threshold, None)?;
    Ok(hits.into_iter().next().map(|h| h.memory_id))
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
        assert!(float_eq(sim, 1.0), "identical vectors should have similarity 1.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(float_eq(sim, 0.0), "orthogonal vectors should have similarity 0.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(float_eq(sim, -1.0), "opposite vectors should have similarity -1.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!(float_eq(sim, 0.0), "zero vector should yield 0.0, got {sim}");
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
