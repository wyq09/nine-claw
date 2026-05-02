use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ndarray::{Array1, Array2, ArrayD};
use ort::session::Session;
use ort::value::Tensor;
use tokenizers::Tokenizer;

use super::EmbeddingProvider;

const PROVIDER_ID: &str = "bge-small-zh-local";
const DIMENSION: usize = 512;

/// Local ONNX embedding provider using bge-small-zh model.
///
/// Loads a quantized ONNX model + HuggingFace tokenizer from disk and runs
/// inference locally. Applies mean pooling over attention mask followed by
/// L2 normalization to produce unit-length embedding vectors.
#[derive(Debug)]
pub struct OnnxLocalProvider {
    session: Arc<Mutex<Session>>,
    tokenizer: Tokenizer,
}

impl OnnxLocalProvider {
    /// Create a new provider by loading the ONNX model and tokenizer from
    /// `model_dir`.
    ///
    /// Expects `model_dir` to contain `model.onnx` and `tokenizer.json`.
    pub fn new(model_dir: &Path) -> Result<Self, String> {
        let model_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !model_path.exists() {
            return Err(format!(
                "ONNX model not found at {}",
                model_path.display()
            ));
        }
        if !tokenizer_path.exists() {
            return Err(format!(
                "Tokenizer not found at {}",
                tokenizer_path.display()
            ));
        }

        let session = Session::builder()
            .map_err(|e| format!("Failed to create session builder: {}", e))?
            .commit_from_file(&model_path)
            .map_err(|e| format!("Failed to load ONNX model: {}", e))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            tokenizer,
        })
    }

    /// Compute embedding for a single text string via ONNX inference.
    ///
    /// Tokenizes input, runs the model, then applies mean pooling + L2 norm.
    fn embed_single(session: &Mutex<Session>, tokenizer: &Tokenizer, text: &str) -> Result<Vec<f32>, String> {
        let encoding = tokenizer
            .encode(text, true)
            .map_err(|e| format!("Tokenization failed: {}", e))?;

        let input_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();
        let type_ids_raw = encoding.get_type_ids();

        // BGE models expect token_type_ids (all zeros is fine for single-sentence)
        let type_ids: Vec<i64> = if type_ids_raw.is_empty() {
            vec![0i64; input_ids.len()]
        } else {
            type_ids_raw.iter().map(|&v| v as i64).collect()
        };

        let seq_len = input_ids.len();
        let input_ids_i64: Vec<i64> = input_ids.iter().map(|&v| v as i64).collect();
        let attention_mask_i64: Vec<i64> = attention_mask.iter().map(|&v| v as i64).collect();

        let input_ids_tensor =
            Tensor::from_array((vec![1i64, seq_len as i64], input_ids_i64))
                .map_err(|e| format!("Failed to create input_ids tensor: {}", e))?;
        let attention_mask_tensor = Tensor::from_array((
            vec![1i64, seq_len as i64],
            attention_mask_i64,
        ))
        .map_err(|e| format!("Failed to create attention_mask tensor: {}", e))?;
        let token_type_ids_tensor =
            Tensor::from_array((vec![1i64, seq_len as i64], type_ids))
                .map_err(|e| format!("Failed to create token_type_ids tensor: {}", e))?;

        let mut session_guard = session.lock().map_err(|e| {
            format!("Failed to lock ONNX session: {}", e)
        })?;

        let outputs = session_guard
            .run(ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
                "token_type_ids" => token_type_ids_tensor,
            ])
            .map_err(|e| format!("ONNX inference failed: {}", e))?;

        // Extract the last hidden state output (shape: [1, seq_len, hidden_dim])
        let output_value = &outputs[0];

        let output_array: ArrayD<f32> = output_value
            .try_extract_array::<f32>()
            .map_err(|e| format!("Failed to extract output tensor: {}", e))?
            .to_owned();

        // Reshape from ArrayD to Array2: [seq_len, hidden_dim]
        let shape = output_array.shape();
        if shape.len() != 3 || shape[0] != 1 {
            return Err(format!(
                "Unexpected output shape: {:?}, expected [1, seq_len, hidden]",
                shape
            ));
        }
        let seq = shape[1];
        let dim = shape[2];
        let token_embeddings = output_array
            .into_shape_with_order((seq, dim))
            .map_err(|e| format!("Failed to reshape output: {}", e))?;

        // Convert attention mask to f64 for mean pooling
        let mask_f64: Vec<f64> = attention_mask
            .iter()
            .map(|&v| v as f64)
            .collect();

        let pooled = Self::mean_pool(&token_embeddings, &mask_f64);

        Ok(Self::l2_normalize(&pooled))
    }

    /// Mean pooling: weighted average of token embeddings by attention mask.
    ///
    /// `token_embeddings` is [seq_len, hidden_dim], `attention_mask` is [seq_len].
    /// Returns [hidden_dim].
    pub fn mean_pool(token_embeddings: &Array2<f32>, attention_mask: &[f64]) -> Array1<f32> {
        let (seq_len, dim) = token_embeddings.dim();
        assert_eq!(
            attention_mask.len(),
            seq_len,
            "Attention mask length must match sequence length"
        );

        let mut sum = Array1::zeros(dim);
        let mut mask_sum: f64 = 0.0;

        for (i, &mask_val) in attention_mask.iter().enumerate() {
            if mask_val > 0.0 {
                mask_sum += mask_val;
                for j in 0..dim {
                    sum[j] += token_embeddings[[i, j]] * mask_val as f32;
                }
            }
        }

        if mask_sum > 0.0 {
            &sum / mask_sum as f32
        } else {
            sum
        }
    }

    /// L2 normalize a vector: divide by its Euclidean norm.
    pub fn l2_normalize(vec: &Array1<f32>) -> Vec<f32> {
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            vec.iter().map(|&v| v / norm).collect()
        } else {
            vec.to_vec()
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OnnxLocalProvider {
    fn id(&self) -> &str {
        PROVIDER_ID
    }

    fn dimension(&self) -> usize {
        DIMENSION
    }

    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        // Clone Arc and tokenizer for moving into spawn_blocking
        let session = Arc::clone(&self.session);
        let tokenizer = self.tokenizer.clone();
        let texts_clone = texts.clone();

        let result = tokio::task::spawn_blocking(move || {
            let mut embeddings = Vec::with_capacity(texts_clone.len());
            for text in &texts_clone {
                embeddings.push(Self::embed_single(&session, &tokenizer, text)?);
            }
            Ok::<Vec<Vec<f32>>, String>(embeddings)
        })
        .await
        .map_err(|e| format!("spawn_blocking panicked: {}", e))??;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    #[test]
    fn test_provider_id_and_dimension() {
        // These are constants, just verify them directly
        assert_eq!(PROVIDER_ID, "bge-small-zh-local");
        assert_eq!(DIMENSION, 512);
    }

    #[test]
    fn test_mean_pool_single_token() {
        // Single token embedding [3.0, 4.0], mask [1]
        // Mean pool with single masked token = the embedding itself
        // Then L2 normalize: norm = 5.0, result = [0.6, 0.8]
        let token_embeddings = array![[3.0_f32, 4.0_f32]];
        let mask = vec![1.0_f64];
        let pooled = OnnxLocalProvider::mean_pool(&token_embeddings, &mask);

        assert!((pooled[0] - 3.0).abs() < 1e-6);
        assert!((pooled[1] - 4.0).abs() < 1e-6);

        // Now L2 normalize
        let normalized = OnnxLocalProvider::l2_normalize(&pooled);
        assert!((normalized[0] - 0.6).abs() < 1e-6);
        assert!((normalized[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_mean_pool_two_tokens() {
        // Two token embeddings [[1, 0], [0, 1]], mask [1, 1]
        // Mean pool: [0.5, 0.5]
        // L2 normalize: norm = sqrt(0.5), result = [1/sqrt(2), 1/sqrt(2)]
        let token_embeddings = array![[1.0_f32, 0.0_f32], [0.0_f32, 1.0_f32]];
        let mask = vec![1.0_f64, 1.0_f64];
        let pooled = OnnxLocalProvider::mean_pool(&token_embeddings, &mask);

        assert!((pooled[0] - 0.5).abs() < 1e-6);
        assert!((pooled[1] - 0.5).abs() < 1e-6);

        let normalized = OnnxLocalProvider::l2_normalize(&pooled);
        let expected = 1.0 / 2.0_f32.sqrt();
        assert!((normalized[0] - expected).abs() < 1e-6);
        assert!((normalized[1] - expected).abs() < 1e-6);
    }

    #[test]
    fn test_mean_pool_zero_mask() {
        // All-zero mask should produce all-zero pooled output
        let token_embeddings = array![[1.0_f32, 2.0_f32], [3.0_f32, 4.0_f32]];
        let mask = vec![0.0_f64, 0.0_f64];
        let pooled = OnnxLocalProvider::mean_pool(&token_embeddings, &mask);

        assert_eq!(pooled[0], 0.0);
        assert_eq!(pooled[1], 0.0);
    }

    #[test]
    fn test_new_fails_without_model() {
        let result = OnnxLocalProvider::new(Path::new("/nonexistent/path/that/does/not/exist"));
        match result {
            Err(msg) => {
                assert!(
                    msg.contains("not found"),
                    "Expected 'not found' in error, got: {}",
                    msg
                );
            }
            Ok(_) => panic!("Expected error when model dir doesn't exist"),
        }
    }
}
