use super::EmbeddingProvider;
use serde::Deserialize;

#[derive(Clone)]
pub struct RemoteApiConfig {
    pub id: String,
    #[allow(dead_code)]
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
    pub fn new(config: RemoteApiConfig, client: reqwest::Client) -> Self {
        Self { config, client }
    }

    #[allow(dead_code)]
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
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.config.api_key),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("embedding request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("embedding API returned {}: {}", status, text));
        }

        let parsed: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| format!("failed to parse embedding response: {}", e))?;

        let target_dim = self.config.dimension;
        let results: Vec<Vec<f32>> = parsed
            .data
            .into_iter()
            .map(|d| {
                let mut vec = d.embedding;
                vec.truncate(target_dim);
                if vec.len() < target_dim {
                    vec.resize(target_dim, 0.0);
                }
                vec
            })
            .collect();

        if results.len() != texts.len() {
            return Err(format!(
                "embedding count mismatch: sent {} texts, got {} embeddings",
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

    fn make_config() -> RemoteApiConfig {
        RemoteApiConfig {
            id: "test-remote".to_string(),
            name: "Test Remote API".to_string(),
            endpoint: "https://httpbin.org/post".to_string(),
            model_name: "text-embedding-3-small".to_string(),
            api_key: "sk-test-key".to_string(),
            dimension: 1536,
        }
    }

    #[test]
    fn test_provider_id() {
        let provider = RemoteApiProvider::new(make_config(), reqwest::Client::new());
        assert_eq!(provider.id(), "test-remote");
    }

    #[test]
    fn test_provider_dimension() {
        let provider = RemoteApiProvider::new(make_config(), reqwest::Client::new());
        assert_eq!(provider.dimension(), 1536);
    }

    #[test]
    fn test_config_accessible() {
        let provider = RemoteApiProvider::new(make_config(), reqwest::Client::new());
        let cfg = provider.config();
        assert_eq!(cfg.id, "test-remote");
        assert_eq!(cfg.name, "Test Remote API");
        assert_eq!(cfg.model_name, "text-embedding-3-small");
    }

    #[tokio::test]
    async fn test_embed_fails_on_bad_endpoint() {
        let mut config = make_config();
        config.endpoint = "https://example.invalid/embeddings".to_string();
        let provider = RemoteApiProvider::new(config, reqwest::Client::new());

        let result = provider.embed(vec!["hello".to_string()]).await;
        assert!(result.is_err(), "expected embed to fail on bad endpoint");
        let err = result.unwrap_err();
        assert!(
            err.contains("embedding request failed"),
            "error should mention request failure: {}",
            err
        );
    }

    #[test]
    fn test_config_clone() {
        let config = make_config();
        let cloned = config.clone();
        assert_eq!(config.id, cloned.id);
        assert_eq!(config.name, cloned.name);
        assert_eq!(config.endpoint, cloned.endpoint);
        assert_eq!(config.model_name, cloned.model_name);
        assert_eq!(config.api_key, cloned.api_key);
        assert_eq!(config.dimension, cloned.dimension);
    }
}
