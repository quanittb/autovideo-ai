use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdentityMode {
    Generated,
    Reference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaceReplaceContract {
    pub case_id: String,
    pub video_file: String,
    pub transformation_intent: String,
    pub identity_mode: IdentityMode,
    pub reference_face_file: Option<String>,
    pub target_face_index: Option<usize>,
    pub target_face_confirmed: bool,
    pub replace_count: usize,
    pub preserve_non_target_faces: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetFaceError {
    TargetFaceAmbiguous(String),
    MultipleFacesNotAllowed(String),
    InvalidTargetIndex(usize),
}

impl std::fmt::Display for TargetFaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetFaceAmbiguous(msg) => write!(f, "TARGET_FACE_AMBIGUOUS: {}", msg),
            Self::MultipleFacesNotAllowed(msg) => {
                write!(f, "MULTIPLE_FACES_NOT_ALLOWED: {}", msg)
            }
            Self::InvalidTargetIndex(idx) => write!(f, "INVALID_TARGET_INDEX: {}", idx),
        }
    }
}

impl std::error::Error for TargetFaceError {}

pub struct TargetFacePolicy;

impl TargetFacePolicy {
    /// Validates target face specification before execution.
    /// In multi-face videos (e.g. C3), target_face_confirmed MUST be true, exactly ONE face must be replaced,
    /// and all non-target faces must be preserved.
    pub fn validate_target(
        visible_face_count: usize,
        target_index: Option<usize>,
        is_confirmed: bool,
        replace_count: usize,
    ) -> Result<usize, TargetFaceError> {
        if replace_count != 1 {
            return Err(TargetFaceError::MultipleFacesNotAllowed(format!(
                "Replace count must be exactly 1, got {}",
                replace_count
            )));
        }

        if visible_face_count == 1 {
            return Ok(target_index.unwrap_or(0));
        }

        // Multi-face scenario (visible_face_count > 1)
        if !is_confirmed || target_index.is_none() {
            return Err(TargetFaceError::TargetFaceAmbiguous(
                "Multiple visible faces detected but target face has not been positively confirmed"
                    .to_string(),
            ));
        }

        let idx = target_index.unwrap();
        if idx >= visible_face_count {
            return Err(TargetFaceError::InvalidTargetIndex(idx));
        }

        Ok(idx)
    }
}

pub struct IdentityResolver;

impl IdentityResolver {
    /// Resolves the IdentityMode based strictly on the user request.
    /// The presence of repo test fixtures (e.g. `test-assets/phase20b/faces/face.jpg`) NEVER
    /// causes a default request to become REFERENCE mode.
    pub fn resolve_mode(
        user_supplied_reference_face: Option<&Path>,
    ) -> (IdentityMode, Option<String>) {
        match user_supplied_reference_face {
            Some(path) => {
                let path_str = path.to_string_lossy().replace('\\', "/");
                (IdentityMode::Reference, Some(path_str))
            }
            None => (IdentityMode::Generated, None),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyResolution {
    Configured(String),
    NotConfigured,
}

pub struct ApiKeyResolver;

impl ApiKeyResolver {
    /// Resolves API key following strict precedence:
    /// 1. User explicit override
    /// 2. Application default credential / environment
    /// 3. NOT_CONFIGURED
    ///
    /// Placeholders like "Axxxxxxxxxxx" are treated as NOT_CONFIGURED.
    pub fn resolve(user_override: Option<&str>, env_var_name: &str) -> ApiKeyResolution {
        if let Some(key) = user_override {
            let trimmed = key.trim();
            if Self::is_valid_key(trimmed) {
                return ApiKeyResolution::Configured(trimmed.to_string());
            }
        }

        if let Ok(env_val) = std::env::var(env_var_name) {
            let trimmed = env_val.trim();
            if Self::is_valid_key(trimmed) {
                return ApiKeyResolution::Configured(trimmed.to_string());
            }
        }

        ApiKeyResolution::NotConfigured
    }

    fn is_valid_key(key: &str) -> bool {
        if key.is_empty() {
            return false;
        }
        // Reject common template placeholders
        if key.starts_with("Axxxx")
            || key == "your_api_key_here"
            || key == "PLACEHOLDER"
            || key
                .chars()
                .all(|c| c == 'x' || c == 'X' || c == '0' || c == '*')
        {
            return false;
        }
        true
    }

    /// Masks any secret key for safe logging or reporting (prevents credential leakage).
    pub fn mask_key(key: &str) -> String {
        if key.len() <= 8 {
            "***".to_string()
        } else {
            format!("{}...{}", &key[..4], &key[key.len() - 4..])
        }
    }
}
