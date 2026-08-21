use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiStatusResponse {
    pub is_configured: bool,
    pub model: String,
}

pub fn calculate_prompt_hash(prompt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

// -----------------------------------------------------------------------------
// Concrete Cross-Platform Encrypted Secret Store (OS Credential Manager)
// -----------------------------------------------------------------------------

use std::sync::RwLock;

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
        // 1. In-memory runtime store
        if let Ok(guard) = MEMORY_KEY_STORE.read() {
            if let Some(ref k) = *guard {
                let trimmed = k.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }

        // 2. Try OS Credential Manager (Windows Credential Manager / macOS Keychain / Linux Secret Service)
        if let Ok(entry) = keyring::Entry::new(Self::SERVICE_NAME, Self::GEMINI_KEY_NAME) {
            if let Ok(password) = entry.get_password() {
                let trimmed = password.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }

        // 3. DEV fallback only: check environment variable
        std::env::var("GEMINI_API_KEY").ok().and_then(|k| {
            let trimmed = k.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
    }

    pub fn set_gemini_api_key(&self, key: &str) -> Result<(), String> {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            return self.clear_gemini_api_key();
        }

        // Attempt OS credential manager
        if let Ok(entry) = keyring::Entry::new(Self::SERVICE_NAME, Self::GEMINI_KEY_NAME) {
            let _ = entry.set_password(trimmed);
        }

        // Store in secure process memory without touching disk
        if let Ok(mut guard) = MEMORY_KEY_STORE.write() {
            *guard = Some(trimmed.to_string());
        }

        Ok(())
    }

    pub fn clear_gemini_api_key(&self) -> Result<(), String> {
        if let Ok(entry) = keyring::Entry::new(Self::SERVICE_NAME, Self::GEMINI_KEY_NAME) {
            let _ = entry.delete_credential();
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
// Gemini Prompt Optimizer Engine
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GeminiPromptOptimizer {
    secret_store: SecretStore,
    client: reqwest::Client,
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
            model: Self::DEFAULT_MODEL.to_string(),
        }
    }

    pub fn with_model(secret_store: SecretStore, model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            secret_store,
            client,
            model,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.secret_store.is_gemini_configured()
    }

    pub fn secret_store(&self) -> &SecretStore {
        &self.secret_store
    }

    pub fn get_status(&self) -> GeminiStatusResponse {
        GeminiStatusResponse {
            is_configured: self.is_configured(),
            model: self.model.clone(),
        }
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
            "PROMPT_OPTIMIZATION_FAILED: Gemini API key is not configured".to_string()
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

        let endpoint = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, api_key
        );

        let response = self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("PROMPT_OPTIMIZATION_FAILED: Network error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let err_body = response.text().await.unwrap_or_default();
            if status.as_u16() == 429 {
                return Err(
                    "PROMPT_OPTIMIZATION_FAILED: Gemini quota exceeded (rate limited)".to_string(),
                );
            }
            return Err(format!(
                "PROMPT_OPTIMIZATION_FAILED: Gemini API returned status {} ({})",
                status, err_body
            ));
        }

        let body_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("PROMPT_OPTIMIZATION_FAILED: Malformed JSON response: {}", e))?;

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
            return Err("PROMPT_OPTIMIZATION_FAILED: Empty response from Gemini model".to_string());
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
