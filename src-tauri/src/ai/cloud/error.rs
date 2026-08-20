use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CloudProviderError {
    ProviderUnavailable(String),
    AuthFailed(String),
    RequestInvalid(String),
    RateLimited(String),
    Timeout(String),
    JobFailed(String),
    DownloadFailed(String),
    OutputInvalid(String),
    CostLimitExceeded { estimated: f64, limit: f64 },
    NetworkError(String),
    SecurityViolation(String),
    ProtocolViolation(String),
    Other(String),
}

impl std::fmt::Display for CloudProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProviderUnavailable(s) => write!(f, "CLOUD_PROVIDER_UNAVAILABLE: {}", s),
            Self::AuthFailed(s) => write!(f, "CLOUD_AUTH_FAILED: {}", s),
            Self::RequestInvalid(s) => write!(f, "CLOUD_REQUEST_INVALID: {}", s),
            Self::RateLimited(s) => write!(f, "CLOUD_RATE_LIMITED: {}", s),
            Self::Timeout(s) => write!(f, "CLOUD_TIMEOUT: {}", s),
            Self::JobFailed(s) => write!(f, "CLOUD_JOB_FAILED: {}", s),
            Self::DownloadFailed(s) => write!(f, "CLOUD_DOWNLOAD_FAILED: {}", s),
            Self::OutputInvalid(s) => write!(f, "CLOUD_OUTPUT_INVALID: {}", s),
            Self::CostLimitExceeded { estimated, limit } => write!(
                f,
                "CLOUD_COST_LIMIT_EXCEEDED: estimated ${:.2} exceeds limit ${:.2}",
                estimated, limit
            ),
            Self::NetworkError(s) => write!(f, "CLOUD_NETWORK_ERROR: {}", s),
            Self::SecurityViolation(s) => write!(f, "CLOUD_SECURITY_VIOLATION: {}", s),
            Self::ProtocolViolation(s) => write!(f, "CLOUD_PROTOCOL_VIOLATION: {}", s),
            Self::Other(s) => write!(f, "CLOUD_ERROR: {}", s),
        }
    }
}

impl std::error::Error for CloudProviderError {}
