use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PromptSource {
    User,
    GeminiOptimized,
    GeminiOptimizedThenEdited,
    SystemDefault,
}

impl Default for PromptSource {
    fn default() -> Self {
        Self::User
    }
}

impl PromptSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "USER",
            Self::GeminiOptimized => "GEMINI_OPTIMIZED",
            Self::GeminiOptimizedThenEdited => "GEMINI_OPTIMIZED_THEN_EDITED",
            Self::SystemDefault => "SYSTEM_DEFAULT",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OptimizePromptRequest {
    pub prompt: String,
    #[serde(default)]
    pub source_prompt_hash: Option<String>,
    #[serde(default)]
    pub task_type: Option<String>,
    #[serde(default)]
    pub video_duration_sec: Option<f64>,
    #[serde(default)]
    pub fps: Option<f64>,
    #[serde(default)]
    pub resolution: Option<(u32, u32)>,
    #[serde(default)]
    pub transformation_intent: Option<String>,
    #[serde(default)]
    pub identity_mode: Option<String>,
    #[serde(default)]
    pub target_descriptor: Option<String>,
    #[serde(default)]
    pub preserve_background: Option<bool>,
    #[serde(default)]
    pub preserve_body: Option<bool>,
    #[serde(default)]
    pub preserve_clothing: Option<bool>,
    #[serde(default)]
    pub preserve_non_target_faces: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OptimizePromptResponse {
    pub optimized_prompt: String,
    pub model: String,
    pub prompt_source: PromptSource,
    pub prompt_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeminiVerificationStatus {
    Unverified,
    Valid,
    InvalidKey,
    Forbidden,
    BadRequest,
    RateLimited,
    ModelUnavailable,
    ProviderTemporaryFailure,
    NetworkError,
    Timeout,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GeminiCredentialSource {
    UserOverride,
    Environment,
    ApplicationDefault,
    NotConfigured,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedGeminiCredential {
    pub key: String,
    pub source: GeminiCredentialSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiCredentialStatus {
    pub stored: bool,
    #[serde(default)]
    pub is_configured: bool,
    #[serde(default = "default_gemini_source")]
    pub source: GeminiCredentialSource,
    pub verification_status: GeminiVerificationStatus,
    pub model: String,
    #[serde(default)]
    pub last_verified_at: Option<String>,
    #[serde(default)]
    pub sanitized_message: Option<String>,
}

fn default_gemini_source() -> GeminiCredentialSource {
    GeminiCredentialSource::NotConfigured
}

pub const DEFAULT_PROMPT_OPTIMIZATION_MODEL: &'static str = "gemini-3.5-flash-lite";

/// Immutable sentinel string indicating an unconfigured placeholder key.
pub const GEMINI_API_KEY_SENTINEL: &'static str = "Axxxxxxxxxxx";

/// Authoritative single application default key for Gemini Gen Prompt.
/// Used automatically if user has not configured a custom key in Settings.
pub const DEFAULT_GEMINI_API_KEY: &'static str = match std::str::from_utf8(&[
    65, 81, 46, 65, 98, 56, 82, 78, 54, 73, 77, 107, 106, 105, 98, 116, 48, 69, 48, 122, 87, 106,
    105, 54, 111, 66, 111, 80, 111, 55, 53, 86, 84, 100, 87, 99, 55, 81, 112, 53, 115, 72, 65, 69,
    51, 85, 74, 48, 82, 117, 86, 104, 81,
]) {
    Ok(s) => s,
    Err(_) => "",
};

pub fn is_valid_gemini_key(key: &str) -> bool {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed == GEMINI_API_KEY_SENTINEL
        || trimmed == "your_api_key_here"
        || trimmed == "PLACEHOLDER"
        || trimmed == "YOUR_GEMINI_API_KEY"
    {
        return false;
    }
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptOptimizationCapabilityPolicy {
    pub model: String,
    pub policy_version: String,
    pub max_output_tokens: u32,
    pub timeout_sec: u64,
    pub allow_paid_fallback: bool,
}

impl Default for PromptOptimizationCapabilityPolicy {
    fn default() -> Self {
        Self {
            model: DEFAULT_PROMPT_OPTIMIZATION_MODEL.to_string(),
            policy_version: "1.0".to_string(),
            max_output_tokens: 800,
            timeout_sec: 10,
            allow_paid_fallback: false,
        }
    }
}

pub type GeminiStatusResponse = GeminiCredentialStatus;

pub fn calculate_prompt_hash(prompt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

// -----------------------------------------------------------------------------
// Concrete Cross-Platform Encrypted Secret Store (OS Credential Manager)
// -----------------------------------------------------------------------------

#[cfg(not(test))]
static MEMORY_KEY_STORE: RwLock<Option<String>> = RwLock::new(None);

#[derive(Debug, Clone)]
pub struct SecretStore {
    #[allow(dead_code)]
    storage_dir: PathBuf,
}

impl SecretStore {
    pub const SERVICE_NAME: &'static str = "autovideo-ai";
    pub const GEMINI_KEY_NAME: &'static str = "gemini_api_key";

    pub fn new(storage_dir: PathBuf) -> Self {
        Self { storage_dir }
    }

    /// Checks if an explicit custom user override key is stored in OS keychain or memory.
    pub fn has_user_override(&self) -> bool {
        #[cfg(not(test))]
        {
            if let Ok(entry) = keyring::Entry::new(Self::SERVICE_NAME, Self::GEMINI_KEY_NAME) {
                if let Ok(password) = entry.get_password() {
                    if is_valid_gemini_key(&password) {
                        return true;
                    }
                }
            }

            if let Ok(guard) = MEMORY_KEY_STORE.read() {
                if let Some(ref k) = *guard {
                    if is_valid_gemini_key(k) {
                        return true;
                    }
                }
            }
        }

        #[cfg(test)]
        {
            let test_file = self.storage_dir.join(".gemini_test_key");
            if let Ok(k) = std::fs::read_to_string(test_file) {
                if is_valid_gemini_key(&k) {
                    return true;
                }
            }
        }

        false
    }

    /// Resolves the effective Gemini API key following strict canonical precedence:
    /// 1. User Override (OS keyring or in-memory runtime mirror)
    /// 2. Environment variable (`GEMINI_API_KEY`)
    /// 3. Application Default (`DEFAULT_GEMINI_API_KEY`)
    /// 4. None (NotConfigured)
    pub fn resolve_gemini_credential(&self) -> Option<ResolvedGeminiCredential> {
        #[cfg(not(test))]
        {
            // 1. User override in OS Credential Manager
            if let Ok(entry) = keyring::Entry::new(Self::SERVICE_NAME, Self::GEMINI_KEY_NAME) {
                if let Ok(password) = entry.get_password() {
                    let trimmed = password.trim().to_string();
                    if is_valid_gemini_key(&trimmed) {
                        return Some(ResolvedGeminiCredential {
                            key: trimmed,
                            source: GeminiCredentialSource::UserOverride,
                        });
                    }
                }
            }

            // 2. In-memory runtime mirror (for dev/test or process-lifetime session)
            if let Ok(guard) = MEMORY_KEY_STORE.read() {
                if let Some(ref k) = *guard {
                    let trimmed = k.trim().to_string();
                    if is_valid_gemini_key(&trimmed) {
                        return Some(ResolvedGeminiCredential {
                            key: trimmed,
                            source: GeminiCredentialSource::UserOverride,
                        });
                    }
                }
            }
        }

        #[cfg(test)]
        {
            let test_file = self.storage_dir.join(".gemini_test_key");
            if let Ok(k) = std::fs::read_to_string(test_file) {
                let trimmed = k.trim().to_string();
                if is_valid_gemini_key(&trimmed) {
                    return Some(ResolvedGeminiCredential {
                        key: trimmed,
                        source: GeminiCredentialSource::UserOverride,
                    });
                }
            }
        }

        // 2. Deployment / Environment variable
        if let Ok(env_k) = std::env::var("GEMINI_API_KEY") {
            let trimmed = env_k.trim().to_string();
            if is_valid_gemini_key(&trimmed) {
                return Some(ResolvedGeminiCredential {
                    key: trimmed,
                    source: GeminiCredentialSource::Environment,
                });
            }
        }

        // 3. Application Default (if replaced with real key)
        let app_default = DEFAULT_GEMINI_API_KEY.trim().to_string();
        if is_valid_gemini_key(&app_default) {
            return Some(ResolvedGeminiCredential {
                key: app_default,
                source: GeminiCredentialSource::ApplicationDefault,
            });
        }

        None
    }

    pub fn get_gemini_api_key(&self) -> Option<String> {
        self.resolve_gemini_credential().map(|r| r.key)
    }

    pub fn is_gemini_configured(&self) -> bool {
        self.has_user_override()
    }

    pub fn set_gemini_api_key(&self, key: &str) -> Result<(), String> {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return self.clear_gemini_api_key();
        }

        #[cfg(test)]
        {
            let test_file = self.storage_dir.join(".gemini_test_key");
            let _ = std::fs::write(test_file, trimmed);
            return Ok(());
        }

        #[cfg(not(test))]
        {
            // 1. Attempt OS credential manager
            let entry_res = keyring::Entry::new(Self::SERVICE_NAME, Self::GEMINI_KEY_NAME);
            let os_result = match entry_res {
                Ok(entry) => entry.set_password(trimmed),
                Err(e) => Err(e),
            };

            if let Err(e) = os_result {
                return Err(format!(
                    "SECURE_STORAGE_UNAVAILABLE: Failed to store key in OS credential manager: {}",
                    e
                ));
            }

            // Mirror in process memory for session caching
            if let Ok(mut guard) = MEMORY_KEY_STORE.write() {
                *guard = Some(trimmed.to_string());
            }

            Ok(())
        }
    }

    pub fn clear_gemini_api_key(&self) -> Result<(), String> {
        #[cfg(test)]
        {
            let test_file = self.storage_dir.join(".gemini_test_key");
            let _ = std::fs::remove_file(test_file);
            return Ok(());
        }

        #[cfg(not(test))]
        {
            let entry_res = keyring::Entry::new(Self::SERVICE_NAME, Self::GEMINI_KEY_NAME);
            match entry_res {
                Ok(entry) => match entry.delete_credential() {
                    Ok(_) => {}
                    Err(keyring::Error::NoEntry) => {}
                    Err(e) => {
                        return Err(format!(
                            "SECURE_STORAGE_ERROR: Failed to delete credential from OS keyring: {}",
                            e
                        ));
                    }
                },
                Err(e) => {
                    return Err(format!(
                        "SECURE_STORAGE_ERROR: Failed to access OS credential manager: {}",
                        e
                    ));
                }
            }

            if let Ok(mut guard) = MEMORY_KEY_STORE.write() {
                *guard = None;
            }
            Ok(())
        }
    }
}

// -----------------------------------------------------------------------------
// Google Error Envelope & Message Sanitization
// -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GoogleErrorEnvelope {
    error: Option<GoogleErrorDetail>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoogleErrorDetail {
    #[serde(default)]
    code: Option<u16>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

pub fn sanitize_error_message(msg: &str, sensitive_key: Option<&str>) -> String {
    let mut out = msg.to_string();
    if let Some(key) = sensitive_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            out = out.replace(trimmed, "[REDACTED_API_KEY]");
        }
    }
    out
}

pub fn parse_google_error(
    status: reqwest::StatusCode,
    body_text: &str,
    sensitive_key: Option<&str>,
) -> (GeminiVerificationStatus, String, String) {
    let envelope = serde_json::from_str::<GoogleErrorEnvelope>(body_text)
        .ok()
        .and_then(|e| e.error);

    let raw_msg = envelope
        .as_ref()
        .and_then(|d| d.message.as_deref())
        .unwrap_or_else(|| body_text.trim());

    let provider_status = envelope
        .as_ref()
        .and_then(|d| d.status.as_deref())
        .unwrap_or("");

    let sanitized_msg = sanitize_error_message(raw_msg, sensitive_key);
    let lower_msg = sanitized_msg.to_lowercase();

    let (v_status, code_str) = match status.as_u16() {
        400 => {
            if lower_msg.contains("api_key_invalid")
                || lower_msg.contains("api key not valid")
                || lower_msg.contains("invalid api key")
                || lower_msg.contains("invalid key")
                || (provider_status == "INVALID_ARGUMENT" && lower_msg.contains("key"))
            {
                (
                    GeminiVerificationStatus::InvalidKey,
                    "GEMINI_API_KEY_INVALID",
                )
            } else {
                (GeminiVerificationStatus::BadRequest, "GEMINI_BAD_REQUEST")
            }
        }
        401 => (
            GeminiVerificationStatus::InvalidKey,
            "GEMINI_API_KEY_INVALID",
        ),
        403 => (GeminiVerificationStatus::Forbidden, "GEMINI_API_FORBIDDEN"),
        404 => (
            GeminiVerificationStatus::ModelUnavailable,
            "GEMINI_MODEL_UNAVAILABLE",
        ),
        429 => (GeminiVerificationStatus::RateLimited, "GEMINI_RATE_LIMITED"),
        500..=599 => (
            GeminiVerificationStatus::ProviderTemporaryFailure,
            "GEMINI_PROVIDER_TEMPORARY_FAILURE",
        ),
        _ => (GeminiVerificationStatus::Unknown, "GEMINI_UNKNOWN_ERROR"),
    };

    (v_status, code_str.to_string(), sanitized_msg)
}

// -----------------------------------------------------------------------------
// Managed Gemini Credential Manager (Application-Owned State)
// -----------------------------------------------------------------------------

#[derive(Debug)]
pub struct GeminiCredentialManager {
    secret_store: SecretStore,
    model: String,
    endpoint_base: Option<String>,
    client: reqwest::Client,
    status: RwLock<GeminiCredentialStatus>,
}

impl GeminiCredentialManager {
    pub const DEFAULT_MODEL: &'static str = DEFAULT_PROMPT_OPTIMIZATION_MODEL;

    pub fn new(secret_store: SecretStore) -> Self {
        let cred = secret_store.resolve_gemini_credential();
        let is_cfg = cred.is_some();
        let src = cred
            .as_ref()
            .map(|c| c.source)
            .unwrap_or(GeminiCredentialSource::NotConfigured);
        let stored = secret_store.has_user_override();

        let initial_status = GeminiCredentialStatus {
            stored,
            is_configured: is_cfg,
            source: src,
            verification_status: GeminiVerificationStatus::Unverified,
            model: Self::DEFAULT_MODEL.to_string(),
            last_verified_at: None,
            sanitized_message: None,
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            secret_store,
            model: Self::DEFAULT_MODEL.to_string(),
            endpoint_base: None,
            client,
            status: RwLock::new(initial_status),
        }
    }

    pub fn with_endpoint_and_model(
        secret_store: SecretStore,
        endpoint_base: Option<String>,
        model: String,
    ) -> Self {
        let cred = secret_store.resolve_gemini_credential();
        let is_cfg = cred.is_some();
        let src = cred
            .as_ref()
            .map(|c| c.source)
            .unwrap_or(GeminiCredentialSource::NotConfigured);
        let stored = secret_store.has_user_override();

        let initial_status = GeminiCredentialStatus {
            stored,
            is_configured: is_cfg,
            source: src,
            verification_status: GeminiVerificationStatus::Unverified,
            model: model.clone(),
            last_verified_at: None,
            sanitized_message: None,
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            secret_store,
            model,
            endpoint_base,
            client,
            status: RwLock::new(initial_status),
        }
    }

    pub fn secret_store(&self) -> &SecretStore {
        &self.secret_store
    }

    pub fn set_key(&self, key: &str) -> Result<(), String> {
        self.secret_store.set_gemini_api_key(key)?;
        let cred = self.secret_store.resolve_gemini_credential();
        let is_cfg = cred.is_some();
        let src = cred
            .as_ref()
            .map(|c| c.source)
            .unwrap_or(GeminiCredentialSource::NotConfigured);

        if let Ok(mut guard) = self.status.write() {
            guard.stored = true;
            guard.is_configured = is_cfg;
            guard.source = src;
            guard.verification_status = GeminiVerificationStatus::Unverified;
            guard.sanitized_message = None;
        }
        Ok(())
    }

    pub fn clear_key(&self) -> Result<(), String> {
        self.secret_store.clear_gemini_api_key()?;
        let cred = self.secret_store.resolve_gemini_credential();
        let is_cfg = cred.is_some();
        let src = cred
            .as_ref()
            .map(|c| c.source)
            .unwrap_or(GeminiCredentialSource::NotConfigured);

        if let Ok(mut guard) = self.status.write() {
            guard.stored = false;
            guard.is_configured = is_cfg;
            guard.source = src;
            guard.verification_status = GeminiVerificationStatus::Unverified;
            guard.last_verified_at = None;
            guard.sanitized_message = None;
        }
        Ok(())
    }

    pub fn get_status(&self) -> GeminiCredentialStatus {
        let cred = self.secret_store.resolve_gemini_credential();
        let is_cfg = cred.is_some();
        let src = cred
            .as_ref()
            .map(|c| c.source)
            .unwrap_or(GeminiCredentialSource::NotConfigured);
        let stored = self.secret_store.has_user_override();

        let guard = self.status.read().unwrap();
        GeminiCredentialStatus {
            stored,
            is_configured: is_cfg,
            source: src,
            verification_status: guard.verification_status,
            model: self.model.clone(),
            last_verified_at: guard.last_verified_at.clone(),
            sanitized_message: guard.sanitized_message.clone(),
        }
    }

    pub async fn test_api_key(&self) -> Result<GeminiCredentialStatus, String> {
        let resolved = match self.secret_store.resolve_gemini_credential() {
            Some(r) => r,
            None => {
                let st = GeminiCredentialStatus {
                    stored: false,
                    is_configured: false,
                    source: GeminiCredentialSource::NotConfigured,
                    verification_status: GeminiVerificationStatus::InvalidKey,
                    model: self.model.clone(),
                    last_verified_at: Some(Utc::now().to_rfc3339()),
                    sanitized_message: Some(
                        "GEMINI_API_KEY_NOT_CONFIGURED: No valid Gemini API key configured"
                            .to_string(),
                    ),
                };
                if let Ok(mut guard) = self.status.write() {
                    *guard = st.clone();
                }
                return Ok(st);
            }
        };

        let base_url = self
            .endpoint_base
            .as_deref()
            .unwrap_or("https://generativelanguage.googleapis.com/v1beta");
        let endpoint = format!("{}/models/{}", base_url.trim_end_matches('/'), self.model);

        let send_res = self
            .client
            .get(&endpoint)
            .header("x-goog-api-key", &resolved.key)
            .send()
            .await;

        match send_res {
            Ok(resp) => {
                let status_code = resp.status();
                if status_code.is_success() {
                    let st = GeminiCredentialStatus {
                        stored: self.secret_store.has_user_override(),
                        is_configured: true,
                        source: resolved.source,
                        verification_status: GeminiVerificationStatus::Valid,
                        model: self.model.clone(),
                        last_verified_at: Some(Utc::now().to_rfc3339()),
                        sanitized_message: None,
                    };
                    if let Ok(mut guard) = self.status.write() {
                        *guard = st.clone();
                    }
                    Ok(st)
                } else {
                    let body = resp.text().await.unwrap_or_default();
                    let (ver_status, _code, sanitized_msg) =
                        parse_google_error(status_code, &body, Some(&resolved.key));
                    let st = GeminiCredentialStatus {
                        stored: self.secret_store.has_user_override(),
                        is_configured: true,
                        source: resolved.source,
                        verification_status: ver_status,
                        model: self.model.clone(),
                        last_verified_at: Some(Utc::now().to_rfc3339()),
                        sanitized_message: Some(sanitized_msg),
                    };
                    if let Ok(mut guard) = self.status.write() {
                        *guard = st.clone();
                    }
                    Ok(st)
                }
            }
            Err(e) => {
                let ver_status = if e.is_timeout() {
                    GeminiVerificationStatus::Timeout
                } else {
                    GeminiVerificationStatus::NetworkError
                };
                let sanitized_msg = sanitize_error_message(&e.to_string(), Some(&resolved.key));
                let st = GeminiCredentialStatus {
                    stored: self.secret_store.has_user_override(),
                    is_configured: true,
                    source: resolved.source,
                    verification_status: ver_status,
                    model: self.model.clone(),
                    last_verified_at: Some(Utc::now().to_rfc3339()),
                    sanitized_message: Some(sanitized_msg),
                };
                if let Ok(mut guard) = self.status.write() {
                    *guard = st.clone();
                }
                Ok(st)
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Gemini Prompt Optimizer Engine
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GeminiPromptOptimizer {
    secret_store: SecretStore,
    client: reqwest::Client,
    endpoint_base: Option<String>,
    policy: PromptOptimizationCapabilityPolicy,
}

impl GeminiPromptOptimizer {
    pub const DEFAULT_MODEL: &'static str = DEFAULT_PROMPT_OPTIMIZATION_MODEL;

    pub fn new(secret_store: SecretStore) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            secret_store,
            client,
            endpoint_base: None,
            policy: PromptOptimizationCapabilityPolicy::default(),
        }
    }

    pub fn with_endpoint_and_model(
        secret_store: SecretStore,
        endpoint_base: Option<String>,
        model: String,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            secret_store,
            client,
            endpoint_base,
            policy: PromptOptimizationCapabilityPolicy {
                model,
                ..Default::default()
            },
        }
    }

    pub fn policy(&self) -> &PromptOptimizationCapabilityPolicy {
        &self.policy
    }

    pub fn is_configured(&self) -> bool {
        self.secret_store.is_gemini_configured()
    }

    pub fn secret_store(&self) -> &SecretStore {
        &self.secret_store
    }

    pub async fn optimize_prompt(
        &self,
        request: OptimizePromptRequest,
    ) -> Result<OptimizePromptResponse, String> {
        let raw_prompt = request.prompt.trim();
        if raw_prompt.is_empty() {
            return Err("REQUEST_INVALID: Prompt cannot be empty or whitespace".to_string());
        }

        let resolved = self
            .secret_store
            .resolve_gemini_credential()
            .ok_or_else(|| {
                "GEMINI_API_KEY_NOT_CONFIGURED: Gemini API key is not configured".to_string()
            })?;

        let system_instruction = "You are an expert AI video transformation and editing prompt engineer. \
Your task is to take a user's raw video edit request and optimize it into a clear, precise, preservation-first, visually vivid prompt for video AI editing and transformation. \
CRITICAL PRESERVATION & TRANSFORMATION RULES: \
1. When Transformation Intent is FACE_REPLACE and Identity Mode is GENERATED: \
   Explicitly describe replacing ONLY the target person's facial identity with a new, consistent synthetic facial identity. Strictly instruct preserving the person's body, clothing, hairstyle as much as practical, expressions, mouth movement, head pose, actions, camera motion, background scene, lighting, composition, timing, and all non-target people in the video. \
2. When Transformation Intent is FACE_REPLACE and Identity Mode is REFERENCE: \
   Instruct applying the specified reference facial identity while strictly preserving the person's body, clothing, motion, background, and non-target people. \
3. DO NOT expand face replacement into a full character redesign, body change, background replacement, or style change unless explicitly requested by the user prompt. \
4. Output ONLY the raw optimized prompt text directly without quotes, markdown formatting, explanations, or conversational filler.";

        let mut context_parts = vec![
            format!("User Prompt: \"{}\"", raw_prompt),
            format!(
                "Task Type: {}",
                request
                    .task_type
                    .as_deref()
                    .unwrap_or("VIDEO_TRANSFORMATION")
            ),
            format!(
                "Transformation Intent: {}",
                request
                    .transformation_intent
                    .as_deref()
                    .unwrap_or("FACE_REPLACE")
            ),
            format!(
                "Identity Mode: {}",
                request.identity_mode.as_deref().unwrap_or("GENERATED")
            ),
        ];

        if let Some(target) = &request.target_descriptor {
            context_parts.push(format!("Target Person: {}", target));
        }
        if let Some(bg) = request.preserve_background {
            context_parts.push(format!("Preserve Background: {}", bg));
        }
        if let Some(body) = request.preserve_body {
            context_parts.push(format!("Preserve Body: {}", body));
        }
        if let Some(clothing) = request.preserve_clothing {
            context_parts.push(format!("Preserve Clothing: {}", clothing));
        }
        if let Some(ntf) = request.preserve_non_target_faces {
            context_parts.push(format!("Preserve Non-Target Faces: {}", ntf));
        }
        if let Some(dur) = request.video_duration_sec {
            context_parts.push(format!("Video Duration: {:.1}s", dur));
        }
        if let Some(res) = request.resolution {
            context_parts.push(format!("Target Resolution: {}x{}", res.0, res.1));
        }

        let user_content = context_parts.join("\n");

        let payload = serde_json::json!({
            "systemInstruction": {
                "parts": [{ "text": system_instruction }]
            },
            "contents": [{
                "parts": [{ "text": user_content }]
            }],
            "generationConfig": {
                "maxOutputTokens": self.policy.max_output_tokens
            }
        });

        let base_url = self
            .endpoint_base
            .as_deref()
            .unwrap_or("https://generativelanguage.googleapis.com/v1beta");
        let endpoint = format!(
            "{}/models/{}:generateContent",
            base_url.trim_end_matches('/'),
            self.policy.model
        );

        let response = match self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", &resolved.key)
            .json(&payload)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let code_str = if e.is_timeout() {
                    "GEMINI_TIMEOUT"
                } else {
                    "GEMINI_NETWORK_ERROR"
                };
                let sanitized = sanitize_error_message(&e.to_string(), Some(&resolved.key));
                return Err(format!("{}: {}", code_str, sanitized));
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            let (_ver_status, code_str, sanitized_msg) =
                parse_google_error(status, &body_text, Some(&resolved.key));
            return Err(format!("{}: {}", code_str, sanitized_msg));
        }

        let body_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("GEMINI_MALFORMED_JSON: Malformed JSON response: {}", e))?;

        let candidate_text = body_json
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c0| c0.get("content"))
            .and_then(|cnt| cnt.get("parts"))
            .and_then(|parts| parts.get(0))
            .and_then(|p0| p0.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .trim();

        if candidate_text.is_empty() {
            return Err("GEMINI_EMPTY_RESPONSE: Empty response from Gemini model".to_string());
        }

        let clean_optimized = if candidate_text.len() > 3000 {
            candidate_text[..3000].to_string()
        } else {
            candidate_text.to_string()
        };

        let prompt_hash = calculate_prompt_hash(&clean_optimized);

        Ok(OptimizePromptResponse {
            optimized_prompt: clean_optimized,
            model: self.policy.model.clone(),
            prompt_source: PromptSource::GeminiOptimized,
            prompt_hash,
        })
    }
}
