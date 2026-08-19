use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridProvenanceMetadata {
    pub source_asset_hash: String,
    pub provider: String,
    pub model: String,
    pub model_version: String,
    pub generation_type: String,
    pub seed: u64,
    pub prompt_hash: String,
    pub input_reference_hashes: Vec<String>,
    pub hardware_profile: String,
    pub timestamp: u64,
    pub cost_estimate: Option<f64>,
    pub actual_cost: Option<f64>,
    pub inference_used: bool,
    pub pipeline_version: String,
    pub zero_fake_verified: bool,
}

impl Default for HybridProvenanceMetadata {
    fn default() -> Self {
        Self {
            source_asset_hash: String::new(),
            provider: "local_engine".to_string(),
            model: "sd15".to_string(),
            model_version: "1.5".to_string(),
            generation_type: "hybrid_keyframe".to_string(),
            seed: 42,
            prompt_hash: String::new(),
            input_reference_hashes: Vec::new(),
            hardware_profile: "adaptive_default".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            cost_estimate: None,
            actual_cost: None,
            inference_used: false,
            pipeline_version: "12.0.0".to_string(),
            zero_fake_verified: true,
        }
    }
}
