use super::error::CloudProviderError;
use super::manifest::SegmentBoundary;
use super::segment::{SegmentSplitter, SEGMENTATION_POLICY_VERSION, SPLIT_ENCODING_POLICY_VERSION};
use super::spec::{SourceMediaFacts, SourceMediaProbe};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentCacheMeta {
    pub cache_key: String,
    pub source_checksum: String,
    pub start_frame: u64,
    pub end_frame: u64,
    pub segmentation_policy_version: u32,
    pub split_encoding_policy_version: u32,
    pub ffmpeg_fingerprint: String,
    pub source_facts: SourceMediaFacts,
    pub segment_facts: SourceMediaFacts,
    pub created_at: String,
}

pub struct SegmentCacheManager;

impl SegmentCacheManager {
    pub fn compute_file_sha256(path: &Path) -> Result<String, CloudProviderError> {
        let bytes = fs::read(path).map_err(|e| {
            CloudProviderError::RequestInvalid(format!(
                "FAILED_READ_FOR_CHECKSUM: {}: {}",
                path.display(),
                e
            ))
        })?;
        let mut hasher = Sha256::default();
        hasher.update(&bytes);
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub fn compute_split_cache_key(
        source_checksum: &str,
        boundary: &SegmentBoundary,
        ffmpeg_fingerprint: &str,
    ) -> String {
        let mut hasher = Sha256::default();
        hasher.update(source_checksum.as_bytes());
        hasher.update(b":");
        hasher.update(boundary.start_frame.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(boundary.end_frame.to_string().as_bytes());
        hasher.update(b":v");
        hasher.update(SEGMENTATION_POLICY_VERSION.to_string().as_bytes());
        hasher.update(b":v");
        hasher.update(SPLIT_ENCODING_POLICY_VERSION.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(ffmpeg_fingerprint.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn get_cached_split_segment(
        project_dir: &Path,
        cache_key: &str,
        source_checksum: &str,
    ) -> Result<Option<(PathBuf, SourceMediaFacts)>, CloudProviderError> {
        let cache_entry_dir = project_dir
            .join("cache")
            .join("cloud-segments")
            .join(cache_key);
        let segment_path = cache_entry_dir.join("segment.mp4");
        let meta_path = cache_entry_dir.join("cache_meta.json");

        if !segment_path.exists() || !meta_path.exists() {
            return Ok(None);
        }

        let meta_content = match fs::read_to_string(&meta_path) {
            Ok(c) => c,
            Err(_) => {
                let _ = fs::remove_dir_all(&cache_entry_dir);
                return Ok(None);
            }
        };

        let meta: SegmentCacheMeta = match serde_json::from_str(&meta_content) {
            Ok(m) => m,
            Err(_) => {
                let _ = fs::remove_dir_all(&cache_entry_dir);
                return Ok(None);
            }
        };

        if meta.source_checksum != source_checksum || meta.cache_key != cache_key {
            let _ = fs::remove_dir_all(&cache_entry_dir);
            return Ok(None);
        }

        // Authoritative probe of cached segment on disk
        match SourceMediaProbe::probe_file(&segment_path) {
            Ok(facts) => {
                if facts.duration_sec > 0.0 && facts.width > 0 && facts.height > 0 {
                    Ok(Some((segment_path, facts)))
                } else {
                    let _ = fs::remove_dir_all(&cache_entry_dir);
                    Ok(None)
                }
            }
            Err(_) => {
                let _ = fs::remove_dir_all(&cache_entry_dir);
                Ok(None)
            }
        }
    }

    pub fn get_or_create_split_segment(
        project_dir: &Path,
        source_path: &Path,
        source_checksum: &str,
        source_facts: &SourceMediaFacts,
        boundary: &SegmentBoundary,
        fps: f64,
        max_provider_limit_sec: f64,
    ) -> Result<(PathBuf, SourceMediaFacts), CloudProviderError> {
        let ffmpeg_fingerprint = SegmentSplitter::get_ffmpeg_build_fingerprint();
        let cache_key =
            Self::compute_split_cache_key(source_checksum, boundary, &ffmpeg_fingerprint);

        if let Some((path, facts)) =
            Self::get_cached_split_segment(project_dir, &cache_key, source_checksum)?
        {
            return Ok((path, facts));
        }

        let cache_entry_dir = project_dir
            .join("cache")
            .join("cloud-segments")
            .join(&cache_key);
        fs::create_dir_all(&cache_entry_dir).map_err(|e| {
            CloudProviderError::JobFailed(format!(
                "FAILED_CREATE_CACHE_DIR: {}: {}",
                cache_entry_dir.display(),
                e
            ))
        })?;

        let segment_path = cache_entry_dir.join("segment.mp4");
        let segment_facts = SegmentSplitter::split_segment(
            source_path,
            boundary,
            fps,
            &segment_path,
            max_provider_limit_sec,
        )?;

        let meta = SegmentCacheMeta {
            cache_key: cache_key.clone(),
            source_checksum: source_checksum.to_string(),
            start_frame: boundary.start_frame,
            end_frame: boundary.end_frame,
            segmentation_policy_version: SEGMENTATION_POLICY_VERSION,
            split_encoding_policy_version: SPLIT_ENCODING_POLICY_VERSION,
            ffmpeg_fingerprint,
            source_facts: source_facts.clone(),
            segment_facts: segment_facts.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| {
            CloudProviderError::JobFailed(format!("FAILED_SERIALIZE_CACHE_META: {}", e))
        })?;

        let meta_path = cache_entry_dir.join("cache_meta.json");
        fs::write(&meta_path, meta_json).map_err(|e| {
            CloudProviderError::JobFailed(format!(
                "FAILED_WRITE_CACHE_META: {}: {}",
                meta_path.display(),
                e
            ))
        })?;

        Ok((segment_path, segment_facts))
    }
}
