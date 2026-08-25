use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransformationIntent {
    FaceReplace,
    BackgroundReplace,
    BackgroundRemove,
    LightingEdit,
    StyleEdit,
    GenericPromptEdit,
}

impl Default for TransformationIntent {
    fn default() -> Self {
        Self::FaceReplace
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdentityMode {
    Generated,
    Reference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetFaceSelection {
    pub index: usize,
    pub confirmed: bool,
    pub descriptor: Option<String>,
    pub anchor_frame_timestamp_sec: Option<f64>,
    pub normalized_bounding_box: Option<[f64; 4]>, // [x, y, w, h]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceReplaceContract {
    pub case_id: String,
    pub video_file: String,
    pub transformation_intent: TransformationIntent,
    pub identity_mode: IdentityMode,
    pub reference_face_file: Option<String>,
    pub target_face: TargetFaceSelection,
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
        target: &TargetFaceSelection,
        replace_count: usize,
    ) -> Result<usize, TargetFaceError> {
        if replace_count != 1 {
            return Err(TargetFaceError::MultipleFacesNotAllowed(format!(
                "Replace count must be exactly 1, got {}",
                replace_count
            )));
        }

        if visible_face_count == 1 {
            return Ok(target.index);
        }

        // Multi-face scenario (visible_face_count > 1)
        if !target.confirmed {
            return Err(TargetFaceError::TargetFaceAmbiguous(
                "Multiple visible faces detected but target face has not been positively confirmed"
                    .to_string(),
            ));
        }

        if target.index >= visible_face_count {
            return Err(TargetFaceError::InvalidTargetIndex(target.index));
        }

        Ok(target.index)
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

// -----------------------------------------------------------------------------
// Backend Credential Management & Application Default Credential Layer
// -----------------------------------------------------------------------------

/// Backend-isolated development placeholder for Pruna.
/// If left as sentinel "Axxxxxxxxxxx", it is treated as NOT_CONFIGURED.
pub const DEFAULT_PRUNA_API_KEY: &str = "Axxxxxxxxxxx";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    UserOverride,
    DeploymentDefault,
    ApplicationDefault,
    NotConfigured,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatusDto {
    pub provider_id: String,
    pub is_configured: bool,
    pub source: CredentialSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedCredential {
    Configured {
        key: String,
        source: CredentialSource,
    },
    NotConfigured,
}

pub struct ProviderCredentialResolver;

impl ProviderCredentialResolver {
    /// Resolves Pruna credential with strict precedence:
    /// 1. Valid user override
    /// 2. Deployment / Environment variable (`REPLICATE_API_TOKEN`)
    /// 3. Application Default Development Credential (`DEFAULT_PRUNA_API_KEY`)
    /// 4. NotConfigured (if sentinel "Axxxxxxxxxxx" or unconfigured)
    pub fn resolve_pruna(user_override: Option<&str>) -> ResolvedCredential {
        // 1. User override
        if let Some(key) = user_override {
            let trimmed = key.trim();
            if Self::is_valid_key(trimmed) {
                return ResolvedCredential::Configured {
                    key: trimmed.to_string(),
                    source: CredentialSource::UserOverride,
                };
            }
        }

        // 2. Deployment / Environment variable
        if let Ok(env_val) = std::env::var("REPLICATE_API_TOKEN") {
            let trimmed = env_val.trim();
            if Self::is_valid_key(trimmed) {
                return ResolvedCredential::Configured {
                    key: trimmed.to_string(),
                    source: CredentialSource::DeploymentDefault,
                };
            }
        }

        // 3. Application Default
        let app_default = DEFAULT_PRUNA_API_KEY.trim();
        if Self::is_valid_key(app_default) {
            return ResolvedCredential::Configured {
                key: app_default.to_string(),
                source: CredentialSource::ApplicationDefault,
            };
        }

        ResolvedCredential::NotConfigured
    }

    /// Resolves generic API key given custom app default and env variable name (for testing/custom providers).
    pub fn resolve_custom(
        user_override: Option<&str>,
        env_var_name: &str,
        app_default_key: Option<&str>,
    ) -> ResolvedCredential {
        if let Some(key) = user_override {
            let trimmed = key.trim();
            if Self::is_valid_key(trimmed) {
                return ResolvedCredential::Configured {
                    key: trimmed.to_string(),
                    source: CredentialSource::UserOverride,
                };
            }
        }

        if let Ok(env_val) = std::env::var(env_var_name) {
            let trimmed = env_val.trim();
            if Self::is_valid_key(trimmed) {
                return ResolvedCredential::Configured {
                    key: trimmed.to_string(),
                    source: CredentialSource::DeploymentDefault,
                };
            }
        }

        if let Some(app_default) = app_default_key {
            let trimmed = app_default.trim();
            if Self::is_valid_key(trimmed) {
                return ResolvedCredential::Configured {
                    key: trimmed.to_string(),
                    source: CredentialSource::ApplicationDefault,
                };
            }
        }

        ResolvedCredential::NotConfigured
    }

    /// Returns a frontend-safe status DTO with zero credential leakage.
    pub fn get_pruna_status(user_override: Option<&str>) -> CredentialStatusDto {
        match Self::resolve_pruna(user_override) {
            ResolvedCredential::Configured { source, .. } => CredentialStatusDto {
                provider_id: "pruna".to_string(),
                is_configured: true,
                source,
            },
            ResolvedCredential::NotConfigured => CredentialStatusDto {
                provider_id: "pruna".to_string(),
                is_configured: false,
                source: CredentialSource::NotConfigured,
            },
        }
    }

    pub fn is_valid_key(key: &str) -> bool {
        if key.is_empty() {
            return false;
        }
        // Reject common template placeholders and sentinels
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
