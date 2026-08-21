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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiCredentialStatus {
    pub stored: bool,
    pub verification_status: GeminiVerificationStatus,
    pub model: String,
    #[serde(default)]
    pub last_verified_at: Option<String>,
    #[serde(default)]
    pub sanitized_message: Option<String>,
}

// Backward-compatible DTO alias for existing consumers
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

static MEMORY_KEY_STORE: RwLock<Option<String>> = RwLock::new(None);

#[derive(Debug, Clone)]
pub struct SecretStore {
    _storage_dir: PathBuf,
}

impl SecretStore {
    pub const SERVICE_NAME: &'static str = "autovideo-ai";
    pub const GEMINI_KEY_NAME: &'static str = "gemini_api_key";

    pub fn new(storage_dir: PathBuf) -> Self {
        Self {
            _storage_dir: storage_dir,
        }
    }

    pub fn get_gemini_api_key(&self) -> Option<String> {
        // 1. Try OS Credential Manager (Windows Credential Manager / macOS Keychain / Linux Secret Service)
        if let Ok(entry) = keyring::Entry::new(Self::SERVICE_NAME, Self::GEMINI_KEY_NAME) {
            if let Ok(password) = entry.get_password() {
                let trimmed = password.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }

        // 2. In-memory runtime mirror (for dev/test)
        if let Ok(guard) = MEMORY_KEY_STORE.read() {
            if let Some(ref k) = *guard {
                let trimmed = k.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }

        // 3. DEV fallback only (debug builds or explicit dev environment variable)
        #[cfg(debug_assertions)]
        if let Ok(k) = std::env::var("GEMINI_API_KEY") {
            let trimmed = k.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }

        None
    }

    pub fn set_gemini_api_key(&self, key: &str) -> Result<(), String> {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return self.clear_gemini_api_key();
        }

        // 1. Attempt OS credential manager
        let entry_res = keyring::Entry::new(Self::SERVICE_NAME, Self::GEMINI_KEY_NAME);
        let os_result = match entry_res {
            Ok(entry) => entry.set_password(trimmed),
            Err(e) => Err(e),
        };

        if let Err(e) = os_result {
            // Check if in test environment, allow in-memory store for sandboxed CI/tests
            #[cfg(test)]
            {
                let _ = e;
                if let Ok(mut guard) = MEMORY_KEY_STORE.write() {
                    *guard = Some(trimmed.to_string());
                }
                return Ok(());
            }

            #[cfg(not(test))]
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

    pub fn clear_gemini_api_key(&self) -> Result<(), String> {
        let entry_res = keyring::Entry::new(Self::SERVICE_NAME, Self::GEMINI_KEY_NAME);
        match entry_res {
            Ok(entry) => match entry.delete_credential() {
                Ok(_) => {}
                Err(keyring::Error::NoEntry) => {}
                Err(e) => {
                    #[cfg(not(test))]
                    return Err(format!(
                        "SECURE_STORAGE_ERROR: Failed to delete credential from OS keyring: {}",
                        e
                    ));
                    #[cfg(test)]
                    {
                        let _ = e;
                    }
                }
            },
            Err(e) => {
                #[cfg(not(test))]
                return Err(format!(
                    "SECURE_STORAGE_ERROR: Failed to access OS credential manager: {}",
                    e
                ));
                #[cfg(test)]
                {
                    let _ = e;
                }
            }
        }

        if let Ok(mut guard) = MEMORY_KEY_STORE.write() {
            *guard = None;
        }
        Ok(())
    }

    pub fn is_gemini_configured(&self) -> bool {
        self.get_gemini_api_key().is_some()
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
    pub const DEFAULT_MODEL: &'static str = "gemini-2.5-flash-lite";

    pub fn new(secret_store: SecretStore) -> Self {
        let is_cfg = secret_store.is_gemini_configured();
        let initial_status = GeminiCredentialStatus {
            stored: is_cfg,
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
        let is_cfg = secret_store.is_gemini_configured();
        let initial_status = GeminiCredentialStatus {
            stored: is_cfg,
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
        if let Ok(mut guard) = self.status.write() {
            guard.stored = true;
            guard.verification_status = GeminiVerificationStatus::Unverified;
            guard.sanitized_message = None;
            guard.last_verified_at = None;
        }
        Ok(())
    }

    pub fn clear_key(&self) -> Result<(), String> {
        self.secret_store.clear_gemini_api_key()?;
        if let Ok(mut guard) = self.status.write() {
            guard.stored = false;
            guard.verification_status = GeminiVerificationStatus::Unverified;
            guard.sanitized_message = None;
            guard.last_verified_at = None;
        }
        Ok(())
    }

    pub fn get_status(&self) -> GeminiCredentialStatus {
        let is_cfg = self.secret_store.is_gemini_configured();
        if let Ok(guard) = self.status.read() {
            let mut st = guard.clone();
            st.stored = is_cfg;
            st
        } else {
            GeminiCredentialStatus {
                stored: is_cfg,
                verification_status: GeminiVerificationStatus::Unverified,
                model: self.model.clone(),
                last_verified_at: None,
                sanitized_message: None,
            }
        }
    }

    pub async fn test_api_key(&self) -> Result<GeminiCredentialStatus, String> {
        let key_opt = self.secret_store.get_gemini_api_key();
        let key = match key_opt {
            Some(k) if !k.trim().is_empty() => k,
            _ => {
                let st = GeminiCredentialStatus {
                    stored: false,
                    verification_status: GeminiVerificationStatus::Unverified,
                    model: self.model.clone(),
                    last_verified_at: None,
                    sanitized_message: Some(
                        "No Gemini API key stored in credential manager".to_string(),
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
            .header("x-goog-api-key", &key)
            .send()
            .await;

        match send_res {
            Ok(resp) => {
                let status_code = resp.status();
                if status_code.is_success() {
                    let st = GeminiCredentialStatus {
                        stored: true,
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
                        parse_google_error(status_code, &body, Some(&key));
                    let st = GeminiCredentialStatus {
                        stored: true,
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
                let sanitized_msg = sanitize_error_message(&e.to_string(), Some(&key));
                let st = GeminiCredentialStatus {
                    stored: true,
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
    model: String,
}

impl GeminiPromptOptimizer {
    pub const DEFAULT_MODEL: &'static str = "gemini-2.5-flash-lite";

    pub fn new(secret_store: SecretStore) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            secret_store,
            client,
            endpoint_base: None,
            model: Self::DEFAULT_MODEL.to_string(),
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
            model,
        }
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

        let api_key = self.secret_store.get_gemini_api_key().ok_or_else(|| {
            "GEMINI_API_KEY_NOT_CONFIGURED: Gemini API key is not configured".to_string()
        })?;

        let system_instruction = "You are an expert AI video generation prompt engineer. \
Your task is to take a raw, user-provided video transformation or generation prompt and optimize it into a clear, visually vivid, cinematic, and descriptive prompt for video AI generation. \
Maintain the user's core intent, characters, and subject actions. \
Avoid commentary or conversational filler. Output ONLY the optimized prompt text directly without quotes or formatting tags.";

        let user_content = format!(
            "User Prompt: \"{}\"\nTask Type: {}\nVideo Duration: {}s\nTarget Resolution: {:?}",
            raw_prompt,
            request
                .task_type
                .as_deref()
                .unwrap_or("VIDEO_TRANSFORMATION"),
            request.video_duration_sec.unwrap_or(5.0),
            request.resolution.unwrap_or((1920, 1080))
        );

        let payload = serde_json::json!({
            "systemInstruction": {
                "parts": [{ "text": system_instruction }]
            },
            "contents": [{
                "parts": [{ "text": user_content }]
            }],
            "generationConfig": {
                "temperature": 0.7,
                "maxOutputTokens": 800
            }
        });

        let base_url = self
            .endpoint_base
            .as_deref()
            .unwrap_or("https://generativelanguage.googleapis.com/v1beta");
        let endpoint = format!(
            "{}/models/{}:generateContent",
            base_url.trim_end_matches('/'),
            self.model
        );

        let response = match self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", &api_key)
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
                let sanitized = sanitize_error_message(&e.to_string(), Some(&api_key));
                return Err(format!("{}: {}", code_str, sanitized));
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            let (_ver_status, code_str, sanitized_msg) =
                parse_google_error(status, &body_text, Some(&api_key));
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
            model: self.model.clone(),
            prompt_source: PromptSource::GeminiOptimized,
            prompt_hash,
        })
    }
}
