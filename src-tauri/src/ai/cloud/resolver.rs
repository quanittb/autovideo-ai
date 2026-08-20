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
            "replicate" => {
                let provider = ReplicateProvider::new();
                if !provider.is_configured() {
                    return Err(CloudProviderError::ProviderUnavailable(
                        "MISSING_PROVIDER_CREDENTIALS: REPLICATE_API_TOKEN environment variable is not configured".to_string(),
                    ));
                }
                Ok(Arc::new(provider))
            }
            other => Err(CloudProviderError::ProviderUnavailable(format!(
                "Provider '{}' not supported or missing executable adapter",
                other
            ))),
        }
    }
}
