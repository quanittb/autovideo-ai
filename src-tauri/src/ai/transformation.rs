use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransformationIntent {
    FaceReplace,
    BackgroundReplace,
    BackgroundRemove,
    LightingEdit,
    StyleEdit,
    ObjectEdit,
    GenericPromptEdit,
}

impl Default for TransformationIntent {
    fn default() -> Self {
        TransformationIntent::FaceReplace
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdentityMode {
    Generated,
    Reference,
}

impl Default for IdentityMode {
    fn default() -> Self {
        IdentityMode::Generated
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetFaceCandidate {
    pub index: usize,
    pub label: String,
    pub descriptor: String,
    pub anchor_timestamp_sec: f64,
    pub normalized_bounding_box: [f64; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetFaceSelection {
    pub index: usize,
    pub confirmed: bool,
    #[serde(default)]
    pub descriptor: Option<String>,
    #[serde(default, alias = "anchorTimestampSec")]
    pub anchor_frame_timestamp_sec: Option<f64>,
    #[serde(default)]
    pub normalized_bounding_box: Option<[f64; 4]>,
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

#[derive(Debug, Clone)]
pub struct TargetFacePolicy;

impl TargetFacePolicy {
    /// Validates target face specification before execution.
    /// In multi-face videos (e.g. C3), target_face confirmed MUST be true, exactly ONE face must be replaced,
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

    /// Validates against known candidate list when available.
    pub fn validate_target_candidates(
        selection: Option<&TargetFaceSelection>,
        known_candidates: &[TargetFaceCandidate],
    ) -> Result<Option<usize>, String> {
        if known_candidates.is_empty() {
            // Single/unknown face scenario
            return Ok(selection.map(|s| s.index));
        }

        if known_candidates.len() == 1 {
            return Ok(Some(known_candidates[0].index));
        }

        // Multi-face scenario requires confirmed explicit target
        match selection {
            Some(target) => {
                if !target.confirmed {
                    return Err(format!(
                        "TARGET_FACE_AMBIGUOUS: Multiple visible faces detected ({} candidates). Target face #{} ({:?}) must be confirmed before generation.",
                        known_candidates.len(),
                        target.index,
                        target.descriptor
                    ));
                }
                if target.index >= known_candidates.len() {
                    return Err(format!(
                        "TARGET_FACE_INVALID: Target index {} exceeds available candidates ({})",
                        target.index,
                        known_candidates.len()
                    ));
                }
                Ok(Some(target.index))
            }
            None => Err(format!(
                "TARGET_FACE_AMBIGUOUS: Multiple visible faces detected ({} candidates). A confirmed target face selection is required.",
                known_candidates.len()
            )),
        }
    }
}
