use super::error::CloudProviderError;
use super::provider::CloudVideoProvider;
use super::providers::ReplicateProvider;
use std::sync::Arc;

pub trait CloudProviderResolver: Send + Sync {
    fn resolve_provider(
        &self,
        provider_id: &str,
    ) -> Result<Arc<dyn CloudVideoProvider>, CloudProviderError>;
}

pub struct DefaultCloudProviderResolver;

impl DefaultCloudProviderResolver {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultCloudProviderResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl CloudProviderResolver for DefaultCloudProviderResolver {
    fn resolve_provider(
        &self,
        provider_id: &str,
    ) -> Result<Arc<dyn CloudVideoProvider>, CloudProviderError> {
        match provider_id {
            "replicate" => Ok(Arc::new(ReplicateProvider::new())),
            other => Err(CloudProviderError::ProviderUnavailable(format!(
                "Provider '{}' not supported or missing executable adapter",
                other
            ))),
        }
    }
}
