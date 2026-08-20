use super::error::CloudProviderError;
use super::provider::CloudVideoProvider;
use super::providers::{PrunaPVideoReplaceProvider, ReplicateProvider};
use super::uploader::{ProviderAssetUploader, ReplicateAssetUploader};
use std::sync::Arc;

pub struct ResolvedProviderRuntime {
    pub provider: Arc<dyn CloudVideoProvider>,
    pub uploader: Arc<dyn ProviderAssetUploader>,
}

pub trait CloudProviderResolver: Send + Sync {
    fn resolve_provider(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Result<Arc<dyn CloudVideoProvider>, CloudProviderError>;

    fn resolve_runtime(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Result<ResolvedProviderRuntime, CloudProviderError> {
        let provider = self.resolve_provider(provider_id, model_id)?;
        let uploader: Arc<dyn ProviderAssetUploader> = Arc::new(ReplicateAssetUploader::new());
        Ok(ResolvedProviderRuntime { provider, uploader })
    }
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
        model_id: &str,
    ) -> Result<Arc<dyn CloudVideoProvider>, CloudProviderError> {
        match (provider_id, model_id) {
            ("replicate", "prunaai/p-video-replace") => {
                let provider = PrunaPVideoReplaceProvider::new();
                if !provider.is_configured() {
                    return Err(CloudProviderError::ProviderUnavailable(
                        "MISSING_PROVIDER_CREDENTIALS: REPLICATE_API_TOKEN environment variable is not configured".to_string(),
                    ));
                }
                Ok(Arc::new(provider))
            }
            ("replicate", "minimax/video-01") | ("replicate", "") => {
                let provider = ReplicateProvider::new();
                if !provider.is_configured() {
                    return Err(CloudProviderError::ProviderUnavailable(
                        "MISSING_PROVIDER_CREDENTIALS: REPLICATE_API_TOKEN environment variable is not configured".to_string(),
                    ));
                }
                Ok(Arc::new(provider))
            }
            (other_prov, other_mod) => Err(CloudProviderError::ProviderUnavailable(format!(
                "Provider '{}' (model '{}') not supported or missing executable adapter",
                other_prov, other_mod
            ))),
        }
    }
}
