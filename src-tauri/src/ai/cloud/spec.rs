use super::error::CloudProviderError;
use super::job::CloudJobRequest;
use super::provider::{ProviderKey, ResolutionTier, TargetFps};
use super::submission::ValidatedSubmissionPlan;
use super::uploader::UploadedAsset;
use crate::projects::Project;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rational {
    pub num: u32,
    pub den: u32,
}

impl Rational {
    pub fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }

    pub fn to_f64(&self) -> f64 {
        if self.den == 0 {
            0.0
        } else {
            self.num as f64 / self.den as f64
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailedTimingFacts {
    pub r_frame_rate: Rational,
    pub avg_frame_rate: Rational,
    pub time_base: Rational,
    pub is_vfr: bool,
    pub nb_frames: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMediaFacts {
    pub duration_sec: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
    #[serde(default)]
    pub timing: Option<DetailedTimingFacts>,
}

impl Default for SourceMediaFacts {
    fn default() -> Self {
        Self {
            duration_sec: 0.0,
            width: 0,
            height: 0,
            fps: 0.0,
            has_audio: false,
            timing: None,
        }
    }
}

pub struct SourceMediaProbe;

impl SourceMediaProbe {
    pub fn probe_file(path: &Path) -> Result<SourceMediaFacts, CloudProviderError> {
        Self::probe_file_detailed(path).map(|(facts, _)| facts)
    }

    pub fn probe_file_detailed(
        path: &Path,
    ) -> Result<(SourceMediaFacts, DetailedTimingFacts), CloudProviderError> {
        if !path.exists() || !path.is_file() {
            return Err(CloudProviderError::RequestInvalid(format!(
                "SOURCE_NOT_FOUND: Source video file not found at {}",
                path.display()
            )));
        }

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

        let output = std::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,codec_name,width,height,r_frame_rate,avg_frame_rate,time_base,duration,nb_frames,tags:format=duration,format_name",
                "-of",
                "json",
                path.to_str().ok_or_else(|| {
                    CloudProviderError::RequestInvalid("Invalid unicode in file path".to_string())
                })?,
            ])
            .output()
            .map_err(|e| {
                CloudProviderError::RequestInvalid(format!(
                    "SOURCE_PROBE_FAILED: Failed to invoke ffprobe: {}",
                    e
                ))
            })?;

        if !output.status.success() {
            return Err(CloudProviderError::RequestInvalid(format!(
                "SOURCE_PROBE_FAILED: ffprobe returned non-zero exit code: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| {
            CloudProviderError::RequestInvalid(format!(
                "SOURCE_PROBE_FAILED: Failed to parse ffprobe json: {}",
                e
            ))
        })?;

        let streams = parsed.get("streams").and_then(|s| s.as_array());
        let mut width = 0u32;
        let mut height = 0u32;
        let mut has_audio = false;
        let mut stream_duration_sec = 0.0f64;
        let mut r_frame_rate = Rational::new(30, 1);
        let mut avg_frame_rate = Rational::new(30, 1);
        let mut time_base = Rational::new(1, 1000);
        let mut nb_frames: Option<u64> = None;
        let mut video_stream_found = false;

        fn parse_rational(s: &str) -> Option<Rational> {
            let (num_str, den_str) = s.split_once('/')?;
            let num = num_str.parse::<u32>().ok()?;
            let den = den_str.parse::<u32>().ok()?;
            Some(Rational::new(num, den))
        }

        if let Some(stream_list) = streams {
            for stream in stream_list {
                let codec_type = stream
                    .get("codec_type")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default();
                if codec_type == "video" && !video_stream_found {
                    video_stream_found = true;
                    if let Some(w) = stream.get("width").and_then(|v| v.as_u64()) {
                        width = w as u32;
                    }
                    if let Some(h) = stream.get("height").and_then(|v| v.as_u64()) {
                        height = h as u32;
                    }
                    if let Some(dur_str) = stream.get("duration").and_then(|v| v.as_str()) {
                        if let Ok(d) = dur_str.parse::<f64>() {
                            if d > 0.0 {
                                stream_duration_sec = d;
                            }
                        }
                    }
                    if let Some(r_str) = stream.get("r_frame_rate").and_then(|v| v.as_str()) {
                        if let Some(rat) = parse_rational(r_str) {
                            if rat.den > 0 {
                                r_frame_rate = rat;
                            }
                        }
                    }
                    if let Some(avg_str) = stream.get("avg_frame_rate").and_then(|v| v.as_str()) {
                        if let Some(rat) = parse_rational(avg_str) {
                            if rat.den > 0 {
                                avg_frame_rate = rat;
                            }
                        }
                    }
                    if let Some(tb_str) = stream.get("time_base").and_then(|v| v.as_str()) {
                        if let Some(rat) = parse_rational(tb_str) {
                            if rat.den > 0 {
                                time_base = rat;
                            }
                        }
                    }
                    if let Some(nbf_str) = stream.get("nb_frames").and_then(|v| v.as_str()) {
                        if let Ok(nbf) = nbf_str.parse::<u64>() {
                            nb_frames = Some(nbf);
                        }
                    }
                } else if codec_type == "audio" {
                    has_audio = true;
                }
            }
        }

        let mut format_duration_sec = 0.0f64;
        if let Some(format_obj) = parsed.get("format") {
            if let Some(dur_str) = format_obj.get("duration").and_then(|v| v.as_str()) {
                if let Ok(d) = dur_str.parse::<f64>() {
                    format_duration_sec = d;
                }
            }
        }

        let duration_sec = if stream_duration_sec > 0.0 {
            stream_duration_sec
        } else {
            format_duration_sec
        };

        if duration_sec <= 0.0 || !duration_sec.is_finite() {
            return Err(CloudProviderError::RequestInvalid(format!(
                "INVALID_DURATION: Probed non-positive or non-finite duration {:.2}s",
                duration_sec
            )));
        }

        if width == 0 || height == 0 {
            return Err(CloudProviderError::RequestInvalid(format!(
                "INVALID_DIMENSIONS: Probed invalid dimensions {}x{}",
                width, height
            )));
        }

        let fps = r_frame_rate.to_f64();
        if fps <= 0.0 || !fps.is_finite() {
            return Err(CloudProviderError::RequestInvalid(format!(
                "INVALID_FPS: Probed invalid fps {:.2}",
                fps
            )));
        }

        // VFR detection: if r_frame_rate and avg_frame_rate differ significantly or den == 0
        let avg_fps = avg_frame_rate.to_f64();
        let is_vfr = if avg_fps > 0.0 && (fps - avg_fps).abs() > 0.05 {
            true
        } else {
            false
        };

        let timing_facts = DetailedTimingFacts {
            r_frame_rate,
            avg_frame_rate,
            time_base,
            is_vfr,
            nb_frames,
        };

        let source_facts = SourceMediaFacts {
            duration_sec,
            width,
            height,
            fps,
            has_audio,
            timing: Some(timing_facts.clone()),
        };

        Ok((source_facts, timing_facts))
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
        let source_facts = plan.source_facts.clone().ok_or_else(|| {
            CloudProviderError::RequestInvalid(
                "SOURCE_FACTS_REQUIRED: Background removal requires pre-probed source facts in submission plan".to_string(),
            )
        })?;

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
