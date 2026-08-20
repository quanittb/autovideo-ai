use super::error::CloudProviderError;
use super::job::CloudJobRequest;
use super::provider::{ProviderKey, ResolutionTier, TargetFps};
use super::submission::ValidatedSubmissionPlan;
use super::uploader::UploadedAsset;
use crate::projects::Project;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSubmissionSpec {
    pub provider_key: ProviderKey,
    pub source_video: PathBuf,
    pub reference_images: Vec<PathBuf>,
    pub instruction_prompt: Option<String>,
    pub resolution_tier: ResolutionTier,
    pub target_fps: TargetFps,
    pub save_audio: bool,
    pub ignore_audio: bool,
    pub turbo: bool,
    pub disable_safety_checker: bool,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreparedProviderSubmission {
    pub spec: ProviderSubmissionSpec,
    pub uploaded_source: UploadedAsset,
    pub uploaded_references: Vec<UploadedAsset>,
}

impl ProviderSubmissionSpec {
    pub fn build(
        request: &CloudJobRequest,
        project: &Project,
        plan: &ValidatedSubmissionPlan,
    ) -> Result<Self, CloudProviderError> {
        // 1. Source Video validation
        let source_video = match &request.source_video {
            Some(p) if p.is_file() => p.clone(),
            Some(p) => {
                return Err(CloudProviderError::RequestInvalid(format!(
                    "Source video not found or invalid: {}",
                    p.display()
                )));
            }
            None => {
                return Err(CloudProviderError::RequestInvalid(
                    "Source video is required for character replacement".to_string(),
                ));
            }
        };

        // 2. Reference Images Normalization (Canonical reference_images vs legacy reference_image)
        let reference_images = match (&request.reference_images, &request.reference_image) {
            (Some(list), Some(single)) if !list.is_empty() => {
                // If single is not contained in list and list has items, reject as ambiguous
                if !list.contains(single) {
                    return Err(CloudProviderError::RequestInvalid(
                        "AMBIGUOUS_REFERENCE_INPUTS: Conflicting reference_image and reference_images provided"
                            .to_string(),
                    ));
                }
                list.clone()
            }
            (Some(list), _) if !list.is_empty() => list.clone(),
            (_, Some(single)) => vec![single.clone()],
            _ => {
                if plan.task_class == crate::ai::cloud::router::TaskClass::CharacterReplacement {
                    return Err(CloudProviderError::RequestInvalid(
                        "At least 1 reference image is required for character replacement"
                            .to_string(),
                    ));
                }
                Vec::new()
            }
        };

        if reference_images.is_empty()
            && plan.task_class == crate::ai::cloud::router::TaskClass::CharacterReplacement
        {
            return Err(CloudProviderError::RequestInvalid(
                "At least 1 reference image is required for character replacement".to_string(),
            ));
        }

        if reference_images.len() > 3 {
            return Err(CloudProviderError::RequestInvalid(format!(
                "Too many reference images ({}); current official schema supports 1 to 3 references",
                reference_images.len()
            )));
        }

        for img in &reference_images {
            if !img.is_file() {
                return Err(CloudProviderError::RequestInvalid(format!(
                    "Reference image not found or invalid: {}",
                    img.display()
                )));
            }
        }

        // 3. Audio Preservation: Derived strictly from project preservation policy + source media
        let save_audio = project
            .transformation_config
            .preservation
            .preserve_original_audio
            && project
                .source_media
                .as_ref()
                .map(|m| m.has_audio)
                .unwrap_or(false);

        // 4. Resolution Tier
        let resolution_tier = ResolutionTier::from_dimensions(request.resolution)?;

        // 5. Target FPS
        let target_fps = TargetFps::from_f64(request.fps);

        // 6. Prompt
        let instruction_prompt = if request.prompt.trim().is_empty() {
            None
        } else {
            Some(request.prompt.clone())
        };

        Ok(Self {
            provider_key: plan.provider_key.clone(),
            source_video,
            reference_images,
            instruction_prompt,
            resolution_tier,
            target_fps,
            save_audio,
            ignore_audio: false,
            turbo: false,
            disable_safety_checker: false, // ALWAYS false in AutoVideo AI
            seed: None,
        })
    }
}
