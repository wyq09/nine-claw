pub mod onnx_local;
pub mod remote_api;

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

    impl MockProvider {
        fn new(id: &str, dim: usize) -> Self {
            Self {
                id: id.to_string(),
                dim,
            }
        }
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for MockProvider {
        fn id(&self) -> &str {
            &self.id
        }

        fn dimension(&self) -> usize {
            self.dim
        }

        async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
            Ok(texts.iter().map(|_| vec![0.1; self.dim]).collect())
        }
    }

    #[tokio::test]
    async fn test_registry_register_and_get() {
        let mut registry = ProviderRegistry::new("mock");
        let provider = Arc::new(MockProvider::new("mock", 128));
        registry.register(provider);

        let got = registry.get("mock").expect("provider should be found");
        assert_eq!(got.id(), "mock");
        assert_eq!(got.dimension(), 128);
    }

    #[tokio::test]
    async fn test_registry_default_provider() {
        let mut registry = ProviderRegistry::new("mock");
        registry.register(Arc::new(MockProvider::new("mock", 64)));

        let default = registry.default_provider().expect("default should exist");
        assert_eq!(default.id(), "mock");
    }

    #[tokio::test]
    async fn test_registry_default_missing_returns_none() {
        let registry = ProviderRegistry::new("nonexistent");

        assert!(registry.default_provider().is_none());
    }

    #[tokio::test]
    async fn test_registry_list_ids() {
        let mut registry = ProviderRegistry::new("a");
        registry.register(Arc::new(MockProvider::new("alpha", 32)));
        registry.register(Arc::new(MockProvider::new("beta", 64)));

        let mut ids = registry.list_ids();
        ids.sort();
        assert_eq!(ids, vec!["alpha", "beta"]);
    }

    #[tokio::test]
    async fn test_mock_provider_embed() {
        let provider = MockProvider::new("test", 4);
        let result = provider
            .embed(vec!["hello".to_string(), "world".to_string()])
            .await
            .expect("embed should succeed");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], vec![0.1; 4]);
        assert_eq!(result[1], vec![0.1; 4]);
    }

    #[tokio::test]
    async fn test_registry_overwrite_on_reregister() {
        let mut registry = ProviderRegistry::new("mock");

        registry.register(Arc::new(MockProvider::new("mock", 64)));
        assert_eq!(
            registry.get("mock").unwrap().dimension(),
            64
        );

        // Re-register with different dimension
        registry.register(Arc::new(MockProvider::new("mock", 256)));
        assert_eq!(
            registry.get("mock").unwrap().dimension(),
            256
        );
    }
}
