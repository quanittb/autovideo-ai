use super::planner::TransformationIntent;
use super::provenance::HybridProvenanceMetadata;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey(pub String);

impl CacheKey {
    pub fn compute(
        source_asset_hash: &str,
        intent: TransformationIntent,
        prompt: &str,
        negative_prompt: Option<&str>,
        ref_image_hash: Option<&str>,
        provider_id: &str,
        model_id: &str,
        seed: u64,
        resolution: (u32, u32),
        steps: u32,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(source_asset_hash.as_bytes());
        hasher.update(format!("{:?}", intent).as_bytes());
        hasher.update(prompt.as_bytes());
        if let Some(neg) = negative_prompt {
            hasher.update(neg.as_bytes());
        }
        if let Some(r_hash) = ref_image_hash {
            hasher.update(r_hash.as_bytes());
        }
        hasher.update(provider_id.as_bytes());
        hasher.update(model_id.as_bytes());
        hasher.update(seed.to_le_bytes());
        hasher.update(resolution.0.to_le_bytes());
        hasher.update(resolution.1.to_le_bytes());
        hasher.update(steps.to_le_bytes());

        Self(format!("{:x}", hasher.finalize()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationCacheEntry {
    pub key: CacheKey,
    pub generated_frames: Vec<PathBuf>,
    pub generated_video: Option<PathBuf>,
    pub provenance: HybridProvenanceMetadata,
    pub created_timestamp_secs: u64,
}

pub struct GenerationCache {
    entries: RwLock<HashMap<CacheKey, GenerationCacheEntry>>,
}

impl GenerationCache {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<GenerationCacheEntry> {
        let read_guard = self.entries.read().ok()?;
        read_guard.get(key).cloned()
    }

    pub fn insert(&self, key: CacheKey, entry: GenerationCacheEntry) {
        if let Ok(mut write_guard) = self.entries.write() {
            write_guard.insert(key, entry);
        }
    }

    pub fn invalidate(&self, key: &CacheKey) -> bool {
        if let Ok(mut write_guard) = self.entries.write() {
            write_guard.remove(key).is_some()
        } else {
            false
        }
    }

    pub fn clear(&self) {
        if let Ok(mut write_guard) = self.entries.write() {
            write_guard.clear();
        }
    }

    pub fn len(&self) -> usize {
        self.entries.read().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for GenerationCache {
    fn default() -> Self {
        Self::new()
    }
}
