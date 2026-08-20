use super::error::CloudProviderError;
use super::job::CloudJobRequest;
use super::provider::{ProviderKey, ResolutionTier, TargetFps};
use super::submission::ValidatedSubmissionPlan;
use super::uploader::UploadedAsset;
use crate::projects::Project;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMediaFacts {
    pub duration_sec: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
}

pub struct SourceMediaProbe;

impl SourceMediaProbe {
    pub fn probe_file(path: &Path) -> Result<SourceMediaFacts, CloudProviderError> {
        if !path.exists() || !path.is_file() {
            return Err(CloudProviderError::RequestInvalid(format!(
                "SOURCE_NOT_FOUND: Source video file not found at {}",
                path.display()
            )));
        }

        let media_service = crate::media::MediaService::new();
        let metadata = std::fs::metadata(path).map_err(|e| {
            CloudProviderError::RequestInvalid(format!(
                "SOURCE_READ_FAILED: Failed to read source metadata: {}",
                e
            ))
        })?;

        if metadata.len() == 0 {
            return Err(CloudProviderError::RequestInvalid(
                "SOURCE_EMPTY: Source video file is empty (0 bytes)".to_string(),
            ));
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("source.mp4");
        let container = path.extension().and_then(|e| e.to_str()).unwrap_or("mp4");

        let probe = media_service
            .probe_with_ffprobe(path, file_name, container, metadata.len())
            .map_err(|e| {
                CloudProviderError::RequestInvalid(format!(
                    "SOURCE_PROBE_FAILED: Failed to probe source media with ffprobe: {}",
                    e
                ))
            })?;

        let duration_sec = probe.duration_ms as f64 / 1000.0;
        if duration_sec <= 0.0 || !duration_sec.is_finite() {
            return Err(CloudProviderError::RequestInvalid(format!(
                "INVALID_DURATION: Probed non-positive or non-finite duration {:.2}s",
                duration_sec
            )));
        }

        if probe.width == 0 || probe.height == 0 {
            return Err(CloudProviderError::RequestInvalid(format!(
                "INVALID_DIMENSIONS: Probed invalid dimensions {}x{}",
                probe.width, probe.height
            )));
        }

        if probe.fps <= 0.0 || !probe.fps.is_finite() {
            return Err(CloudProviderError::RequestInvalid(format!(
                "INVALID_FPS: Probed invalid fps {:.2}",
                probe.fps
            )));
        }

        Ok(SourceMediaFacts {
            duration_sec,
            width: probe.width,
            height: probe.height,
            fps: probe.fps,
            has_audio: probe.has_audio,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundMode {
    Transparent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundRemovalOutputFormat {
    WebmVp9,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundRemovalSpec {
    pub provider_key: ProviderKey,
    pub source_video: PathBuf,
    pub source_facts: SourceMediaFacts,
    pub background_mode: BackgroundMode,
    pub output_format: BackgroundRemovalOutputFormat,
    pub preserve_audio: bool,
}

impl BackgroundRemovalSpec {
    pub fn build(
        request: &CloudJobRequest,
        project: &Project,
        plan: &ValidatedSubmissionPlan,
    ) -> Result<Self, CloudProviderError> {
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
                    "Source video is required for background removal".to_string(),
                ));
            }
        };

        let source_facts = match &plan.source_facts {
            Some(facts) => facts.clone(),
            None => SourceMediaProbe::probe_file(&source_video)?,
        };

        Self::build_with_facts(request, project, plan, source_facts)
    }

    pub fn build_with_facts(
        request: &CloudJobRequest,
        project: &Project,
        plan: &ValidatedSubmissionPlan,
        source_facts: SourceMediaFacts,
    ) -> Result<Self, CloudProviderError> {
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
                    "Source video is required for background removal".to_string(),
                ));
            }
        };

        // Reference check: exactly 0 reference images allowed
        let has_references = request
            .reference_images
            .as_ref()
            .map(|r| !r.is_empty())
            .unwrap_or(false)
            || request.reference_image.is_some();
        if has_references {
            return Err(CloudProviderError::RequestInvalid(
                "UNEXPECTED_REFERENCE_INPUTS_FOR_BACKGROUND_REMOVAL: Background removal requires 0 reference images".to_string(),
            ));
        }

        let preserve_audio = project
            .transformation_config
            .preservation
            .preserve_original_audio
            && source_facts.has_audio;

        Ok(Self {
            provider_key: plan.provider_key.clone(),
            source_video,
            source_facts,
            background_mode: BackgroundMode::Transparent,
            output_format: BackgroundRemovalOutputFormat::WebmVp9,
            preserve_audio,
        })
    }
}

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
pub struct PreparedCharacterReplacement {
    pub spec: ProviderSubmissionSpec,
    pub uploaded_source: UploadedAsset,
    pub uploaded_references: Vec<UploadedAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreparedBackgroundRemoval {
    pub spec: BackgroundRemovalSpec,
    pub uploaded_source: UploadedAsset,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PreparedProviderSubmission {
    CharacterReplacement(PreparedCharacterReplacement),
    BackgroundRemoval(PreparedBackgroundRemoval),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderTaskSpec {
    CharacterReplacement(ProviderSubmissionSpec),
    BackgroundRemoval(BackgroundRemovalSpec),
}

impl ProviderTaskSpec {
    pub fn build(
        request: &CloudJobRequest,
        project: &Project,
        plan: &ValidatedSubmissionPlan,
    ) -> Result<Self, CloudProviderError> {
        match plan.task_class {
            crate::ai::cloud::router::TaskClass::BackgroundRemoval => {
                let spec = BackgroundRemovalSpec::build(request, project, plan)?;
                Ok(ProviderTaskSpec::BackgroundRemoval(spec))
            }
            crate::ai::cloud::router::TaskClass::CharacterReplacement
            | crate::ai::cloud::router::TaskClass::FullGenerativeTransformation => {
                let spec = ProviderSubmissionSpec::build(request, project, plan)?;
                Ok(ProviderTaskSpec::CharacterReplacement(spec))
            }
            other => Err(CloudProviderError::RequestInvalid(format!(
                "TASK_NOT_PREPARABLE: Task class {:?} cannot be prepared for cloud submission",
                other
            ))),
        }
    }
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
