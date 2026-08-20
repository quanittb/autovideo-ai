use super::error::CloudProviderError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub trait LiveExecutionPolicy: Send + Sync {
    fn is_paid_live_allowed(&self) -> bool;

    fn ensure_paid_live_allowed(&self) -> Result<(), CloudProviderError> {
        if !self.is_paid_live_allowed() {
            Err(CloudProviderError::ProviderUnavailable(
                "PAID_LIVE_TEST_DISABLED: Paid cloud provider execution is disabled (ALLOW_PAID_LIVE_TEST != 1). Live prediction creation and file uploads are blocked."
                    .to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Default, Clone)]
pub struct EnvLiveExecutionPolicy;

impl LiveExecutionPolicy for EnvLiveExecutionPolicy {
    fn is_paid_live_allowed(&self) -> bool {
        match std::env::var("ALLOW_PAID_LIVE_TEST") {
            Ok(v) => {
                let trimmed = v.trim().to_lowercase();
                trimmed == "1" || trimmed == "true"
            }
            Err(_) => false,
        }
    }
}

#[derive(Clone)]
pub struct MockLiveExecutionPolicy {
    allowed: Arc<AtomicBool>,
}

impl MockLiveExecutionPolicy {
    pub fn new(allowed: bool) -> Self {
        Self {
            allowed: Arc::new(AtomicBool::new(allowed)),
        }
    }

    pub fn set_allowed(&self, allowed: bool) {
        self.allowed.store(allowed, Ordering::SeqCst);
    }
}

impl Default for MockLiveExecutionPolicy {
    fn default() -> Self {
        Self::new(false)
    }
}

impl LiveExecutionPolicy for MockLiveExecutionPolicy {
    fn is_paid_live_allowed(&self) -> bool {
        self.allowed.load(Ordering::SeqCst)
    }
}

pub struct PaidLiveExecutionGuard;

impl PaidLiveExecutionGuard {
    pub fn is_paid_live_test_allowed() -> bool {
        EnvLiveExecutionPolicy.is_paid_live_allowed()
    }

    pub fn ensure_paid_execution_allowed() -> Result<(), CloudProviderError> {
        EnvLiveExecutionPolicy.ensure_paid_live_allowed()
    }
}
